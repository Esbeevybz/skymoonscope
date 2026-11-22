use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

const UPGRADE_DELAY_LEDGERS: u32 = 24;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    PendingUpgrade,
    Implementation,
}

#[contracttype]
#[derive(Clone)]
pub struct PendingUpgrade {
    pub new_implementation: Symbol,
    pub proposal_ledger: u32,
}

#[contract]
pub struct ProxyLogicV2;

#[contractimpl]
impl ProxyLogicV2 {
    pub fn calculate(_env: Env, a: i32, b: i32) -> i32 {
        a + b + 10
    }

    pub fn version(_env: Env) -> u32 {
        2
    }

    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    pub fn set_admin(env: Env, new_admin: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    pub fn propose_upgrade(env: Env, new_implementation: Symbol) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();

        let proposal = PendingUpgrade {
            new_implementation,
            proposal_ledger: env.ledger().sequence(),
        };
        env.storage()
            .instance()
            .set(&DataKey::PendingUpgrade, &proposal);
    }

    pub fn execute_upgrade(env: Env) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();

        let proposal: PendingUpgrade = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgrade)
            .expect("no pending upgrade");

        let current_ledger = env.ledger().sequence();
        let earliest_execution = proposal
            .proposal_ledger
            .checked_add(UPGRADE_DELAY_LEDGERS)
            .expect("ledger overflow");
        if current_ledger < earliest_execution {
            panic!("upgrade time-lock not elapsed");
        }

        env.storage()
            .instance()
            .set(&DataKey::Implementation, &proposal.new_implementation);
        env.storage().instance().remove(&DataKey::PendingUpgrade);
    }

    pub fn get_implementation(env: Env) -> Option<Symbol> {
        env.storage().instance().get(&DataKey::Implementation)
    }

    pub fn get_pending_upgrade(env: Env) -> Option<PendingUpgrade> {
        env.storage().instance().get(&DataKey::PendingUpgrade)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{symbol_short, Env};

    fn setup() -> (Env, Address, ProxyLogicV2Client<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ProxyLogicV2, ());
        let client = ProxyLogicV2Client::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, admin, client)
    }

    #[test]
    fn test_early_execution_rejected() {
        let (env, _admin, client) = setup();
        env.ledger().set_sequence_number(100);

        let new_impl = symbol_short!("impl_v3");
        client.propose_upgrade(&new_impl);

        // Advance fewer than 24 ledgers.
        env.ledger().set_sequence_number(100 + 23);

        let result = client.try_execute_upgrade();
        assert!(result.is_err());

        // Pending proposal should still be present.
        let pending = client.get_pending_upgrade();
        assert!(pending.is_some());
        assert!(client.get_implementation().is_none());
    }

    #[test]
    fn test_execution_after_delay_succeeds() {
        let (env, _admin, client) = setup();
        env.ledger().set_sequence_number(100);

        let new_impl = symbol_short!("impl_v3");
        client.propose_upgrade(&new_impl);

        // Advance exactly 24 ledgers.
        env.ledger().set_sequence_number(100 + 24);

        client.execute_upgrade();

        assert_eq!(client.get_implementation(), Some(new_impl));
        assert!(client.get_pending_upgrade().is_none());
    }

    #[test]
    fn test_execution_after_longer_delay_succeeds() {
        let (env, _admin, client) = setup();
        env.ledger().set_sequence_number(50);

        let new_impl = symbol_short!("impl_v4");
        client.propose_upgrade(&new_impl);

        env.ledger().set_sequence_number(50 + 100);

        client.execute_upgrade();

        assert_eq!(client.get_implementation(), Some(new_impl));
        assert!(client.get_pending_upgrade().is_none());
    }

    #[test]
    fn test_execute_without_proposal_fails() {
        let (env, _admin, client) = setup();
        env.ledger().set_sequence_number(100);

        let result = client.try_execute_upgrade();
        assert!(result.is_err());
    }
}