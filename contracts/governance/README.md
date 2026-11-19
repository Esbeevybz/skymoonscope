# Governance Contract

A Comprehensive DAO governance system for Soroban implementing multi-step proposal workflows with voting, delegation, and timelock execution.

## Overview

This contract provides a full-featured governance system where proposals progress through distinct stages:

- **Draft**: Proposal created but not yet open for voting
- **Voting**: Active voting period with delegation support
- **Queued**: Proposal approved, waiting for timelock delay
- **Executed**: Proposal actions have been executed
- **Cancelled**: Proposal cancelled by admin

## Features

- **Multi-stage Proposals**: Structured proposal lifecycle
- **Voting Delegation**: Users can delegate voting power to others
- **Quorum Requirements**: Minimum participation thresholds
- **Timelock Execution**: Delayed execution for security
- **Admin Controls**: Proposal cancellation and parameter management
- **M-of-N Execution Authorization**: A configurable, distinct set of administrators must approve execution.
- **Side-by-Side Version Comparison**: Compare two snapshots of the same proposal to see what changed.

## Multi-signature execution

Use `initialize_with_admins` to configure the execution administrator set and the required threshold. The primary `admin` remains responsible for voter management, while `execute_proposal` accepts the proposal ID and a list of approving administrators. Every approver must be a configured administrator, authorize the call, and be distinct; duplicate addresses never count twice.

```rust
client.initialize_with_admins(&primary_admin, &admins, &2, &9, &false);
client.execute_proposal(&proposal_id, &approvers); // requires two distinct admin approvals
```

The legacy `initialize` method configures a one-of-one execution policy for backwards compatibility.

## Proposal States

```rust
enum ProposalState {
    Draft,      // Created but not voting
    Voting,     // Active voting period
    Queued,      // Approved, waiting for timelock
    Executed,    // Successfully executed
    Cancelled,   // Cancelled by admin
    Vetoed,      // Vetoed by the security council
}
```

## Contract Interface

### Initialization
```rust
fn initialize(e: Env, admin: Address, voting_period: u64, timelock_delay: u64, quorum_percentage: u32)
```
Sets up the governance contract with admin and timing parameters.

### Proposal Management
```rust
fn create_proposal(e: Env, title: String, description: String, actions: Vec<ProposalAction>) -> u32
fn start_voting(e: Env, proposal_id: u32)  // Admin only
fn queue_proposal(e: Env, proposal_id: u32)
fn execute_proposal(e: Env, proposal_id: u32)
fn cancel_proposal_admin(e: Env, proposal_id: u32)  // Admin only
```

### Voting
```rust
fn cast_vote(e: Env, proposal_id: u32, support: bool)
fn delegate(e: Env, delegate: Address)
fn set_voting_power(e: Env, user: Address, power: i128)  // Admin only
```

### Queries
```rust
fn get_proposal(e: Env, proposal_id: u32) -> Proposal
fn get_voting_power(e: Env, user: Address) -> i128
fn get_effective_voting_power(e: Env, user: Address) -> i128  // Includes delegated power
fn get_config(e: Env) -> GovernanceConfig
```

### Version Comparison
```rust
fn get_proposal_version(e: Env, proposal_id: u32, version: u32) -> Proposal
fn compare_proposal_versions(e: Env, proposal_id: u32, version_a: u32, version_b: u32) -> ProposalComparison
```

Each proposal snapshot is stored under a `ProposalVersion(proposal_id, version)` storage key. `compare_proposal_versions` returns a `ProposalComparison` describing the side-by-side differences:

| Field | Meaning |
| --- | --- |
| `title_changed` | The proposal title differs between versions |
| `description_changed` | The description differs |
| `actions_changed` | Any added, removed, or modified action |
| `state_changed` | The proposal state differs |
| `for_votes_changed` / `against_votes_changed` | Vote tallies differ |
| `actions_added` | Count of actions present in version B but not in A  |
| `actions_removed` | Count of actions present in version A but not in B |
| `actions_modified` | Count of actions with the same target but different args |
| `same_actions` | True when no action differences exist |
| `identical` | True when every compared field is unchanged |

Example:

```rust
// Snapshot two versions of proposal 7 and compare them.
let comparison = client.compare_proposal_versions(&7, &1, &2);
if comparison.title_changed {
    // surface the title diff to users

}
```

## Usage Example

```rust
// Initialize governance
let client = GovernanceClient::new(&env, &contract_id);
client.initialize(&admin, &604800, &172800, &10); // 7d voting, 2d timelock, 10% quorum

// Set up voting power
client.set_voting_power(&user1, &100);

// Create proposal
let actions = Vec::new(&env); // Add contract calls here
let proposal_id = client.create_proposal(
    &"Update Protocol".into(),
    &"Update protocol parameters".into(),
    &actions
);

// Start voting (admin)
client.start_voting(&proposal_id);

// Users vote
client.cast_vote(&proposal_id, &true);

// Or delegate
client.delegate(&trusted_delegate);

// Queue successful proposal
client.queue_proposal(&proposal_id);

// Execute after timelock
client.execute_proposal(&proposal_id);
```

## Security Features

- **Timelock**: Prevents flash loan attacks on governance
- **Quorum Requirements**: Ensures sufficient participation
- **Admin Oversight**: Emergency cancellation capabilities
- **Delegation Safety**: Secure voting power delegation
- **State Validation**: Strict state transitions

## Building

```bash
# Build the contract
cargo build --package soroban-governance-contract

# Build for WASM
cargo build --target wasm32-unknown-unknown --release --package soroban-governance-contract
```

## Testing

```bash
# Run tests
cargo test --package soroban-governance-contract
```

## Parameters

- **voting_period**: Duration of voting in seconds (e.g. 604800 = 7 days)
- **timelock_delay**: Delay before execution in seconds (e.g. 172800 = 2 days)
- **quorum_percentage**: Minimum participation as percentage of total voting power

## Future Enhancements

- Quadratic voting
- Proposal templates
- Cross-chain governance
- Snapshot voting integration

## License

MIT Stellar Wave
