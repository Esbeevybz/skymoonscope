use crate::admin::{get_security_council, read_administrator, read_config, set_security_council, write_administrator, write_config};
use crate::proposal::{cancel_proposal, create_proposal, execute_proposal, queue_proposal, read_proposal, start_voting, veto_proposal};
use crate::storage_types::{GovernanceConfig, Proposal, ProposalAction, ProposalComparison, ProposalState};
use crate::voting::{delegate_voting_power, get_effective_voting_power, read_voting_power, vote, write_voting_power};
use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};

/// Stores the full proposal record for a given proposal id and version.
pub fn read_proposal_version(e: &Env, proposal_id: u32, version: u32) -> Proposal {
    e.storage()
        .persistent()
        .get(&crate::storage_types::DataKey::ProposalVersion(proposal_id, version))
        .expect("proposal version not found")
}

/// Persists a proposal snapshot under a given version number.
pub fn write_proposal_version(e: &Env, proposal_id: u32, version: u32, proposal: &Proposal) {
    e.storage().persistent().set(
        &crate::storage_types::DataKey::ProposalVersion(proposal_id, version),
        proposal,
    );
}

fn actions_equal(a: &ProposalAction, b: &ProposalAction) -> bool {
    a.contract_id == b.contract_id && a.function_name == b.function_name && a.args == b.args
}

fn actions_contains(actions: &Vec<ProposalAction>, target: &ProposalAction) -> bool {
    for action in actions.iter() {
        if actions_equal(&action, target) {
            return true;
        }
    }
    false
}

/// Builds a structured side-by-side comparison between two proposal versions.
pub fn compare_proposal_versions(
    e: &Env,
    proposal_id: u32,
    version_a: u32,
    version_b: u32,
) -> ProposalComparison {
    let a: Proposal = read_proposal_version(e, proposal_id, version_a);
    let b: Proposal = read_proposal_version(e, proposal_id, version_b);

    let title_changed = a.title != b.title;
    let description_changed = a.description != b.description;
    let state_changed = a.state != b.state;
    let for_votes_changed = a.for_votes != b.for_votes;
    let against_votes_changed = a.against_votes != b.against_votes;

    let mut actions_added: u32 = 0;
    let mut actions_removed: u32 = 0;
    let mut actions_modified: u32 = 0;

    // Actions present in B but not in A are considered added.
    for action in b.actions.iter() {
        if !actions_contains(&a.actions, &action) {
            actions_added += 1;
        }
    }

    // Actions present in A but not in B are considered removed.
    for action in a.actions.iter() {
        if !actions_contains(&b.actions, &action) {
            actions_removed += 1;
        }
    }

    // Actions sharing the same target contract and function but different args are
    // considered modified.
    for action_a in a.actions.iter() {
        for action_b in b.actions.iter() {
            if action_a.contract_id == action_b.contract_id
                && action_a.function_name == action_b.function_name
                && action_a.args != action_b.args
            {
                actions_modified += 1;
            }
        }
    }

    let actions_changed = actions_added > 0 || actions_removed > 0 || actions_modified > 0;
    let same_actions = !actions_changed;
    let identical = !title_changed
        && !description_changed
        && !state_changed
        && !for_votes_changed
        && !against_votes_changed
        && same_actions;

    ProposalComparison {
        proposal_id,
        version_a,
        version_b,
        title_changed,
        description_changed,
        actions_changed,
        state_changed,
        for_votes_changed,
        against_votes_changed,
        actions_added,
        actions_removed,
        actions_modified,
        same_actions,
        identical,
    }
}

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

    // Version comparison
    fn get_proposal_version(e: Env, proposal_id: u32, version: u32) -> Proposal;
    fn compare_proposal_versions(e: Env, proposal_id: u32, version_a: u32, version_b: u32) -> ProposalComparison;
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
        read_voting_power(&e, user)
    }

    fn get_effective_voting_power(e: Env, user: Address) -> i128 {
        get_effective_voting_power(&e, user)
    }

    fn get_config(e: Env) -> GovernanceConfig {
        read_config(&e)
    }

    fn get_security_council(e: Env) -> Option<Address> {
        get_security_council(&e)
    }

    fn get_proposal_version(e: Env, proposal_id: u32, version: u32) -> Proposal {
        read_proposal_version(fe, proposal_id, version)
    }

    fn compare_proposal_versions(
        e: Env,
        proposal_id: u32,
        version_a: u32,
        version_b: u32,
    ) -> ProposalComparison {
        compare_proposal_versions(&e, proposal_id, version_a, version_b)
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
        let contract_id = env.register_contract(None, Governance);
        let client = GovernanceClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin, &100, &3600, &10);
        (client, admin)
    }

    fn sample_actions(env: &Env) -> Vec<ProposalAction> {
        let mut actions = Vec::new(env);
        actions.push_back(ProposalAction {
            contract_id: Address::generate(env),
            function_name: String::from_str(env, "transfer"),
            args: Vec::new(env),
        });
        actions
    }

    fn write_version(env: &Env, client: &GovernanceClient, proposal_id: u32, version: u32, proposal: &Proposal) {
        env.as_contract(client.address(), || {
            write_proposal_version(env, proposal_id, version, proposal);
        });
    }

    #[test]
    fn test_compare_identical_versions() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = setup(&env);
        let proposal_id = 7;
        let proposal = Proposal {
            id: proposal_id,
            proposer: Address::generate(&env),
            title: String::from_str(&env, "Title"),
            description: String::from_str(&env, "Description"),
            actions: sample_actions(&env),
            state: ProposalState::Draft,
            start_time: 0,
            end_time: 0,
            queued_time: 0,
            executed_time: 0,
            for_votes: 0,
            against_votes: 0,
            quorum_required: 0,
        };
        write_version(&env, &client, proposal_id, 1, &proposal);
        write_version(&env, &client, proposal_id, 2, &proposal);

        let comparison = client.compare_proposal_versions(&proposal_id, &1, &2);
        assert!(comparison.identical);
        assert!(comparison.same_actions);
        assert!(!comparison.title_changed);
        assert!(!comparison.description_changed);
        assert!(!comparison.actions_changed);
        assert_eq!(comparison.actions_added, 0);
        assert_eq!(comparison.actions_removed, 0);
        assert_eq!(comparison.actions_modified, 0);
    }

    #[test]
    fn test_compare_detects_field_changes() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = setup(&env);
        let proposal_id = 9;
        let mut version_a = Proposal {
            id: proposal_id,
            proposer: Address::generate(&env),
            title: String::from_str(&env, "Old title"),
            description: String::from_str(&env, "Old description"),
            actions: sample_actions(&env),
            state: ProposalState::Draft,
            start_time: 0,
            end_time: 0,
            queued_time: 0,
            executed_time: 0,
            for_votes: 10,
            against_votes: 5,
            quorum_required: 1,
        };
        let mut version_b = version_a.clone();
        version_b.title = String::from_str(&env, "New title");
        version_b.description = String::from_str(&env, "New description");
        version_b.state = ProposalState::Voting;
        version_b.for_votes = 20;
        version_b.against_votes = 7;

        write_version(&env, &client, proposal_id, 1, &version_a);
        write_version(&env, &client, proposal_id, 2, &version_b);

        let comparison = client.compare_proposal_versions(&proposal_id, &1, &2);
        assert!(!comparison.identical);
        assert!(comparison.title_changed);
        assert!(comparison.description_changed);
        assert!(comparison.state_changed);
        assert!(comparison.for_votes_changed);
        assert!(comparison.against_votes_changed);
        assert!(comparison.same_actions);
    }

    #[test]
    fn test_compare_detects_action_diffs() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = setup(&env);
        let proposal_id = 11;
        let action_a = ProposalAction {
            contract_id: Address::generate(&env),
            function_name: String::from_str(&env, "mint"),
            args: Vec::new(&env),
        };
        let action_b = ProposalAction {
            contract_id: Address::generate(&env),
            function_name: String::from_str(&env, "burn"),
            args: Vec::new(&env),
        };
        let action_modified = ProposalAction {
            contract_id: action_a.contract_id.clone(),
            function_name: action_a.function_name.clone(),
            args: Vec::new(&env),
        };

        let mut actions_a = Vec::new(&env);
        actions_a.push_back(action_a.clone());
        let mut actions_b = Vec::new(&env);
        actions_b.push_back(action_b);
        actions_b.push_back(action_modified);

        let mut version_a = Proposal {
            id: proposal_id,
            proposer: Address::generate(&env),
            title: String::from_str(&env, "Title"),
            description: String::from_str(&env, "Description"),
            actions: actions_a,
            state: ProposalState::Draft,
            start_time: 0,
            end_time: 0,
            queued_time: 0,
            executed_time: 0,
            for_votes: 0,
            against_votes: 0,
            quorum_required: 0,
        };
        version_a.actions = actions_a;
        let mut version_b = version_a.clone();
        version_b.actions = actions_b;

        write_version(&env, &client, proposal_id, 1, &version_a);
        write_version(&env, &client, proposal_id, 2, &version_b);

        let comparison = client.compare_proposal_versions(&proposal_id, &1, &2);
        assert!(comparison.actions_changed);
        assert!(!comparison.same_actions);
        assert_eq!(comparison.actions_added, 2);
        assert_eq!(comparison.actions_removed, 1);
        assert_eq!(comparison.actions_modified, 1);
    }

    #[test]
    fn test_get_proposal_version() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = setup(&env);
        let proposal_id = 13;
        let proposal = Proposal {
            id: proposal_id,
            proposer: Address::generate(&env),
            title: String::from_str(&env, "Title"),
            description: String::from_str(&env, "Description"),
            actions: Vec::new(&env),
            state: ProposalState::Queued,
            start_time: 1,
            end_time: 2,
            queued_time: 3,
            executed_time: 0,
            for_votes: 5,
            against_votes: 2,
            quorum_required: 1,
        };
        write_version(&env, &client, proposal_id, 42, &proposal);

        let fetched = client.get_proposal_version(&proposal_id, &42);
        assert_eq!(fetched.id, proposal.id);
        assert_eq!(fetched.title, proposal.title);
        assert_eq!(fetched.state, proposal.state);
    }

    #[test]
    fn test_double_voting_prevention() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Governance);
        let client = GovernanceClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let voter_a = Address::generate(&env);
        let voter_b = Address::generate(&env);

        client.initialize(&admin, &100, &3600, &10);

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
        let result = std::panic::catch_unwind(c|| client.cast_vote(&proposal_id, &voter_b, &true));
        assert!(result.is_err());
    }
}
