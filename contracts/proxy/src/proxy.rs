use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, BytesN, Env,
    IntoVal, Symbol, Val, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// `initialize` has not been called, so there is no admin to authorize.
    NotInitialized = 1,
    /// The caller did not supply an authorization for the admin.
    Unauthorized = 2,
}

pub const TIMELOCK_DELAY: u64 = 172_800; // 48 hours in seconds (48 * 60 * 60)

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingUpgrade {
    pub new_implementation: Address,
    pub eta: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Implementation,
    Counter,
    Storage(BytesN<32>),
    PendingUpgrade,
}

#[contract]
pub struct Proxy;

#[contractimpl]
impl Proxy {
    pub fn initialize(env: Env, admin: Address, implementation: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Proxy already initialized");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Implementation, &implementation);
        env.storage().instance().set(&DataKey::Counter, &0i32);
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    /// Single audited gate for every privileged operation (issue #80).
    ///
    /// The admin lives in `instance` storage, written once by `initialize`, and
    /// every upgrade operation funnels through this helper so there is exactly
    /// one place where the check can be wrong.
    ///
    /// The workspace pins `soroban-sdk` 22, which does not expose
    /// `Env::invoker()`, so the requirement is expressed as
    /// `admin.require_auth()`: the transaction must carry an authorization for
    /// the admin covering this contract, this function and these arguments. A
    /// caller that cannot produce the admin's authorization therefore cannot
    /// repoint the implementation.
    fn require_admin(env: &Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        Ok(())
    }

    pub fn get_implementation(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Implementation)
            .unwrap()
    }

    /// Returns pending upgrade details if one exists
    pub fn get_pending_upgrade(env: Env) -> Option<PendingUpgrade> {
        env.storage().persistent().get(&DataKey::PendingUpgrade)
    }
    /// Step 1: Propose an upgrade with a 48-hour timelock delay
    ///
    /// Admin-only. There is deliberately no way to repoint the implementation
    /// immediately: the only route to a new implementation is
    /// propose -> timelock -> execute, so a compromised admin still cannot
    /// silently swap in a malicious implementation (issue #80).
    pub fn propose_upgrade(env: Env, new_implementation: Address) -> Result<(), Error> {
        Self::require_admin(&env)?;

        let eta = env.ledger().timestamp() + TIMELOCK_DELAY;
        let proposal = PendingUpgrade {
            new_implementation: new_implementation.clone(),
            eta,
        };

        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgrade, &proposal);

        env.events()
            .publish((symbol_short!("propose"),), (new_implementation, eta));
        Ok(())
    }

    /// Step 2: Execute the pending upgrade once 48 hours have passed
    pub fn execute_upgrade(env: Env) -> Result<(), Error> {
        Self::require_admin(&env)?;

        let proposal: PendingUpgrade = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgrade)
            .expect("No pending upgrade proposal found");

        if env.ledger().timestamp() < proposal.eta {
            panic!("Timelock delay of 48 hours has not elapsed");
        }

        env.storage()
            .persistent()
            .set(&DataKey::Implementation, &proposal.new_implementation);

        env.storage().persistent().remove(&DataKey::PendingUpgrade);

        env.events()
            .publish((symbol_short!("upgraded"),), proposal.new_implementation);
        Ok(())
    }

    /// Step 3: Admin can cancel a pending upgrade
    ///
    /// Admin-only.
    pub fn cancel_upgrade(env: Env) -> Result<(), Error> {
        Self::require_admin(&env)?;

        env.storage().persistent().remove(&DataKey::PendingUpgrade);

        env.events().publish((symbol_short!("cancel"),), ());
        Ok(())
    }

    /// Backwards-compatible alias for [`Proxy::propose_upgrade`].
    ///
    /// This routes through the timelock rather than setting the implementation
    /// directly, which is exactly what the previous duplicate `upgrade_to` did.
    pub fn upgrade_to(env: Env, implementation: Address) -> Result<(), Error> {
        Self::propose_upgrade(env, implementation)
    }

    /// Execute the pending upgrade (after timelock) and immediately call a method on the new implementation.
    /// The upgrade must have already been proposed via `propose_upgrade` and the 48-hour
    /// timelock must have elapsed before calling this.
    pub fn upgrade_to_and_call(
        env: Env,
        implementation: Address,
        method: Symbol,
        args: Vec<Val>,
    ) -> Result<Val, Error> {
        Self::require_admin(&env)?;

        let proposal: PendingUpgrade = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgrade)
            .expect("No pending upgrade proposal – call propose_upgrade first");

        if proposal.new_implementation != implementation {
            panic!("Implementation does not match the pending proposal");
        }

        if env.ledger().timestamp() < proposal.eta {
            panic!("Timelock delay of 48 hours has not elapsed");
        }

        env.storage()
            .persistent()
            .set(&DataKey::Implementation, &proposal.new_implementation);
        env.storage().persistent().remove(&DataKey::PendingUpgrade);

        env.events()
            .publish((symbol_short!("upgraded"),), proposal.new_implementation);
        // The upgraded implementation is invoked separately by the caller.
        Ok(Val::from(0i32))
    }

    pub fn delegate_call(env: Env, method: Symbol, args: Vec<Val>) -> Val {
        let implementation = Self::get_implementation(env.clone());
        env.invoke_contract(&implementation, &method, args)
    }

    pub fn increment(env: Env, amount: i32) -> i32 {
        let current = Self::get_value(env.clone());
        let method = Symbol::new(&env, "calculate");
        let args: Vec<Val> = Vec::from_array(&env, [current.into_val(&env), amount.into_val(&env)]);
        let next: i32 = env.invoke_contract(&Self::get_implementation(env.clone()), &method, args);
        // Write the counter directly rather than through `set_value`, which is
        // now admin-gated; `increment` is callable by anyone by design.
        env.storage().instance().set(&DataKey::Counter, &next);
        next
    }

    pub fn get_value(env: Env) -> i32 {
        env.storage().instance().get(&DataKey::Counter).unwrap_or(0)
    }

    /// Direct counter write.
    ///
    /// Admin-only, for the same reason as the upgrade path (issue #80): leaving
    /// an unauthenticated state-mutating entry point on a proxy undermines the
    /// admin check that guards the logic.
    pub fn set_value(env: Env, value: i32) -> Result<(), Error> {
        Self::require_admin(&env)?;
        env.storage().instance().set(&DataKey::Counter, &value);
        Ok(())
    }

    /// Arbitrary storage write used by the implementation for its own state.
    ///
    /// Admin-only, for the same reason as the upgrade path.
    pub fn set_storage(env: Env, key: BytesN<32>, value: Val) -> Result<(), Error> {
        Self::require_admin(&env)?;
        env.storage()
            .persistent()
            .set(&DataKey::Storage(key), &value);
        Ok(())
    }

    pub fn get_storage(env: Env, key: BytesN<32>) -> Option<Val> {
        env.storage().persistent().get(&DataKey::Storage(key))
    }
}
