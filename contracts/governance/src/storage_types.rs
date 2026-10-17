use soroban_sdk::{contracttype, Address, String, Vec};

#[derive(Clone, PartialEq, Debug, Eq)]
#[contracttype]
pub enum ProposalState {
    Draft,
    Voting,
    Queued,
    Executed,
    Cancelled,
    Vetoed,
}

#[derive(Clone, PartialEq, Debug, Eq)]
#[contracttype]
pub struct ProposalAction {
    pub contract_id: Address,
    pub function_name: String,
    pub args: Vec<String>,
}

#[derive(Clone, PartialEq, Debug, Eq)]
#[contracttype]
pub struct Proposal {
    pub id: u32,
    pub proposer: Address,
    pub title: String,
    pub description: String,
    pub actions: Vec<ProposalAction>,
    pub state: ProposalState,
    pub start_time: u64,
    pub end_time: u64,
    pub queued_time: u64,
    pub executed_time: u64,
    pub for_votes: i128,
    pub against_votes: i128,
    pub quorum_required: i128,
}

#[derive(Clone, PartialEq, Debug, Eq)]
#[contracttype]
pub struct GovernanceConfig {
    pub admin: Address,
    pub voting_period: u64,
    pub timelock_delay: u64,
    pub quorum_percentage: u32,
    pub proposal_count: u32,
}

/// Side by
/// side comparison result for two contract versions.
#[derive(Clone, PartialEq, Debug, Eq)]
#[contracttype]
pub struct ProposalComparison {
    /// Whether the two proposals are identical in all compared fields.
    pub equal: bool,
    /// Whether the titles differ.
    pub title_changed: bool,
    /// Whether the descriptions differ.
    pub description_changed: bool,
    /// Whether the action sets differ.
    pub actions_changed: bool,
    /// Whether the states differ.
    pub state_changed: bool,
    /// Number of actions in the first proposal.
    pub actions_a: u32,
    /// Number of actions in the second proposal.
    pub actions_b: u32,
    /// Difference in for-votes (b - a).
    pub for_votes_delta: i128,
    /// Difference in against-votes (b - a).
    pub against_votes_delta: i128,
}

#[derive(Clone, PartialEq, Debug, Eq)]
#[contracttype]
pub enum DataKey {
    Proposal(u32),
    VotingPower(Address),
    DelegatedPower(Address),
    Delegate(Address),
    Config,
    HasVoted(u32, Address),
    SecurityCouncil,
    Voters,
    VotingPowerSnapshot(u32, Address),
    TotalVotingPowerSnapshot(u32),
}
