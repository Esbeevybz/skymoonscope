# Cross-Chain Verifier Contract

## Overview

The `cross_chain_verifier` contract enables trustless verification of cross-chain messages on Stellar/Soroban. A trusted relayer network posts Merkle state roots (one per source-chain block height), and any party can then prove that a specific message or transaction was included in that block by supplying a Binary Merkle Tree (BMT) proof.

Every successful cross-chain verification emits an event so that off-chain indexers and monitoring services can observe verification activity. See [Events](#events) for the emitted topics and data.

```
Source Chain Block
        │
        ▼
┌───────────────────┐
│  Merkle Tree      │  Built off-chain from all messages in the block
│  Root Hash        │──────────────────────────────────────────────────►  update_root()
└───────────────────┘                                                       (relayer only)
                                                                                │
                                                                                ▼
                                                                    ┌───────────────────────┐
                                                                    │  CrossChainVerifier   │
                                                                    │  Contract (Soroban)   │
                                                                    │                       │
                                                                    │  StateRoot(height)    │
                                                                    │  = root_hash          │
                                                                    └───────────┬───────────┘
                                                                                │
                                                                                ▼
                                                                         verify_message()
                                                                         (anyone can call)
```

---

## Contract Functions

### `initialize(admin: Address)`
Deploys and configures the contract with a single admin (the relayer authority). Can only be called once.

### `update_root(block_height: u32, new_root: BytesN<32>)`
Stores a Merkle state root for a given source-chain block height. Only the admin can call this.

### `get_root(block_height: u32) → Option<BytesN<32>>`
Returns the stored state root for a block height, or `None` if not yet posted.

### `verify_message(block_height, payload, proof, proof_flags) → bool`
Verifies a `Payload` against the state root at `block_height`. On success, its nonce is consumed, so it cannot be verified again through another verification entry point. Reusing a nonce panics with `"Nonce already used"`.

On a successful verification, emits a `verify` event (see [Events](#events)).

| Parameter | Type | Description |
|---|---|---|
| `block_height` | `u32` | Source-chain block height to verify against |
| `payload` | `Payload` | Message fields, including the nonce consumed on successful verification |
| `proof` | `Vec<BytesN<32>>` | Ordered list of sibling hashes from leaf to root |

### `verify_batch(messages: Vec<BatchMessage>) → BatchVerificationResult`
Verifies multiple messages in a single batch call to reduce per-call overhead. Returns a structured result indicating which messages succeeded and which failed.

For each successfully verified message, emits a `verify` event (see [Events](#events)).

**Performance Benefit:** 20-30% reduction in per-message overhead for batch sizes > 5 messages.

| Parameter | Type | Description |
|---|---|---|
| `messages` | `Vec<BatchMessage>` | Array of batch messages to verify |

**BatchMessage Structure:**
```rust
pub struct BatchMessage {
    pub block_height: u32,
    pub payload: Payload,
    pub proof: Vec<BytesN<32>>,
    pub proof_flags: Vec<bool>,
}
```

**BatchVerificationResult Structure:**
```rust
pub struct BatchVerificationResult {
    pub total: u32,          // Total messages in batch
    pub succeeded: u32,      // Number of successfully verified messages
    pub failed: u32,         // Number of failed messages
    pub failed_indices: Vec<u32>, // Indices of failed messages
}
```

### `verify_signed_batch(messages: Vec<SignedBatchMessage>) → SignedBatchVerificationResult`
Verifies multiple signed messages in a single batch call. Similar to `verify_batch` but for signed messages with signature verification.

Successfully verified messages consume their nonce, shared with `verify_message` and `verify_batch`. A repeated nonce is reported as a failed batch item rather than aborting the batch.

For each successfully verified message, emits a `verify` event (see [Events](#events)).

| Parameter | Type | Description |
|---|---|---|
| `messages` | `Vec<SignedBatchMessage>` | Array of signed batch messages to verify |

**SignedBatchMessage Structure:**
```rust
pub struct SignedBatchMessage {
    pub block_height: u32,
    pub signed_message: SignedMessage,
    pub proof: Vec<BytesN<32>>,
    pub proof_flags: Vec<bool>,
}
```

**SignedBatchVerificationResult Structure:**
```rust
pub struct SignedBatchVerificationResult {
    pub total: u32,          // Total messages in batch
    pub succeeded: u32,      // Number of successfully verified messages
    pub failed: u32,         // Number of failed messages
    pub failed_indices: Vec<u32>, // Indices of failed messages
}
```
| `proof_flags` | `Vec<bool>` | `true` = sibling is on the left, `false` = sibling is on the right |

### `verify_message_and_consume(block_height, nonce, leaf, proof, proof_flags) → bool`
Verifies a bare leaf and marks `nonce` as processed on success. The nonce is shared with the payload and signed-message verification entry points. Panics with `"nonce already processed"` if the same nonce is submitted a second time.

On a successful verification, emits a `verify` event (see [Events](#events)).

| Parameter | Type | Description |
|---|---|---|
| `nonce` | `u64` | Unique identifier for this message; must not have been used before |

### `is_nonce_processed(nonce) → bool`
Returns `true` if `nonce` has already been consumed by any successful verification entry point.

### `add_authorized_signer(public_key, algorithm)`
Registers a public key as an authorized cross-chain message signer. Only the admin can call this.

| Parameter | Type | Description |
|---|---|---|
| `public_key` | `Bytes` | Raw public key bytes (32 bytes for Ed25519, 33 bytes compressed for Secp256k1) |
| `algorithm` | `SignatureAlgorithm` | `Ed25519` or `Secp256k1` |

Panics with `"Signer already authorized"` if the key is already registered.

### `remove_authorized_signer(public_key)`
Removes a previously registered signer. Panics with `"Signer not found"` if the key is not registered.

### `get_signer_count() → u32`
Returns the number of currently authorized signers.

### `verify_signed_message(signed_message, block_height, proof, proof_flags) → bool`
Verifies that a `SignedMessage` was both included in the Merkle tree at `block_height` and signed by a currently authorized signer. On success, its nonce is consumed across all verification entry points. Returns `false` if the proof or signature is invalid, or if the nonce has already been used.

On a successful verification, emits a `verify` event (see [Events](#events)).

---

## Proof Format

The contract uses a standard Binary Merkle Tree where each internal node is:

```
node = SHA-256(left_child || right_child)
```

A proof is a path from the leaf up to the root. Each step provides the sibling hash and a flag indicating which side the sibling is on:

```
                    ROOT
                   /    \
               H(AB)    H(CD)
              /    \    /    \
             A      B  C      D
                    ▲
                    leaf = B

proof       = [A,    H(CD)]
proof_flags = [true, false]
              (A is left sibling, H(CD) is right sibling)
```

**Verification steps:**
1. Start with `current = leaf`
2. For each `(sibling, is_left)` pair:
   - If `is_left`: `current = SHA-256(sibling || current)`
   - If not `is_left`: `current = SHA-256(current || sibling)`
3. Assert `current == stored_root`

---

## Events

Every successful cross-chain verification emits an event so that off-chain indexers and monitoring services can observe verification activity.

### `verify`

Emitted once per successfully verified message. Failed verifications do **not** emit an event.

**Topics:**

| Topic | Type | Description |
|---|---|---|
| `"verify"` | `Symbol` | Event name |
| `block_height` | `u32` | Source-chain block height the message was verified against |

**Data:**

| Field | Type | Description |
|---|---|---|
| `leaf` | `BytesN<32>` | SHA-256 hash of the verified cross-chain message |
| `nonce` | `Option<u64>` | The consumed nonce, if the call was `verify_message_and_consume`; `None` otherwise |

**Emitted by:**

- `verify_message`
- `verify_message_and_consume`
- `verify_batch` (once per successfully verified message)
- `verify_signed_batch` (once per successfully verified message)
- `verify_signed_message`

---

## CLI Usage (soroban-cli)

All examples below use `soroban-cli`. Replace `<CONTRACT_ID>` with your deployed contract address and `<ADMIN_SECRET>` with the admin's secret key.

### Prerequisites

```bash
# Install soroban-cli (version aligned with SDK 22)
cargo install soroban-cli --version 22.0.0 --locked

# Configure network (testnet)
soroban network add testnet \
  --rpc-url https://soroban-testnet.stellar.org \
  --network-passphrase "Test SDF Network ; September 2015"

# Add your identity
soroban keys generate admin --network testnet
soroban keys generate relayer --network testnet
```

---

### Step 1 — Deploy the Contract

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/cross_chain_verifier.wasm \
  --source admin \
  --network testnet
# Output: CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
```

---

### Step 2 — Initialize

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source admin \
  --network testnet \
  -- initialize \
  --admin $(soroban keys address admin)
```

---

### Step 3 — Post a State Root (Relayer)

After the relayer builds the Merkle tree off-chain (see [Off-Chain Tree Building](#off-chain-tree-building)), it posts the root:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source relayer \
  --network testnet \
  -- update_root \
  --block_height 1000 \
  --new_root 0xaabbccdd...  # 32-byte hex root hash
```

**Example with a known root:**
```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source relayer \
  --network testnet \
  -- update_root \
  --block_height 1000 \
  --new_root "aabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccdd"
```

---

### Step 4 — Query a Stored Root

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- get_root \
  --block_height 1000
# Output: "aabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccdd"
```

---

### Step 5 — Verify a Message Inclusion Proof

This is the core operation. Any party can call this to prove a message was included in a block.

**Simple 2-level proof (leaf B in a 4-leaf tree A, B, C, D):**

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- verify_message \
  --block_height 1000 \
  --leaf "0202020202020202020202020202020202020202020202020202020202020202" \
  --proof '["0101010101010101010101010101010101010101010101010101010101010101", \
             "0404040404040404040404040404040404040404040404040404040404040404"]' \
  --proof_flags '[true, false]'
# Output: true
```

**Proof flag reference:**
- `true`  → the sibling hash goes on the **left**: `SHA-256(sibling || current)`
- `false` → the sibling hash goes on the **right**: `SHA-256(current || sibling)`

**Single-leaf tree (leaf is the root):**
```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- verify_message \
  --block_height 500 \
  --leaf "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890" \
  --proof '[]' \
  --proof_flags '[]'
# Output: true  (only if the stored root equals the leaf exactly)
```

---

## Off-Chain Tree Building

Use the `MerkleTree` utility in `core/src/merkle_tree.rs` to build trees and generate proofs off-chain before posting roots or verifying messages. See [`core/MERKLE_TREE_README.md`](../../core/MERKLE_TREE_README.md) for full documentation.

**Quick example (Rust):**

```rust
use Sky Moon Scope_core::merkle_tree::MerkleTree;

// 1. Hash your messages
let messages: Vec<Vec<u8>> = vec![
    sha256(b"transfer: Alice -> Bob, 100 XLM"),
    sha256(b"transfer: Carol -> Dave, 50 XLM"),
    sha256(b"transfer: Eve -> Frank, 200 XLM"),
    sha256(b"transfer: Grace -> Heidi, 75 XLM"),
];

// 2. Build the tree
let mut tree = MerkleTree::new(32);
tree.build(messages.clone()).expect("build failed");

// 3. Get the root to post on-chain
let root_hex = tree.get_root_hex();
println!("Root: {}", root_hex);

// 4. Generate a proof for message at index 1 (Carol -> Dave)
let (proof, proof_flags) = tree.generate_proof(1).expect("proof failed");
```

---

## End-to-End Example

This walkthrough demonstrates the full relayer → verifier flow.

### Scenario
A bridge relayer monitors an EVM chain. Block 1000 contains 4 token transfer messages. The relayer posts the Merkle root to Soroban, and a recipient proves their transfer was included.

```bash
# --- Off-chain (relayer script) ---

# Messages in block 1000 (SHA-256 hashed)
LEAF_0="1111111111111111111111111111111111111111111111111111111111111111"  # transfer A
LEAF_1="2222222222222222222222222222222222222222222222222222222222222222"  # transfer B  ← prove this
LEAF_2="3333333333333333333333333333333333333333333333333333333333333333"  # transfer C
LEAF_3="4444444444444444444444444444444444444444444444444444444444444444"  # transfer D

# Build tree off-chain, compute root
# ROOT = SHA256(SHA256(LEAF_0||LEAF_1) || SHA256(LEAF_2||LEAF_3))

# --- On-chain (relayer posts root) ---
soroban contract invoke \
  --id <CONTRACT_ID> --source relayer --network testnet \
  -- update_root \
  --block_height 1000 \
  --new_root "<computed_root_hex>"

# --- On-chain (recipient verifies their transfer) ---
# Proof for LEAF_1:
#   Step 1: sibling = LEAF_0 (left),  current = SHA256(LEAF_0 || LEAF_1)
#   Step 2: sibling = SHA256(LEAF_2 || LEAF_3) (right), current = ROOT

soroban contract invoke \
  --id <CONTRACT_ID> --network testnet \
  -- verify_message \
  --block_height 1000 \
  --leaf "2222222222222222222222222222222222222222222222222222222222222222" \
  --proof '["1111111111111111111111111111111111111111111111111111111111111111", \
             "<sha256_of_leaf2_leaf3>"]' \
  --proof_flags '[true, false]'
# Output: true ✓
```

---

## Error Reference

| Condition | Behaviour |
|---|---|
| `initialize` called twice | Panics: `"already initialized"` |
| `update_root` called by non-admin | Transaction rejected (auth failure) |
| `verify_message` with unknown `block_height` | Panics: `"State root not found"` |
| `proof.len() != proof_flags.len()` | Panics: `"Invalid proof format"` |
| Proof does not reconstruct the stored root | Returns `false` |
| Verification succeeds | Emits a `verify` event (see [Events](#events)) |

---

## Security Considerations

- **Relayer trust**: The contract trusts whoever holds the admin key to post correct roots. In production, the admin should be a multi-sig account or a decentralized relayer network.
- **Root history**: Only the latest root per block height is stored. If a relayer posts an incorrect root and then corrects it, the old root is overwritten. Consumers should verify the root matches the source chain before relying on it.
- **Proof malleability**: The SHA-256 hash function used here is collision-resistant. Proofs cannot be forged without breaking SHA-256.
- **Replay protection**: successful payload, signed-message, and bare-leaf verification consumes the supplied nonce. Nonces are global to this verifier instance, so callers must ensure they are unique across all source chains and message types.
- **Event observability**: Successful verifications emit a `verify` event, allowing off-chain indexers to track verification activity without polling contract state.

---

## Testing

```bash
# Run all cross_chain_verifier tests
cargo test -p cross-chain-verifier

# Run a specific test
cargo test -p cross-chain-verifier test_verify_message_success -- --nocapture
```

Test coverage includes:
- `test_initialization` — happy-path init
- `test_double_initialization` — panics on second init
- `test_root_update` — relayer posts and retrieves a root
- `test_verify_message_success` — valid 2-level proof returns `true`
- `test_verify_message_no_root` — panics when block height has no root
- `test_verify_message_emits_event` — successful verification emits a `verify` event

---

## Related

- [`core/MERKLE_TREE_README.md`](../../core/MERKLE_TREE_README.md) — Off-chain tree builder and proof generator
- [`contracts/private_transfer`](../private_transfer/) — ZK-style private transfers using a similar commitment/nullifier pattern
- [`contracts/batch_transfer`](../batch_transfer/) — Batch token transfers that can be verified via Merkle inclusion
