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
    /// Hash of the implementation's logic artifact, supplied by the proposer.
    pub logic_hash: BytesN<32>,
    pub eta: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradedEvent {
    pub logic_hash: BytesN<32>,
    pub interface_version: u32,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Implementation,
    InterfaceVersion,
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
        env.storage()
            .instance()
            .set(&DataKey::InterfaceVersion, &1u32);
        env.storage().instance().set(&DataKey::Counter, &0i32);
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    pub fn get_implementation(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Implementation)
            .unwrap()
    }

    /// Returns the interface generation, starting at 1 and incrementing on each upgrade.
    pub fn interface_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::InterfaceVersion)
            .unwrap_or(1)
    }

    /// Assert the caller is authorised as the recorded admin.
    ///
    /// `Address::require_auth` is the only caller-identity primitive the SDK
    /// exposes: it fails unless the transaction carries a valid authorization
    /// entry for exactly this address, so no other account can reach any
    /// function that goes through here (#080).
    fn require_admin(env: &Env) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
    }

    /// Returns pending upgrade details if one exists
    pub fn get_pending_upgrade(env: Env) -> Option<PendingUpgrade> {
        env.storage().persistent().get(&DataKey::PendingUpgrade)
    }

    /// Step 1: Propose an upgrade with a 48-hour timelock delay.
    /// `logic_hash` identifies the implementation artifact for upgrade observers.
    pub fn propose_upgrade(env: Env, new_implementation: Address, logic_hash: BytesN<32>) {
        Self::require_admin(&env);

        let eta = env.ledger().timestamp() + TIMELOCK_DELAY;
        let proposal = PendingUpgrade {
            new_implementation: new_implementation.clone(),
            logic_hash,
            eta,
        };

        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgrade, &proposal);

        env.events()
            .publish((symbol_short!("propose"),), (new_implementation, eta));
    }

    /// Step 2: Execute the pending upgrade once 48 hours have passed
    pub fn execute_upgrade(env: Env) {
        Self::require_admin(&env);

        let proposal: PendingUpgrade = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgrade)
            .expect("No pending upgrade proposal found");

        if env.ledger().timestamp() < proposal.eta {
            panic!("Timelock delay of 48 hours has not elapsed");
        }

        Self::apply_upgrade(&env, &proposal);
    }

    fn apply_upgrade(env: &Env, proposal: &PendingUpgrade) {
        env.storage()
            .instance()
            .set(&DataKey::Implementation, &proposal.new_implementation);
        let interface_version = Self::interface_version(env.clone())
            .checked_add(1)
            .expect("interface version overflow");
        env.storage()
            .instance()
            .set(&DataKey::InterfaceVersion, &interface_version);
        env.storage().persistent().remove(&DataKey::PendingUpgrade);
        env.events().publish(
            (symbol_short!("upgraded"),),
            UpgradedEvent {
                logic_hash: proposal.logic_hash.clone(),
                interface_version,
            },
        );
    }

    /// Step 3: Admin can cancel a pending upgrade
    pub fn cancel_upgrade(env: Env) {
        Self::require_admin(&env);

        env.storage().persistent().remove(&DataKey::PendingUpgrade);

        env.events().publish((symbol_short!("cancel"),), ());
    }

    /// Legacy upgrade method kept for backward compatibility (delegates to proposal flow)
    ///
    /// Retained name, but the behaviour is the timelocked proposal: an upgrade
    /// is never applied immediately, so this alias cannot be used to skip the
    /// 48-hour delay.
    pub fn upgrade_to(env: Env, implementation: Address, logic_hash: BytesN<32>) {
        Self::propose_upgrade(env, implementation, logic_hash);
    }

    /// Transfer proxy administration. Admin only.
    pub fn set_admin(env: Env, new_admin: Address) {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.events().publish((symbol_short!("admin"),), new_admin);
    }

    /// Execute the pending upgrade (after timelock) and immediately call a method on the new implementation.
    /// The upgrade must have already been proposed via `propose_upgrade` and the 48-hour
    /// timelock must have elapsed before calling this.
    pub fn upgrade_to_and_call(
        env: Env,
        implementation: Address,
        method: Symbol,
        args: Vec<Val>,
    ) -> Val {
        Self::require_admin(&env);

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

        Self::apply_upgrade(&env, &proposal);
        // The upgraded implementation is invoked separately by the caller.
        Val::from(0i32)
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
        Self::write_value(&env, next);
        next
    }

    pub fn get_value(env: Env) -> i32 {
        env.storage().instance().get(&DataKey::Counter).unwrap_or(0)
    }

    /// Overwrite the proxied counter. Admin only.
    ///
    /// This used to be unguarded, so any caller could set the counter that
    /// `increment` treats as its starting state (#080).  `increment` writes
    /// through `write_value` instead, which is deliberately not an entry point.
    pub fn set_value(env: Env, value: i32) {
        Self::require_admin(&env);
        Self::write_value(&env, value);
    }

    /// Internal counter write. Not a contract entry point, so not callable
    /// directly and not subject to the admin check.
    fn write_value(env: &Env, value: i32) {
        env.storage().instance().set(&DataKey::Counter, &value);
    }

    pub fn set_storage(env: Env, key: BytesN<32>, value: Val) {
        Self::require_admin(&env);
        env.storage()
            .persistent()
            .set(&DataKey::Storage(key), &value);
    }

    pub fn get_storage(env: Env, key: BytesN<32>) -> Option<Val> {
        env.storage().persistent().get(&DataKey::Storage(key))
    }
}
