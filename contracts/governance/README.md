# Governance Contract

A DAO governance system for Soroban implementing multi-step proposal workflows with voting, delegation, timelock execution, and side-by-side proposal comparison.

## Overview

Proposals progress through distinct stages:

- **Draft**: Proposal created but not yet open for voting
- **Voting**: Active voting period with delegation support
- **Queued**: Proposal approved, waiting for timelock delay
- **Executed**: Proposal actions have been executed
- **Cancelled**: Proposal cancelled by admin
- **Vetoed**: Proposal vetoed by the security council

## Features

- **Multi-stage Proposals**: Structured proposal lifecycle
- **Voting Delegation**: Users can delegate voting power to others
- **Quorum Requirements**: Minimum participation thresholds
- **Timelock Execution**: Delayed execution for security
- **Admin Controls**: Proposal cancellation and parameter management
- **Side-by-side Comparison**: Diff two proposal versions to inspect changes before execution

## Side-by-side comparison

Use `compare_proposals` to diff two proposals (typically two versions of the same proposal) and return a `<ProposalComparison>` structure:

```rust
let cmp = client.compare_proposals(&version_a, &version_b);
if !cmp.equal {
    // cmp.title_changed, cmp.description_changed, cmp.actions_changed, cmp.state_changed
    // cmp.actions_a, cmp.actions_b, cmp.for_votes_delta, cmp.against_votes_delta
}
```

The comparison is pure and read-only: it never mutates state and panics if either proposal does not exist.

## Proposal States

```rust
enum ProposalState {
    Draft,
    Voting,
    Queued,
    Executed,
    Cancelled,
    Vetoed,
}
```

## Contract Interface

### Initialization
```rust
fn initialize(e: Env, admin: Address, total_supply: i128, voting_period: u64, timelock_delay: u64, quorum_percentage: u32)
```
Sets up the governance contract with admin, total supply, and timing parameters.

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
fn compare_proposals(e: Env, proposal_id_a: u32, proposal_id_b: u32) -> ProposalComparison
```

## Usage Example

```rust
// Initialize governance
let client = GovernanceClient::new(&env, &contract_id);
client.initialize(&admin, &1_000_000, &604800, &172800, &10); // 7d voting, 2d timelock, 10% quorum

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

// Or delegate client.delegate(&trusted_delegate);

// Queue successful proposal
client.queue_proposal(&proposal_id);

// Execute after timelock
client.execute_proposal(&proposal_id);

// Compare two versions of a proposal
client.compare_proposals(&version_a, &version_b);
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
- **quorum_percentage**: Minimum participation as percentage of total supply

## Future Enhancements

- Quadratic voting
- Proposal templates
- Multi-signature execution
- Cross-chain governance
- Snapshot voting integration

## License

MIT Stellar Wave
