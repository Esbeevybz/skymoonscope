use crate::admin::{get_security_council, read_administrator, read_config, set_security_council, write_administrator, write_config};
use crate::compare::compare_proposals;
use crate::proposal::{cancel_proposal, create_proposal, execute_proposal, queue_proposal, read_proposal, start_voting, veto_proposal};
use crate::storage_types::{GovernanceConfig, Proposal, ProposalAction, ProposalComparison};
use crate::voting::{delegate_voting_power, get_effective_voting_power, read_voting_power, vote, write_voting_power};
use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};

pub trait GovernanceTrait {
    // Initialization
    fn initialize(e: Env, admin: Address, voting_period: u64, timelock_delay: u64, quorum_percentage: u32);

    // Admin functions
    fn set_admin(e: Env, new_admin: Address);
    fn set_voting_power(e: Env, user: Address, power: i128);
    fn cancel_proposal_admin(e: Env, proposal_id: u32);
    fn set_security_council(e: Env, security_council: Address);

    // Proposal lifecycle
    fn create_proposal(e: Env, title: String, description: String, actions: Vec<ProposalAction>) -> u32;
    fn start_voting(e: Env, proposal_id: u32);
    fn cast_vote(e: Env, proposal_id: u32, support: bool);
    fn queue_proposal(e: Env, proposal_id: u32);
    fn execute_proposal(e: Env, proposal_id: u32);
    fn veto_proposal(e: Env, proposal_id: u32);

    // Delegation
    fn delegate(e: Env, delegate: Address);

    // Queries
    fn get_proposal(e: Env, proposal_id: u32) -> Proposal;
    fn get_voting_power(e: Env, user: Address) -> i128;
    fn get_effective_voting_power(e: Env, user: Address) -> i128;
    fn get_config(e: Env) -> GovernanceConfig;
    fn get_security_council(e: Env) -> Option<Address>;

    // Side-by-side comparison of two proposal versions
    fn compare_proposal_versions(e: Env, left_id: u32, right_id: u32) -> ProposalComparison;
}

#[contract]
pub struct Governance;

#[contractimpl]
impl GovernanceTrait for Governance {
    fn initialize(e: Env, admin: Address, voting_period: u64, timelock_delay: u64, quorum_percentage: u32) {
        let config = GovernanceConfig {
            admin,
            voting_period,
            timelock_delay,
            quorum_percentage,
            proposal_count: 0,
        };
        write_config(&e, &config);
    }

    fn set_admin(e: Env, new_admin: Address) {
        let admin = read_administrator(&e);
        admin.require_auth();
        write_administrator(&e, &new_admin);
    }

    fn set_voting_power(e: Env, user: Address, power: i128) {
        let admin = read_administrator(&e);
        admin.require_auth();
        write_voting_power(&e, &user, power);
    }

    fn cancel_proposal_admin(e: Env, proposal_id: u32) {
        let admin = read_administrator(&e);
        admin.require_auth();
        cancel_proposal(&e, proposal_id);
    }

    fn set_security_council(e: Env, security_council: Address) {
        let admin = read_administrator(&e);
        admin.require_auth();
        set_security_council(&e, &security_council);
    }

    fn create_proposal(e: Env, title: String, description: String, actions: Vec<ProposalAction>) -> u32 {
        let proposer = e.invoker();
        create_proposal(&e, proposer, title, description, actions)
    }

    fn start_voting(e: Env, proposal_id: u32) {
        let admin = read_administrator(&e);
        admin.require_auth();
        start_voting(&e, proposal_id);
    }

    fn cast_vote(e: Env, proposal_id: u32, support: bool) {
        let voter = e.invoker();
        vote(&e, proposal_id, voter, support);
    }

    fn queue_proposal(e: Env, proposal_id: u32) {
        queue_proposal(&e, proposal_id);
    }

    fn execute_proposal(e: Env, proposal_id: u32) {
        execute_proposal(&e, proposal_id);
    }

    fn veto_proposal(e: Env, proposal_id: u32) {
        let security_council = e.invoker();
        security_council.require_auth();
        veto_proposal(&e, security_council, proposal_id);
    }

    fn delegate(e: Env, delegate: Address) {
        let delegator = e.invoker();
        delegate_voting_power(&e, delegator, delegate);
    }

    fn get_proposal(e: Env, proposal_id: u32) -> Proposal {
        read_proposal(&e, proposal_id)
    }

    fn get_voting_power(e: Env, user: Address) -> i128 {
        read_voting_power(&e, &user)
    }

    fn get_effective_voting_power(e: Env, user: Address) -> i128 {
        get_effective_voting_power(&e, &user)
    }

    fn get_config(e: Env) -> GovernanceConfig {
        read_config(&e)
    }

    fn get_security_council(e: Env) -> Option<Address> {
        get_security_council(&e)
    }

    fn compare_proposal_versions(e: Env, left_id: u32, right_id: u32) -> ProposalComparison {
        compare_proposals(&e, left_id, right_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk:testutils::Address as _;
    use soroban_sdk:Env;
    use crate::storage_types::ProposalAction;
    use soroban_sdk::String as _;
    use soroban_sdk:Vec as _;

    fn setup(env: &Env) -> (GovernanceClient, Address) {
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Governance);
        let client = GovernanceClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin, &100, &3600, &10);
        (client, admin)
    }

    fn create_test_proposal(client: &GovernanceClient, env: &Env, title: &str, desc: &str) -> u32 {
        let title = String::from_str(env, title);
        let description = String::from_str(env, desc);
        let actions: Vec<ProposalAction> = Vec::new(env);
        client.create_proposal(&title, &description, &actions)
    }

    fn create_test_proposal_with_actions(
        client: &GovernanceClient,
        env: &Env,
        title: &str,
        desc: &str,
        actions: Vec<ProposalAction>,
    ) -> u32 {
        let title = String::from_str(env, title);
        let description = String::from_str(env, desc);
        client.create_proposal(&title, &description, &actions)
    }

    #[test]
    fn test_compare_identical_proposals() {
        let env = Env::default();
        let (client, _admin) = setup(&env);
        let id = create_test_proposal(&client, &env, "Title", "Description");
        let comparison = client.compare_proposal_versions(&id, &id);
        assert!(comparison.identical);
        assert_eq!(comparison.diffs.len(), 0);
        assert_eq!(comparison.left_id, id);
        assert_eq!(comparison.right_id, id);
    }

    [test]
    fn test_compare_different_title_and_description() {
        let env = Env::default();
        let (client, _admin) = setup(&env);
        let left_id = create_test_proposal(&client, &env, "Old Title", "Old Desc");
        let right_id = create_test_proposal(&client, &env, "New Title", "New Desc");
        let comparison = client.compare_proposal_versions(&left_id, &right_id);
        assert!(!comparison.identical);
        assert_eq!(comparison.diffs.len(), 2);
        let mut saw_title = false;
        let mut saw_desc = false;
        for diff in comparison.diffs.iter() {
            if diff.field == String::from_str(&env, "title") {
                saw_title = true;
                assert_eq!(diff.left, String::from_str(&env, "Old Title"));
                assert_eq!(diff.right, String::from_str(&env, "New Title"));
            } else if diff.field == String::from_str(&env, "description") {
                saw_desc = true;
                assert_eq!(diff.left, String::from_str(&env, "Old Desc"));
                assert_eq!(diff.right, String::from_str(&env, "New Desc"));
            }
        }
        assert!(saw_title);
        assert!(saw_desc);
    }

    [test]
    fn test_compare_detects_state_change() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let id = create_test_proposal(&client, &env, "Title", "Desc");
        // Create a second proposal and move it to the Voting state.
        let id_voting = create_test_proposal(&client, &env, "Title", "Desc");
        client.start_voting(&admin, &id_voting);
        let comparison = client.compare_proposal_versions(&id, &id_voting);
        assert!(!comparison.identical);
        let mut saw_state = false;
        for diff in comparison.diffs.iter() {
            if diff.field == String::from_str(&env, "state") {
                saw_state = true;
                assert_eq!(diff.left, String::from_str(&env, "Draft"));
                assert_eq!(diff.right, String::from_str(&env, "Voting"));
            }
        }
        assert!(saw_state);
    }

    [test]
    fn test_compare_detects_action_change() {
        let env = Env::default();
        let (client, _admin) = setup(&env);
        let left_id = create_test_proposal(&client, &env, "Title", "Desc");

        let mut actions: Vec<ProposalAction> = Vec::new(&env);
        actions.push_back(ProposalAction {
            contract_id: Address::generate(&env),
            function_name: String::from_str(&env, "transfer"),
            args: Vec::new(&env),
        });
        let right_id = create_test_proposal_with_actions(&client, &env, "Title", "Desc", actions);

        let comparison = client.compare_proposal_versions(&left_id, &right_id);
        assert!(!comparison.identical);
        let mut saw_actions = false;
        for diff in comparison.diffs.iter() {
            if diff.field == String::from_str(&env, "actions") {
                saw_actions = true;
                assert_eq!(diff.left, String::from_str(&env, "[]"));
                assert!(diff.right.len() > 2);
            }
        }
        assert!(saw_actions);
    }

    [test]
    fn test_compare_rejects_missing_proposal() {
        let env = Env::default();
        let (client, _admin) = setup(&env);
        let id = create_test_proposal(&client, &env, "Title", "Desc");
        let result = std::panic::catch_unwind(assert_unwind_safe(|| {
            client.compare_proposal_versions(&id, &9999);
        }));
        assert!(result.is_error());
    }

    fn assert_unwind_safe<F, R/>(f: F) -> R
where
        F: Function() -> R + UnwindSafe,
    {
        f()
    }

    [test]
    fn test_double_voting_prevention() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Governance);
        let client = GovernanceClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let voter_a = Address::generate(&env);
        let voter_b = Address::generate(&env);

        client.initialize(&admin, &100, &3600, &10);

        client.set_voting_power(&&admin, &voter_a, &100);
        client.set_voting_power(&admin, &voter_b, &0);

        let title = String::from_str(&env, "Title");
        let description = String::from_str(&env, "Description");
        let actions: Vec<ProposalAction> = Vec::new(&env);
        let proposal_id = client.create_proposal(&title, &description, &actions);

        client.start_voting(&admin, &proposal_id);
        client.cast_vote(&proposal_id, &voter_a, &true);

        // Simulate token transfer after proposal creation
        client.set_voting_power(&admin, &voter_a, &0);
        client.set_voting_power(&&admin, &voter_b, &100);

        // B tries to vote, should panic because snapshot power is 0
        let result = std::panic::catch_unwind(assert_unwind_safe(|| {
            client.cast_vote(&proposal_id, &voter_b, &true);
        }));
        assert!(result.is_error());
    }
}
