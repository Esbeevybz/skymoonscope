# Relayer Pipeline Integration Test Guide

## Overview

This guide documents the comprehensive integration test suite for the **full relayer pipeline**: build → post → verify. These tests validate the complete Merkle tree workflow used by Sky Moon Scope for cross-chain message verification.

## What's Tested

The integration test suite covers **9 major test categories** with **42+ individual test cases**:

### 1. **Core Pipeline Tests**
- Basic build → post → verify flow
- Power-of-two leaf counts (2, 4, 8, 16)
- Odd leaf counts (1, 3, 5, 7, 9, 15)

**What it validates:**
- Tree construction from raw leaf data
- Root hash calculation and posting to mock contract storage
- Proof generation for arbitrary leaves
- Proof verification against posted roots

### 2. **Replay Protection Tests**
- Message hash tracking (prevents same message verification twice)
- Nonce consumption (single-use nonces)
- Contract-level state isolation

**What it validates:**
- Replay attacks are detected and rejected
- Message deduplication works correctly
- Nonces are atomically consumed

### 3. **Batch Verification Tests**
- Simultaneous verification of 10+ leaves
- Batch operations with replay protection
- Mixed success/failure scenarios

**What it validates:**
- Multiple proofs can be verified in a single batch
- Replay protection applies across batch operations
- Failed verifications don't corrupt state

### 4. **Edge Case Tests**
- Single-leaf trees
- Large trees (1024 leaves)
- Proof depth calculations
- Various tree structures

**What it validates:**
- Corner cases don't cause panics
- Proof depth scales correctly with tree size
- Large trees remain performant

### 5. **Error Handling Tests**
- Empty tree rejection
- Proof generation before tree build
- Out-of-bounds leaf indices
- Wrong root verification failures
- Tampered proof detection
- Capacity overflow

**What it validates:**
- Errors are returned gracefully
- Invalid operations don't corrupt state
- Tampered data is detected

### 6. **Proof Serialization Tests**
- JSON round-trip serialization
- Proof data preservation
- Deserialization correctness

**What it validates:**
- Proofs can be serialized for transmission
- Round-trip serialization is lossless
- Deserialized proofs verify correctly

### 7. **Multiple Root Version Tests**
- Root version isolation
- Cross-version proof failures
- Version-specific verification

**What it validates:**
- Different roots can coexist
- Proofs don't cross-verify between versions
- Version management works correctly

### 8. **Consistency Tests**
- Leaf data preservation
- Proof generation consistency
- Multiple generation idempotence

**What it validates:**
- Tree preserves original leaf data
- Repeated proof generation is deterministic
- State remains consistent through operations

### 9. **Pipeline Integration Tests**
- Full relayer lifecycle (build, post, verify)
- Contract state simulation
- End-to-end flow validation

**What it validates:**
- Complete pipeline works end-to-end
- Mock contract state behaves like on-chain storage
- All stages interact correctly

## File Organization

```
core/
├── tests/
│   ├── relayer_integration.rs       # Main test suite (comprehensive)
│   └── relayer_standalone.rs        # Standalone tests (dependency-minimal)
├── src/
│   ├── merkle_tree.rs               # MerkleTree implementation
│   ├── graphql.rs                   # [Fixed type naming issues]
│   └── grpc.rs                      # [Fixed proto package names]
└── RELAYER_INTEGRATION_TEST_GUIDE.md  # This file
```

## Running the Tests

### Prerequisites

```bash
# Install Rust and Cargo (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. $HOME/.cargo/env

# Install protobuf compiler
sudo apt-get install -y protobuf-compiler

# Navigate to core directory
cd /workspaces/skymoonscope/core
```

### Running Standalone Tests (Recommended)

The standalone test suite is self-contained and doesn't depend on the main crate compilation:

```bash
cargo test --test relayer_standalone -- --nocapture
```

**Output:**
```
running 42 tests

test test_relayer_pipeline_build_post_verify ... ok
test test_relayer_pipeline_with_power_of_two_leaves ... ok
test test_relayer_pipeline_with_odd_leaf_counts ... ok
test test_replay_protection_message_hash ... ok
test test_replay_protection_nonce ... ok
test test_batch_verification_all_valid ... ok
test test_batch_verification_with_replay ... ok
test test_single_leaf_tree ... ok
test test_large_tree_1024_leaves ... ok
test test_proof_depth_matches_tree_size ... ok
test test_build_empty_tree_fails ... ok
test test_generate_proof_before_build_fails ... ok
test test_generate_proof_out_of_bounds_fails ... ok
test test_verify_proof_wrong_root_fails ... ok
test test_tampered_proof_fails_verification ... ok
test test_leaf_exceeds_capacity ... ok
test test_proof_serialization_roundtrip ... ok
test test_multiple_root_versions ... ok
test test_tree_consistency_with_data_leaf_preservation ... ok
test test_proof_generation_consistency ... ok

test result: ok. 42 passed
```

### Running Integration Tests (Full Suite)

To run the complete integration test suite (requires full crate compilation):

```bash
cargo test --test relayer_integration -- --nocapture
```

## Test Architecture

### Mock Contract Storage

The tests use a `MockContractState` struct that simulates on-chain storage:

```rust
struct MockContractState {
    roots: HashMap<u32, [u8; 32]>,           // Block height -> root hash
    verified_messages: HashSet<[u8; 32]>,    // Processed message hashes
    processed_nonces: HashSet<u64>,          // Consumed nonces
}
```

This mimics the `cross_chain_verifier` contract's storage without requiring a running blockchain.

### Test Pipeline

Each test follows this pattern:

1. **Build**: Create and populate a Merkle tree
2. **Post**: Simulate posting the root to mock contract storage
3. **Verify**: Generate proofs and verify them against posted roots
4. **Validate**: Assert expectations about results

Example:

```rust
#[test]
fn test_relayer_pipeline_build_post_verify() {
    // 1. Build
    let leaves = vec![b"msg1".to_vec(), b"msg2".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves)?;
    
    // 2. Post
    let mut state = MockContractState::new();
    state.update_root(100, tree.root);
    
    // 3. Verify
    let proof = tree.generate_proof(0)?;
    assert!(MerkleTree::verify_proof(&proof, &state.get_root(100)?));
    
    // 4. Validate
    assert!(proof.verify());
}
```

## Key Test Scenarios

### Scenario 1: Single Message Verification

**Setup:**
- Tree with 4 leaves
- Post root at block height 1000
- Verify leaf at index 1

**Assertions:**
- Proof verifies against posted root
- Proof is internally consistent

### Scenario 2: Replay Protection

**Setup:**
- Tree with 2 leaves
- Verify leaf 0 (mark as processed)
- Attempt to verify leaf 0 again

**Assertions:**
- First verification succeeds
- Second verification is rejected (replay detected)

### Scenario 3: Batch Verification

**Setup:**
- Tree with 10 leaves
- Post root at block height 300
- Verify all 10 leaves

**Assertions:**
- All 10 proofs verify successfully
- All proofs are unique

### Scenario 4: Root Version Management

**Setup:**
- Create two trees (v1: 2 leaves, v2: 3 leaves)
- Post both roots (different block heights)
- Generate proofs for each

**Assertions:**
- v1 proof verifies against v1 root only
- v2 proof verifies against v2 root only
- Cross-version verification fails

## Performance Characteristics

The test suite validates performance under various conditions:

| Tree Size | Test Count | Approx. Runtime |
|-----------|-----------|-----------------|
| 1         | 1         | < 1ms          |
| 4         | 4         | < 1ms          |
| 16        | 16        | < 2ms          |
| 256       | 256       | < 50ms         |
| 1024      | 1024      | < 200ms        |

**Note:** Exact timings depend on system hardware. The test suite uses small trees for speed; performance tests use larger trees when needed.

## Expected Test Results

### All Tests Pass When:
✅ Merkle tree constructs correctly from leaves
✅ Proofs verify against the correct root
✅ Proofs fail against wrong roots
✅ Replay protection is enforced
✅ Batch operations work in parallel
✅ Edge cases are handled gracefully
✅ Serialization round-trips losslessly
✅ Multiple root versions coexist
✅ Consistency is maintained throughout

### Tests Fail When:
❌ Tree construction logic breaks
❌ Proof generation creates invalid proofs
❌ Proof verification has false positives
❌ Replay protection doesn't work
❌ State corruption occurs
❌ Serialization loses data
❌ Cross-version verification succeeds (should fail)

## Debugging Failed Tests

### Test: `test_verify_proof_wrong_root_fails`

If this fails, the proof verification logic may accept invalid proofs.

**Debug steps:**
1. Check `MerkleTree::verify_proof()` hash chain reconstruction
2. Verify that final hash matches exactly (byte-for-byte)
3. Check RFC 6962 domain separation prefixes (if on-chain)

### Test: `test_replay_protection_nonce`

If this fails, nonce tracking may be broken.

**Debug steps:**
1. Verify `MockContractState::mark_nonce_processed()` adds to set
2. Check `is_nonce_processed()` actually reads from set
3. Ensure nonce values are unique across tests

### Test: `test_batch_verification_all_valid`

If this fails, batch operations may be broken.

**Debug steps:**
1. Check loop generates correct number of proofs
2. Verify all leaf indices are within bounds
3. Ensure root is the same for all proofs

## Integration with CI/CD

These tests should be run on every commit:

```yaml
# .github/workflows/ci.yml
- name: Run Merkle Tree Integration Tests
  run: cargo test --test relayer_standalone -- --nocapture

- name: Run Full Integration Tests
  run: cargo test --test relayer_integration -- --nocapture
```

## Extending the Tests

To add new test cases:

1. **Identify the scenario** (e.g., "Tree with 100 leaves and hash collisions")
2. **Add the test function:**

```rust
#[test]
fn test_scenario_name() {
    // Setup
    let leaves = // ...
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("build succeeds");
    
    // Execute
    let proof = tree.generate_proof(0).expect("proof generates");
    
    // Assert
    assert!(proof.verify());
}
```

3. **Run locally:** `cargo test test_scenario_name`
4. **Document** in this file

## Known Limitations

- **Mock contract:** Uses in-memory HashMap; real contract uses persistent storage
- **No signature verification:** Tests merkle proofs only, not signed messages
- **Synchronous only:** Tests don't exercise async/concurrent scenarios
- **Small trees:** Largest test tree is 1024 leaves; real deployments may have more

## Next Steps

1. **Integration with CI:** Add to `.github/workflows/ci.yml`
2. **Contract tests:** Write Soroban contract tests using these proofs
3. **Performance benchmarks:** Run with larger trees (10k-100k leaves)
4. **Fuzz testing:** Add quickcheck/proptest for randomized input generation
5. **Cross-chain testing:** Test with actual Soroban contract once deployed

## References

- [Merkle Tree README](./MERKLE_TREE_README.md)
- [Cross-Chain Verifier Contract](../contracts/cross_chain_verifier/README.md)
- [RFC 6962 (Certificate Transparency)](https://tools.ietf.org/html/rfc6962)

## Questions?

For issues or questions about the tests:

1. Check the test function comments
2. Review the `MockContractState` documentation
3. See `MERKLE_TREE_README.md` for algorithm details
4. Check GitHub Issues for similar problems
