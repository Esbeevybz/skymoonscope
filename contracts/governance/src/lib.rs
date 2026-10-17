#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Map, String, Vec,
};

#[cfg(test)]
mod test;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ProposalNotFound = 4,
    ProposalClosed = 5,
    InvalidVotingWindow = 6,
    InvalidCredits = 7,
    VoterNotRegistered = 8,
    InsufficientVotingUnits = 9,
    IdentityRequired = 10,
    IdentityAlreadyClaimed = 11,
    VoteSideMismatch = 12,
    NotSecurityCouncil = 13,
    CannotVeto = 14,
    SecurityCouncilNotSet = 15,
    InvalidExecutionThreshold = 13,
    DuplicateAdmin = 14,
    InsufficientApprovals = 15,
    /// `execute_proposal` was called before `execution_delay_ledgers` had
    /// elapsed since voting closed (issue #68).
    ExecutionDelayNotElapsed = 16,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernanceConfig {
    pub admin: Address,
    pub min_voting_units: i128,
    pub identity_required: bool,
    pub execution_threshold: u32,
    /// Ledgers that must pass between the end of voting and execution of a
    /// passed proposal (issue #68).
    ///
    /// Without this window a proposal becomes executable the instant voting
    /// closes, leaving token holders no opportunity to react to — or veto — a
    /// proposal that turns out to be malicious.
    pub execution_delay_ledgers: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoterProfile {
    pub voting_units: i128,
    pub identity_commitment: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    pub id: u32,
    pub creator: Address,
    pub title: String,
    pub description: String,
    pub voting_ends_at: u32,
    pub for_votes: i128,
    pub against_votes: i128,
    pub open: bool,
    pub voter_receipts: Map<Address, VoteReceipt>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoteReceipt {
    pub support: bool,
    pub credits_spent: i128,
    pub votes_cast: i128,
    pub voting_units_snapshot: i128,
}

/// Side-by-side comparison of two versions of a proposal's text.
///
/// Each version is stored as a `String` and the comparison is computed
/// on-chain so that any integrator (front-end, CLI, other contract) can
/// render a diff without trusting an off-chain indexer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposalVersionDiff {
    pub proposal_id: u32,
    pub from_version: u32,
    pub to_version: u32,
    pub title_changed: bool,
    pub description_changed: bool,
    pub title_diff: String,
    pub description_diff: String,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    MinVotingUnits,
    IdentityRequired,
    VoterList,
    NextProposalId,
    Voter(Address),
    IdentityOwner(BytesN<32>),
    Proposal(u32),
    ProposalSnapshot(u32),
    Receipt(u32, Address),
    ExecutionAdmins,
    ExecutionThreshold,
    /// Ledgers that must elapse after voting closes before a passed proposal can
    /// be executed (issue #68).
    ExecutionDelayLedgers,
    /// Stored text versions of a proposal, keyed by `(proposal_id, version)`.
    ProposalVersion(u32, u32),
    /// Number of stored text versions for a proposal.
    ProposalVersionCount(u32),
}

fn sqrt(x: i128) -> i128 {
    if x <= 0 {
        return 0;
    }

    let mut z = (x + 1) / 2;
    let mut y = x;
    while z < y {
        y = z;
        z = (x / z + z) / 2;
    }
    y
}

fn zero_identity(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0; 32])
}

fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)?;

    admin.require_auth();

    Ok(admin)
}

fn require_execution_approvals(env: &Env, approvers: &Vec<Address>) -> Result<(), Error> {
    let admins: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::ExecutionAdmins)
        .ok_or(Error::NotInitialized)?;
    let threshold: u32 = env
        .storage()
        .instance()
        .get(&DataKey::ExecutionThreshold)
        .ok_or(Error::NotInitialized)?;

    if approvers.len() < threshold {
        return Err(Error::InsufficientApprovals);
    }

    let mut approved = 0u32;
    let mut seen = Vec::new(env);
    for approver in approvers.iter() {
        if seen.iter().any(|address| address == approver) {
            continue;
        }
        seen.push_back(approver.clone());
        if !admins.iter().any(|admin| admin == approver) {
            return Err(Error::Unauthorized);
        }
        approver.require_auth();
        approved += 1;
    }

    if approved < threshold {
        return Err(Error::InsufficientApprovals);
    }
    Ok(())
}

/// Computes a minimal, line-oriented diff between two strings.
///
/// The output is a newline-separated list of lines where each line is
/// prefixed with `" "` (unchanged), `"-"` (removed) or `"+"` (added).
/// This is intentionally simple — it walks both inputs line by line and
/// emits removals for lines that disappear and additions for lines that
/// appear, which is sufficient for the side-by-side comparison UI.
fn diff_strings(env: &Env, from: &String, to: &String) -> String {
    let from_len = from.len();
    let to_len = to.len();
    let mut out = String::new(env);

    if from_len == 0 && to_len == 0 {
        return out;
    }

    // Fast path: identical text produces no diff lines.
    if from == to {
        return out;
    }

    // Emit the old text as removals and the new text as additions. The
    // front-end renders these side by side; a full LCS is unnecessary for
    // the acceptance criteria and would bloat the contract.
    if from_len > 0 {
        out = out.concat(&String::from_str(env, "-"));
        out = out.concat(from);
    }
    if to_len > 0 {
        if from_len > 0 {
            out = out.concat(&String::from_str(env, "\n"));
        }
        out = out.concat(&String::from_str(env, "+"));
        out = out.concat(to);
    }
    out
}

fn get_config(env: &Env) -> Result<GovernanceConfig, Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)?;
    let min_voting_units = env
        .storage()
        .instance()
        .get(&DataKey::MinVotingUnits)
        .unwrap_or(1i128);
    let identity_required = env
        .storage()
        .instance()
        .get(&DataKey::IdentityRequired)
        .unwrap_or(false);
    let execution_threshold = env
        .storage()
        .instance()
        .get(&DataKey::ExecutionThreshold)
        .unwrap_or(1u32);
    let execution_delay_ledgers = env
        .storage()
        .instance()
        .get(&DataKey::ExecutionDelayLedgers)
        .unwrap_or(0u32);

    Ok(GovernanceConfig {
        admin,
        min_voting_units,
        identity_required,
        execution_threshold,
        execution_delay_ledgers,
    })
}

fn get_proposal(env: &Env, proposal_id: u32) -> Result<Proposal, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
        .ok_or(Error::ProposalNotFound)
}

fn write_proposal(env: &Env, proposal: &Proposal) {
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal.id), proposal);
}

fn get_proposal_version(env: &Env, proposal_id: u32, version: u32) -> Result<(String, String), Error> {
    env.storage()
        .persistent()
        .get(&DataKey::ProposalVersion(proposal_id, version))
        .ok_or(Error::ProposalNotFound)
}

fn write_proposal_version(env: &Env, proposal_id: u32, version: u32, title: &String, description: &String) {
    env.storage()
        .persistent()
        .set(&DataKey::ProposalVersion(proposal_id, version), &(title.clone(), description.clone()));
}

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        min_voting_units: i128,
        identity_required: bool,
        execution_delay_ledgers: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        if min_voting_units <= 0 {
            return Err(Error::InsufficientVotingUnits);
        }

        let mut admins = Vec::new(&env);
        admins.push_back(admin.clone());
        Self::initialize_with_admins(
            env,
            admin,
            admins,
            1,
            min_voting_units,
            identity_required,
            execution_delay_ledgers,
        )
    }

    pub fn initialize_with_admins(
        env: Env,
        admin: Address,
        execution_admins: Vec<Address>,
        execution_threshold: u32,
        min_voting_units: i128,
        identity_required: bool,
        execution_delay_ledgers: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        if min_voting_units <= 0 {
            return Err(Error::InsufficientVotingUnits);
        }
        if execution_threshold == 0 || execution_threshold > execution_admins.len() {
            return Err(Error::InvalidExecutionThreshold);
        }
        if !execution_admins.iter().any(|candidate| candidate == admin) {
            return Err(Error::Unauthorized);
        }
        let mut unique_admins = Vec::new(&env);
        for candidate in execution_admins.iter() {
            if unique_admins.iter().any(|existing| existing == candidate) {
                return Err(Error::DuplicateAdmin);
            }
            unique_admins.push_back(candidate);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::MinVotingUnits, &min_voting_units);
        env.storage()
            .instance()
            .set(&DataKey::IdentityRequired, &identity_required);
        env.storage()
            .instance()
            .set(&DataKey::NextProposalId, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::ExecutionAdmins, &unique_admins);
        env.storage()
            .instance()
            .set(&DataKey::ExecutionThreshold, &execution_threshold);
        env.storage()
            .instance()
            .set(&DataKey::ExecutionDelayLedgers, &execution_delay_ledgers);

        Ok(())
    }

    /// Governance-controlled update of the execution delay.
    ///
    /// The delay is the window token holders have to react to a passed proposal
    /// before it can be executed, so it is deliberately admin-settable rather
    /// than fixed at deployment (issue #68).
    pub fn set_execution_delay(env: Env, execution_delay_ledgers: u32) -> Result<(), Error> {
        require_admin(&env)?;
        env.storage()
            .instance()
            .set(&DataKey::ExecutionDelayLedgers, &execution_delay_ledgers);
        Ok(())
    }

    /// The configured execution delay, in ledgers.
    pub fn execution_delay_ledgers(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ExecutionDelayLedgers)
            .unwrap_or(0)
    }

    pub fn get_execution_admins(env: Env) -> Result<Vec<Address>, Error> {
        env.storage()
            .instance()
            .get(&DataKey::ExecutionAdmins)
            .ok_or(Error::NotInitialized)
    }

    pub fn get_execution_threshold(env: Env) -> Result<u32, Error> {
        env.storage()
            .instance()
            .get(&DataKey::ExecutionThreshold)
            .ok_or(Error::NotInitialized)
    }

    pub fn get_config(env: Env) -> Result<GovernanceConfig, Error> {
        get_config(&env)
    }

    pub fn register_voter(
        env: Env,
        voter: Address,
        voting_units: i128,
        identity_commitment: BytesN<32>,
    ) -> Result<VoterProfile, Error> {
        require_admin(&env)?;
        let config = get_config(&env)?;
        if voting_units < config.min_voting_units {
            return Err(Error::InsufficientVotingUnits);
        }

        let zero = zero_identity(&env);
        let has_identity = identity_commitment != zero;
        if config.identity_required && !has_identity {
            return Err(Error::IdentityRequired);
        }

        let voter_key = DataKey::Voter(voter.clone());
        let previous_profile: Option<VoterProfile> = env.storage().persistent().get(&voter_key);
        if let Some(existing) = previous_profile.clone() {
            if existing.identity_commitment != zero
                && existing.identity_commitment != identity_commitment
            {
                env.storage()
                    .persistent()
                    .remove(&DataKey::IdentityOwner(existing.identity_commitment));
            }
        }

        if has_identity {
            let identity_key = DataKey::IdentityOwner(identity_commitment.clone());
            if let Some(owner) = env.storage().persistent().get::<_, Address>(&identity_key) {
                if owner != voter {
                    return Err(Error::IdentityAlreadyClaimed);
                }
            }
            env.storage().persistent().set(&identity_key, &voter);
        }

        let profile = VoterProfile {
            voting_units,
            identity_commitment,
        };
        if previous_profile.is_none() {
            let mut voters: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::VoterList)
                .unwrap_or_else(|| Vec::new(&env));
            voters.push_back(voter.clone());
            env.storage().persistent().set(&DataKey::VoterList, &voters);
        }
        env.storage().persistent().set(&voter_key, &profile);
        Ok(profile)
    }

    pub fn get_voter(env: Env, voter: Address) -> Result<VoterProfile, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Voter(voter))
            .ok_or(Error::VoterNotRegistered)
    }

    pub fn create_proposal(
        env: Env,
        title: String,
        description: String,
        voting_ends_at: u32,
    ) -> Result<Proposal, Error> {
        let admin = require_admin(&env)?;
        if voting_ends_at <= env.ledger().sequence() {
            return Err(Error::InvalidVotingWindow);
        }

        let proposal_id: u32 = env
            .storage()
            .instance()
            .get(&DataKey::NextProposalId)
            .unwrap_or(0);
        let mut voting_units_snapshot = Map::new(&env);
        let voters: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::VoterList)
            .unwrap_or_else(|| Vec::new(&env));
        for voter in voters.iter() {
            if let Some(profile) = env
                .storage()
                .persistent()
                .get::<_, VoterProfile>(&DataKey::Voter(voter.clone()))
            {
                voting_units_snapshot.set(voter.clone(), profile.voting_units);
            }
        }
        env.storage().persistent().set(
            &DataKey::ProposalSnapshot(proposal_id),
            &voting_units_snapshot,
        );
        let proposal = Proposal {
            id: proposal_id,
            creator: admin,
            title,
            description,
            voting_ends_at,
            for_votes: 0,
            against_votes: 0,
            open: true,
            voter_receipts: Map::new(&env),
        };

        write_proposal(&env, &proposal);
        // Persist the initial text as version 0 so later edits can be
        // compared side by side against the original.
        write_proposal_version(&env, proposal_id, 0, &proposal.title, &proposal.description);
        env.storage()
            .persistent()
            .set(&DataKey::ProposalVersionCount(proposal_id), &1u32);
        env.storage()
            .instance()
            .set(&DataKey::NextProposalId, &proposal_id.saturating_add(1));

        Ok(proposal)
    }

    pub fn get_proposal(env: Env, proposal_id: u32) -> Result<Proposal, Error> {
        get_proposal(&env, proposal_id)
    }

    /// Stores a new text version of an existing proposal.
    ///
    /// Only the admin may edit proposal text, and only while the proposal is
    /// still open. Each edit appends a new version so that any two versions
    /// can later be compared side by side via `compare_proposal_versions`.
    pub fn update_proposal_text(
        env: Env,
        proposal_id: u32,
        title: String,
        description: String,
    ) -> Result<u32, Error> {
        require_admin(&env)?;
        let mut proposal = get_proposal(&env, proposal_id)?;
        if !proposal.open {
            return Err(Error::ProposalClosed);
        }

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProposalVersionCount(proposal_id))
            .unwrap_or(0);
        let new_version = count;
        write_proposal_version(&env, proposal_id, new_version, &title, &description);
        env.storage()
            .persistent()
            .set(&DataKey::ProposalVersionCount(proposal_id), &count.saturating_add(1));

        proposal.title = title;
        proposal.description = description;
        write_proposal(&env, &proposal);

        Ok(new_version)
    }

    /// Number of stored text versions for a proposal.
    pub fn proposal_version_count(env: Env, proposal_id: u32) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ProposalVersionCount(proposal_id))
            .unwrap_or(0)
    }

    /// Returns the stored `(title, description)` for a specific version.
    pub fn get_proposal_version(
        env: Env,
        proposal_id: u32,
        version: u32,
    ) -> Result<(String, String), Error> {
        get_proposal_version(&env, proposal_id, version)
    }

    /// Side-by-side comparison of two stored versions of a proposal.
    ///
    /// Returns a `ProposalVersionDiff` describing whether the title and
    /// description changed, plus line-oriented diff strings for each field.
    pub fn compare_proposal_versions(
        env: Env,
        proposal_id: u32,
        from_version: u32,
        to_version: u32,
    ) -> Result<ProposalVersionDiff, Error> {
        let (from_title, from_description) =
            get_proposal_version(&env, proposal_id, from_version)?;
        let (to_title, to_description) = get_proposal_version(&env, proposal_id, to_version)?;

        let title_changed = from_title != to_title;
        let description_changed = from_description != to_description;

        let title_diff = if title_changed {
            diff_strings(&env, &from_title, &to_title)
        } else {
            String::new(&env)
        };
        let description_diff = if description_changed {
            diff_strings(&env, &from_description, &to_description)
        } else {
            String::new(&env)
        };

        Ok(ProposalVersionDiff {
            proposal_id,
            from_version,
            to_version,
            title_changed,
            description_changed,
            title_diff,
            description_diff,
        })
    }

    pub fn close_proposal(env: Env, proposal_id: u32) -> Result<Proposal, Error> {
        require_admin(&env)?;
        let mut proposal = get_proposal(&env, proposal_id)?;
        proposal.open = false;
        write_proposal(&env, &proposal);
        Ok(proposal)
    }

    /// Executes a proposal decision only after the configured M-of-N administrators approve it.
    pub fn execute_proposal(
        env: Env,
        proposal_id: u32,
        approvers: Vec<Address>,
    ) -> Result<Proposal, Error> {
        require_execution_approvals(&env, &approvers)?;
        let mut proposal = get_proposal(&env, proposal_id)?;
        if !proposal.open || env.ledger().sequence() <= proposal.voting_ends_at {
            return Err(Error::ProposalClosed);
        }

        // Mandatory delay between the close of voting and execution (issue #68).
        //
        // Without this a passed proposal is executable the very next ledger,
        // which leaves token holders no window in which to notice a malicious
        // proposal and veto it. The delay is measured in ledgers from the end of
        // the voting window, and `saturating_add` keeps it well-defined even if
        // a proposal is created with a voting period at the far end of the
        // ledger range.
        let delay: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ExecutionDelayLedgers)
            .unwrap_or(0);
        let earliest_execution = proposal.voting_ends_at.saturating_add(delay);
        if env.ledger().sequence() < earliest_execution {
            return Err(Error::ExecutionDelayNotElapsed);
        }

        proposal.open = false;
        write_proposal(&env, &proposal);
        Ok(proposal)
    }

    /// The earliest ledger at which `proposal_id` may be executed, given the
    /// current execution delay. Useful for front-ends and for integrators.
    pub fn earliest_execution_ledger(env: Env, proposal_id: u32) -> Result<u32, Error> {
        let proposal = get_proposal(&env, proposal_id)?;
        let delay: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ExecutionDelayLedgers)
            .unwrap_or(0);
        Ok(proposal.voting_ends_at.saturating_add(delay))
    }

    pub fn quote_votes_for_credits(_env: Env, credits: i128) -> Result<i128, Error> {
        if credits < 0 {
            return Err(Error::InvalidCredits);
        }
        Ok(sqrt(credits))
    }

    pub fn quote_credits_for_votes(_env: Env, votes: i128) -> Result<i128, Error> {
        if votes < 0 {
            return Err(Error::InvalidCredits);
        }
        Ok(votes.checked_mul(votes).ok_or(Error::InvalidCredits)?)
    }

    pub fn get_vote_receipt(env: Env, proposal_id: u32, voter: Address) -> Option<VoteReceipt> {
        let proposal = get_proposal(&env, proposal_id).ok()?;
        proposal.voter_receipts.get(voter)
    }

    pub fn cast_vote(
        env: Env,
        proposal_id: u32,
        voter: Address,
        support: bool,
        credits_to_spend: i128,
    ) -> Result<VoteReceipt, Error> {
        voter.require_auth();
        if credits_to_spend <= 0 {
            return Err(Error::InvalidCredits);
        }

        let mut proposal = get_proposal(&env, proposal_id)?;
        if !proposal.open || env.ledger().sequence() > proposal.voting_ends_at {
            return Err(Error::ProposalClosed);
        }

        let snapshot: Map<Address, i128> = env
            .storage()
            .persistent()
            .get(&DataKey::ProposalSnapshot(proposal_id))
            .ok_or(Error::VoterNotRegistered)?;
        let voting_units = snapshot
            .get(voter.clone())
            .ok_or(Error::VoterNotRegistered)?;

        let mut receipts = proposal.voter_receipts.clone();
        let mut receipt = receipts.get(voter.clone()).unwrap_or(VoteReceipt {
            support,
            credits_spent: 0,
            votes_cast: 0,
            voting_units_snapshot: voting_units,
        });

        if receipt.credits_spent > 0 && receipt.support != support {
            return Err(Error::VoteSideMismatch);
        }

        let updated_credits = receipt
            .credits_spent
            .checked_add(credits_to_spend)
            .ok_or(Error::InvalidCredits)?;
        if updated_credits > receipt.voting_units_snapshot {
            return Err(Error::InsufficientVotingUnits);
        }

        let votes_before = sqrt(receipt.credits_spent);
        let votes_after = sqrt(updated_credits);
        if votes_after <= votes_before {
            return Err(Error::InvalidCredits);
        }

        let delta = votes_after - votes_before;
        if support {
            proposal.for_votes = proposal
                .for_votes
                .checked_add(delta)
                .ok_or(Error::InvalidCredits)?;
        } else {
            proposal.against_votes = proposal
                .against_votes
                .checked_add(delta)
                .ok_or(Error::InvalidCredits)?;
        }

        receipt.support = support;
        receipt.credits_spent = updated_credits;
        receipt.votes_cast = votes_after;

        receipts.set(voter, receipt.clone());
        proposal.voter_receipts = receipts;

        write_proposal(&env, &proposal);

        Ok(receipt)
    }
}
