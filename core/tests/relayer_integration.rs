//! Integration tests for the full relayer pipeline: build → post → verify
//!
//! This test suite exercises the complete workflow:
//! 1. **Build**: Construct a Merkle tree from leaf data
//! 2. **Post**: Simulate posting the root on-chain (mocked contract storage)
//! 3. **Verify**: Generate and verify proofs against the committed root
//!
//! The tests cover:
//! - Core pipeline flow with various tree sizes
//! - Edge cases (empty trees, single leaves, power-of-2 and odd-count trees)
//! - Error handling and replay protection
//! - Batch verification scenarios

#[path = "../src/merkle_tree.rs"]
mod merkle_tree;

use merkle_tree::{MerkleProof, MerkleTree};
use std::collections::{HashMap, HashSet};

/// Mock contract storage for tracking posted roots and verified messages.
/// Simulates on-chain storage for the cross-chain verifier contract.
#[derive(Debug, Clone)]
struct MockContractState {
    /// Block height -> root hash mappings (simulates `DataKey::StateRoot`)
    roots: HashMap<u32, [u8; 32]>,
    /// Processed message hashes (simulates `DataKey::ProcessedMessages`)
    verified_messages: HashSet<[u8; 32]>,
    /// Consumed nonces (simulates `DataKey::ProcessedNonce`)
    processed_nonces: HashSet<u64>,
}

impl MockContractState {
    fn new() -> Self {
        MockContractState {
            roots: HashMap::new(),
            verified_messages: HashSet::new(),
            processed_nonces: HashSet::new(),
        }
    }

    /// Post a root at a block height (admin operation)
    fn update_root(&mut self, block_height: u32, root: [u8; 32]) {
        self.roots.insert(block_height, root);
    }

    /// Retrieve a previously posted root
    fn get_root(&self, block_height: u32) -> Option<[u8; 32]> {
        self.roots.get(&block_height).copied()
    }

    /// Check if a message has been verified
    fn is_message_processed(&self, message_hash: &[u8; 32]) -> bool {
        self.verified_messages.contains(message_hash)
    }

    /// Record a verified message (prevents replay)
    fn mark_message_processed(&mut self, message_hash: [u8; 32]) {
        self.verified_messages.insert(message_hash);
    }

    /// Check if a nonce has been consumed
    fn is_nonce_processed(&self, nonce: u64) -> bool {
        self.processed_nonces.contains(&nonce)
    }

    /// Record a consumed nonce
    fn mark_nonce_processed(&mut self, nonce: u64) {
        self.processed_nonces.insert(nonce);
    }
}

// ───────────────────────────────────────────────────────────────────────────────
// CORE PIPELINE TESTS
// ───────────────────────────────────────────────────────────────────────────────

#[test]
fn test_relayer_pipeline_build_post_verify() {
    // Stage 1: Build tree
    let leaves = vec![
        b"message_0".to_vec(),
        b"message_1".to_vec(),
        b"message_2".to_vec(),
        b"message_3".to_vec(),
    ];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    assert_eq!(tree.leaf_count(), 4);
    assert_ne!(tree.root, [0u8; 32]);

    // Stage 2: Post root on-chain (simulated)
    let block_height = 1000u32;
    let mut contract_state = MockContractState::new();
    contract_state.update_root(block_height, tree.root);

    // Verify root was posted
    assert_eq!(contract_state.get_root(block_height), Some(tree.root));

    // Stage 3: Generate and verify proofs
    for leaf_index in 0..tree.leaf_count() {
        let proof = tree.generate_proof(leaf_index).expect("proof generates");

        // Verify locally
        assert!(proof.verify());
        assert!(MerkleTree::verify_proof(&proof, &tree.root));

        // Verify against posted root
        let posted_root = contract_state.get_root(block_height).expect("root exists");
        assert!(MerkleTree::verify_proof(&proof, &posted_root));
    }
}

#[test]
fn test_relayer_pipeline_with_power_of_two_leaves() {
    // Test with 2, 4, 8, 16 leaves (perfect trees)
    for leaf_count in [2, 4, 8, 16] {
        let leaves: Vec<Vec<u8>> = (0..leaf_count)
            .map(|i| format!("leaf_{}", i).into_bytes())
            .collect();

        let mut tree = MerkleTree::new(32);
        tree.build(leaves).expect("tree builds");

        let mut contract_state = MockContractState::new();
        contract_state.update_root(leaf_count as u32, tree.root);

        for leaf_index in 0..leaf_count {
            let proof = tree.generate_proof(leaf_index).expect("proof generates");
            assert!(
                proof.verify(),
                "Proof should verify for leaf_index {}",
                leaf_index
            );
        }
    }
}

#[test]
fn test_relayer_pipeline_with_odd_leaf_counts() {
    // Test with non-power-of-2 leaf counts
    for leaf_count in [1, 3, 5, 7, 9, 15] {
        let leaves: Vec<Vec<u8>> = (0..leaf_count)
            .map(|i| format!("leaf_{}", i).into_bytes())
            .collect();

        let mut tree = MerkleTree::new(32);
        tree.build(leaves).expect("tree builds");

        let mut contract_state = MockContractState::new();
        contract_state.update_root(leaf_count as u32, tree.root);

        for leaf_index in 0..leaf_count {
            let proof = tree.generate_proof(leaf_index).expect("proof generates");
            assert!(
                proof.verify(),
                "Proof should verify for leaf_index {}",
                leaf_index
            );

            // Verify against contract root
            let posted_root = contract_state
                .get_root(leaf_count as u32)
                .expect("root exists");
            assert!(MerkleTree::verify_proof(&proof, &posted_root));
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────────
// REPLAY PROTECTION TESTS
// ───────────────────────────────────────────────────────────────────────────────

#[test]
fn test_replay_protection_message_hash() {
    let leaves = vec![b"msg1".to_vec(), b"msg2".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    let mut contract_state = MockContractState::new();
    contract_state.update_root(100, tree.root);

    let proof = tree.generate_proof(0).expect("proof generates");
    let message_hash = proof.leaf_hash;

    // First verification succeeds
    assert!(!contract_state.is_message_processed(&message_hash));
    contract_state.mark_message_processed(message_hash);

    // Second verification fails (replay protection)
    assert!(contract_state.is_message_processed(&message_hash));
}

#[test]
fn test_replay_protection_nonce() {
    let leaves = vec![b"nonce_1".to_vec(), b"nonce_2".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    let mut contract_state = MockContractState::new();
    contract_state.update_root(200, tree.root);

    let nonce_1 = 1001u64;
    let nonce_2 = 1002u64;

    // First nonce verification succeeds
    assert!(!contract_state.is_nonce_processed(nonce_1));
    contract_state.mark_nonce_processed(nonce_1);

    // Second nonce verification succeeds (different nonce)
    assert!(!contract_state.is_nonce_processed(nonce_2));
    contract_state.mark_nonce_processed(nonce_2);

    // Replay of first nonce fails
    assert!(contract_state.is_nonce_processed(nonce_1));
}

// ───────────────────────────────────────────────────────────────────────────────
// BATCH VERIFICATION TESTS
// ───────────────────────────────────────────────────────────────────────────────

#[test]
fn test_batch_verification_all_valid() {
    let leaf_count = 10;
    let leaves: Vec<Vec<u8>> = (0..leaf_count)
        .map(|i| format!("batch_msg_{}", i).into_bytes())
        .collect();

    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    let block_height = 300u32;
    let mut contract_state = MockContractState::new();
    contract_state.update_root(block_height, tree.root);

    // Verify all leaves
    let mut verified_count = 0;
    for leaf_index in 0..leaf_count {
        let proof = tree.generate_proof(leaf_index).expect("proof generates");

        if MerkleTree::verify_proof(&proof, &contract_state.get_root(block_height).unwrap()) {
            verified_count += 1;
        }
    }

    assert_eq!(verified_count, leaf_count, "All proofs should verify");
}

#[test]
fn test_batch_verification_with_replay() {
    let leaves: Vec<Vec<u8>> = (0..5).map(|i| format!("msg_{}", i).into_bytes()).collect();

    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    let mut contract_state = MockContractState::new();
    contract_state.update_root(400, tree.root);

    let mut verified_count = 0;
    let mut replayed_count = 0;

    for leaf_index in 0..tree.leaf_count() {
        let proof = tree.generate_proof(leaf_index).expect("proof generates");
        let message_hash = proof.leaf_hash;

        if !contract_state.is_message_processed(&message_hash) {
            if MerkleTree::verify_proof(&proof, &contract_state.get_root(400).unwrap()) {
                verified_count += 1;
                contract_state.mark_message_processed(message_hash);
            }
        } else {
            replayed_count += 1;
        }
    }

    assert_eq!(verified_count, tree.leaf_count());
    assert_eq!(replayed_count, 0);

    // Attempt replays
    for leaf_index in 0..tree.leaf_count() {
        let proof = tree.generate_proof(leaf_index).expect("proof generates");
        let message_hash = proof.leaf_hash;

        if contract_state.is_message_processed(&message_hash) {
            replayed_count += 1;
        }
    }

    assert_eq!(
        replayed_count,
        tree.leaf_count(),
        "All should be detected as replays"
    );
}

// ───────────────────────────────────────────────────────────────────────────────
// EDGE CASE TESTS
// ───────────────────────────────────────────────────────────────────────────────

#[test]
fn test_single_leaf_tree() {
    let leaves = vec![b"single_message".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    assert_eq!(tree.leaf_count(), 1);

    let proof = tree.generate_proof(0).expect("proof generates");
    assert!(proof.verify());
    assert!(MerkleTree::verify_proof(&proof, &tree.root));
}

#[test]
fn test_large_tree_1024_leaves() {
    let leaf_count = 1024;
    let leaves: Vec<Vec<u8>> = (0..leaf_count)
        .map(|i| format!("leaf_{:04}", i).into_bytes())
        .collect();

    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    assert_eq!(tree.leaf_count(), leaf_count);

    // Verify first, middle, and last leaf proofs
    for leaf_index in [0, 512, 1023] {
        let proof = tree.generate_proof(leaf_index).expect("proof generates");
        assert!(proof.verify());
        assert_eq!(proof.proof.len(), 10); // log2(1024) = 10 levels
    }
}

#[test]
fn test_proof_depth_matches_tree_size() {
    // Proof depth should be ceil(log2(leaf_count))
    let test_cases = vec![
        (1, 0),  // 1 leaf: depth 0
        (2, 1),  // 2 leaves: depth 1
        (3, 2),  // 3 leaves: depth 2
        (4, 2),  // 4 leaves: depth 2
        (5, 3),  // 5 leaves: depth 3
        (8, 3),  // 8 leaves: depth 3
        (9, 4),  // 9 leaves: depth 4
        (16, 4), // 16 leaves: depth 4
    ];

    for (leaf_count, expected_depth) in test_cases {
        let leaves: Vec<Vec<u8>> = (0..leaf_count)
            .map(|i| format!("leaf_{}", i).into_bytes())
            .collect();

        let mut tree = MerkleTree::new(32);
        tree.build(leaves).expect("tree builds");

        let proof = tree.generate_proof(0).expect("proof generates");
        assert_eq!(
            proof.proof.len(),
            expected_depth,
            "Leaf count: {}",
            leaf_count
        );
    }
}

// ───────────────────────────────────────────────────────────────────────────────
// ERROR HANDLING TESTS
// ───────────────────────────────────────────────────────────────────────────────

#[test]
fn test_build_empty_tree_fails() {
    let mut tree = MerkleTree::new(32);
    let result = tree.build(vec![]);

    assert!(result.is_err(), "Building with empty leaves should fail");
    assert_eq!(
        result.unwrap_err(),
        "Cannot build a Merkle tree from zero leaves."
    );
}

#[test]
fn test_generate_proof_before_build_fails() {
    let tree = MerkleTree::new(32);
    let result = tree.generate_proof(0);

    assert!(result.is_err(), "Generating proof before build should fail");
    assert_eq!(
        result.unwrap_err(),
        "Cannot generate proof before building the tree."
    );
}

#[test]
fn test_generate_proof_out_of_bounds_fails() {
    let leaves = vec![b"leaf".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    let result = tree.generate_proof(1); // Only index 0 exists
    assert!(
        result.is_err(),
        "Generating proof for out-of-bounds index should fail"
    );
    assert_eq!(result.unwrap_err(), "Leaf index is out of bounds.");
}

#[test]
fn test_verify_proof_wrong_root_fails() {
    let leaves = vec![b"leaf1".to_vec(), b"leaf2".to_vec()];
    let mut tree1 = MerkleTree::new(32);
    tree1.build(leaves.clone()).expect("tree1 builds");

    let mut tree2 = MerkleTree::new(32);
    let different_leaves = vec![b"different".to_vec()];
    tree2.build(different_leaves).expect("tree2 builds");

    let proof = tree1.generate_proof(0).expect("proof generates");

    // Proof should fail against a different tree's root
    assert!(!MerkleTree::verify_proof(&proof, &tree2.root));
}

#[test]
fn test_tampered_proof_fails_verification() {
    let leaves = vec![b"leaf1".to_vec(), b"leaf2".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    let mut proof = tree.generate_proof(0).expect("proof generates");

    // Tamper with the proof
    if !proof.proof.is_empty() {
        proof.proof[0].hash[0] ^= 0xFF; // Flip bits in first hash
    }

    // Tampered proof should fail
    assert!(!proof.verify());
    assert!(!MerkleTree::verify_proof(&proof, &tree.root));
}

#[test]
fn test_leaf_exceeds_capacity() {
    let mut tree = MerkleTree::new(2); // Max 4 leaves

    let leaves: Vec<Vec<u8>> = (0..5).map(|i| format!("leaf_{}", i).into_bytes()).collect();

    let result = tree.build(leaves);
    assert!(
        result.is_err(),
        "Building tree with more leaves than capacity should fail"
    );
    assert_eq!(
        result.unwrap_err(),
        "Leaf count exceeds configured tree capacity."
    );
}

// ───────────────────────────────────────────────────────────────────────────────
// PROOF SERIALIZATION & ROUND-TRIP TESTS
// ───────────────────────────────────────────────────────────────────────────────

#[test]
fn test_proof_serialization_roundtrip() {
    let leaves = vec![b"msg1".to_vec(), b"msg2".to_vec(), b"msg3".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    let original_proof = tree.generate_proof(1).expect("proof generates");

    // Serialize to JSON
    let json = serde_json::to_string(&original_proof).expect("serialization succeeds");

    // Deserialize from JSON
    let deserialized_proof: MerkleProof =
        serde_json::from_str(&json).expect("deserialization succeeds");

    // Verify round-tripped proof
    assert_eq!(original_proof.leaf_index, deserialized_proof.leaf_index);
    assert_eq!(original_proof.leaf_hash, deserialized_proof.leaf_hash);
    assert_eq!(original_proof.root, deserialized_proof.root);
    assert_eq!(original_proof.proof, deserialized_proof.proof);
    assert!(deserialized_proof.verify());
}

// ───────────────────────────────────────────────────────────────────────────────
// MULTIPLE ROOT VERSIONS TEST (simulat root updates)
// ───────────────────────────────────────────────────────────────────────────────

#[test]
fn test_multiple_root_versions() {
    let mut contract_state = MockContractState::new();

    // Post root v1 at block 100
    let leaves_v1 = vec![b"msg1".to_vec(), b"msg2".to_vec()];
    let mut tree_v1 = MerkleTree::new(32);
    tree_v1.build(leaves_v1).expect("tree builds");
    contract_state.update_root(100, tree_v1.root);

    // Post root v2 at block 200
    let leaves_v2 = vec![b"msg1".to_vec(), b"msg2".to_vec(), b"msg3".to_vec()];
    let mut tree_v2 = MerkleTree::new(32);
    tree_v2.build(leaves_v2).expect("tree builds");
    contract_state.update_root(200, tree_v2.root);

    // Verify proof from v1 against v1 root
    let proof_v1 = tree_v1.generate_proof(0).expect("proof generates");
    assert!(MerkleTree::verify_proof(
        &proof_v1,
        &contract_state.get_root(100).unwrap()
    ));

    // Verify proof from v2 against v2 root
    let proof_v2 = tree_v2.generate_proof(0).expect("proof generates");
    assert!(MerkleTree::verify_proof(
        &proof_v2,
        &contract_state.get_root(200).unwrap()
    ));

    // Cross-verification should fail
    assert!(!MerkleTree::verify_proof(
        &proof_v1,
        &contract_state.get_root(200).unwrap()
    ));
    assert!(!MerkleTree::verify_proof(
        &proof_v2,
        &contract_state.get_root(100).unwrap()
    ));
}

// ───────────────────────────────────────────────────────────────────────────────
// CONSISTENCY TESTS
// ───────────────────────────────────────────────────────────────────────────────

#[test]
fn test_tree_consistency_with_data_leaf_preservation() {
    let original_data = vec![b"data1".to_vec(), b"data2".to_vec(), b"data3".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(original_data.clone()).expect("tree builds");

    // Verify that tree preserves leaf data
    for (index, proof) in (0..tree.leaf_count())
        .map(|i| (i, tree.generate_proof(i).expect("proof generates")))
        .collect::<Vec<_>>()
    {
        assert_eq!(
            proof.leaf, original_data[index],
            "Leaf data should be preserved"
        );
        assert_eq!(proof.leaf_index, index);
    }
}

#[test]
fn test_proof_generation_consistency() {
    let leaves = vec![b"leaf1".to_vec(), b"leaf2".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    let proof_1st = tree.generate_proof(0).expect("proof 1st generation");
    let proof_2nd = tree.generate_proof(0).expect("proof 2nd generation");

    // Two consecutive generations should produce identical proofs
    assert_eq!(proof_1st.leaf_index, proof_2nd.leaf_index);
    assert_eq!(proof_1st.leaf_hash, proof_2nd.leaf_hash);
    assert_eq!(proof_1st.root, proof_2nd.root);
    assert_eq!(proof_1st.proof, proof_2nd.proof);
}
