use crate::admin::{get_security_council, read_administrator, read_config, set_security_council, write_administrator, write_config};
use crate::proposal::{cancel_proposal, create_proposal, execute_proposal, queue_proposal, read_proposal, start_voting, veto_proposal};
use crate::storage_types::{GovernanceConfig, Proposal, ProposalAction, ProposalComparison, ProposalState};
use crate::voting::{delegate_voting_power, get_effective_voting_power, read_voting_power, vote, write_voting_power};
use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};

pub trait GovernanceTrait {
    // Initialization
    fn initialize(e: Env, admin: Address, total_supply: i128, voting_period: u64, timelock_delay: u64, quorum_percentage: u32);

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

    // Side-by-side comparison of two contract versions
    fn compare_proposals(e: Env, proposal_id_a: u32, proposal_id_b: u32) -> ProposalComparison;
}

#[contract]
pub struct Governance;

#[contractimpl]
impl GovernanceTrait for Governance {
    fn initialize(e: Env, admin: Address, total_supply: i128, voting_period: u64, timelock_delay: u64, quorum_percentage: u32) {
        let config = GovernanceConfig {
            admin,
            voting_period,
            timelock_delay,
            quorum_percentage,
            proposal_count: 0,
        };
        write_config(&e, &config);
        crate::admin::write_total_supply(&e, total_supply);
    }

    fn set_admin(e: Env, new_admin: Address) {
        let admin = read_administrator(&e);
        admin.require_auth();
        write_administrator(&e, &new_admin);
    }

    fn set_voting_power(e: Env, user: Address, power: i128) {
        let admin = read_administrator(&e);
        admin.require_auth();
        write_voting_power(&e, &user, &power);
    }

    fn cancel_proposal_admin(e: Env, proposal_id: u32) {
        let admin = read_administrator(&e);
        admin.require_auth();
        cancel_proposal(&e, &proposal_id);
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

    fn compare_proposals(e: Env, proposal_id_a: u32, proposal_id_b: u32) -> ProposalComparison {
        let a = read_proposal(&e, proposal_id_a);
        let b = read_proposal(&e, proposal_id_b);

        let title_changed = a.title != b.title;
        let description_changed = a.description != b.description;
        let actions_changed = a.actions != b.actions;
        let state_changed = a.state != b.state;

        ProposalComparison {
            equal: !title_changed
                && !description_changed
                && !actions_changed
                && !state_changed,
            title_changed,
            description_changed,
            actions_changed,
            state_changed,
            actions_a: a.actions.len(),
            actions_b: b.actions.len(),
            for_votes_delta: b.for_votes - a.for_votes,
            against_votes_delta: b.against_votes - a.against_votes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk:Env;
    use crate::storage_types::ProposalAction;
    use soroban_sdk::String as _;
    use soroban_sdk:Vec as _;

    fn setup(env: &Env) -> (GovernanceClient, Address) {
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Governance);
        let client = GovernanceClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin, &1000, &100, &3600, &10);
        (client, admin)
    }

    #itest]
    fn test_compare_identical_proposals {
        let env = Env::default();
        let (client, _admin) = setup(&env);
        let title = String::from_str(&env, "Title");
        let description = String::from_str(&env, "Description");
        let actions: Vec<ProposalAction> = Vec::new(&env);
        let id = client.create_proposal(&title, &description, &actions);

        let cmp = client.compare_proposals(&id, &id);
        assert!(cmp.equal);
        assert!(!cmp.title_changed);
        assert!(!cmp.description_changed);
        assert!(!cmp.actions_changed);
        assert!(!cmp.state_changed);
        assert_eq!(cmp.for_votes_delta, 0);
        assert_eq!(cmp.against_votes_delta, 0);
    }

    #itest]
    fn test_compare_different_proposals {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let v1 = Address::generate(&env);
        client.set_voting_power(&admin, &v1, &100);

        let title_a = String::from_str(&env, "Title A");
        let title_b = String::from_str(&env, "Title B");
        let desc = String::from_str(&env, "Description");
        let actions: Vec<ProposalAction> = Vec::new(&env);
        let id_a = client.create_proposal(&title_a, &desc, &actions);
        let id_b = client.create_proposal(&title_b, &desc, &actions);

        client.start_voting(&admin, &id_b);
        client.cast_vote(&id_b, &v1, &true);

        let cmp = client.compare_proposals(&id_a, &id_b);
        assert!(!cmp.equal);
        assert!(cmp.title_changed);
        assert!(!cmp.description_changed);
        assert!(!cmp.actions_changed);
        assert!(cmp.state_changed);
        assert_eq!(cmp.for_votes_delta, 100);
        assert_eq!(cmp.actions_a, 0);
        assert_eq!(cmp.actions_b, 0);
    }

    #itest()
    fn test_compare_actions_changed {
        let env = Env::default();
        let (client, _admin) = setup(&env);
        let title = String::from_str(&env, "Title");
        let desc = String::from_str(&env, "Description");
        let empty: Vec<ProposalAction> = Vec::new(&env);
        let id_a = client.create_proposal(&title, &desc, &empty);

        let target = Address::generate(&env);
        let fn = String::from_str(&env, "transfer");
        let args: Vec<String> = Vec::new(&env);
        let action = ProposalAction { contract_id: target, function_name: fn, args };
        let mut one: Vec<ProposalAction> = Vec::new(&env);
        one.push_back(action);
        let id_b = client.create_proposal(&title, &desc, &one);

        let cmp = client.compare_proposals(&id_a, &id_b);
        assert!(!cmp.equal);
        assert!(cmp.actions_changed);
        assert_eq!(cmp.actions_a, 0);
        assert_eq!(cmp.actions_b, 1);
    }

    #itest()
    fn test_compare_missing_proposal_panics {
        let env = Env::default();
        let (client, _admin) = setup(&env);
        let title = String::from_str(&env, "Title");
        let desc = String::from_str(&env, "Description");
        let actions: Vec<ProposalAction> = Vec::new(&env);
        let id = client.create_proposal(&title, &desc, &actions);

        let result = std::panic::catch_unwind(|| client.compare_proposals(&id, &9999));
        assert!(result.is_err());
    }

    #test]
    fn test_double_voting_prevention() {
        let env = Env::default();
        let (client, admin) = setup(&env);
        let voter_a = Address::generate(&env);
        let voter_b = Address::generate(&env);

        client.set_voting_power(&admin, &voter_a, &100);
        client.set_voting_power(&admin, &voter_b, &0);

        let title = String::from_str(&env, "Title");
        let description = String::from_str(&env, "Description");
        let actions: Vec<ProposalAction> = Vec::new(&env);
        let proposal_id = client.create_proposal(&title, &description, &actions);

        client.start_voting(&admin, &proposal_id);
        client.cast_vote(&proposal_id, &voter_a, &true);

        // Simulate token transfer after proposal creation
        client.set_voting_power(&admin, &voter_a, &0);
        client.set_voting_power(&admin, &voter_b, &100);

        // B tries to vote, should panic because snapshot power is 0
        let result = std::panic::catch_unwind(|| client.cast_vote(&proposal_id, &voter_b, &true));
        assert!(result.is_err());
    }
}
