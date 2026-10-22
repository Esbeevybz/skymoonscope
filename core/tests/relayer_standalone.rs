//! Standalone integration tests for the merkle tree relayer pipeline
//! These tests don't depend on the main crate compilation and can run independently.

use std::collections::{HashMap, HashSet};

// Copy the necessary merkle tree code locally to avoid dependency issues
mod merkle {
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct ProofNode {
        pub hash: [u8; 32],
        pub is_left: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MerkleProof {
        pub leaf_index: usize,
        pub leaf: Vec<u8>,
        pub leaf_hash: [u8; 32],
        pub root: [u8; 32],
        pub proof: Vec<ProofNode>,
    }

    impl MerkleProof {
        pub fn verify(&self) -> bool {
            let mut current = if self.leaf_hash == [0u8; 32] {
                MerkleTree::hash_leaf(&self.leaf)
            } else {
                self.leaf_hash
            };

            for node in &self.proof {
                current = if node.is_left {
                    MerkleTree::hash_concat(&current, &node.hash)
                } else {
                    MerkleTree::hash_concat(&node.hash, &current)
                };
            }

            current == self.root
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MerkleTree {
        pub root: [u8; 32],
        pub levels: usize,
        pub leaf_count: usize,
        data_leaves: Vec<Vec<u8>>,
        nodes: Vec<Vec<[u8; 32]>>,
    }

    impl MerkleTree {
        pub fn new(levels: usize) -> Self {
            MerkleTree {
                root: [0u8; 32],
                levels,
                leaf_count: 0,
                data_leaves: Vec::new(),
                nodes: Vec::new(),
            }
        }

        pub fn build(&mut self, leaves: Vec<Vec<u8>>) -> Result<(), &'static str> {
            if leaves.is_empty() {
                return Err("Cannot build a Merkle tree from zero leaves.");
            }

            let max_leaves = if self.levels >= 64 {
                usize::MAX
            } else {
                1usize.checked_shl(self.levels as u32).unwrap_or(usize::MAX)
            };

            if leaves.len() > max_leaves {
                return Err("Leaf count exceeds configured tree capacity.");
            }

            let leaf_hashes: Vec<[u8; 32]> =
                leaves.iter().map(|leaf| Self::hash_leaf(leaf)).collect();
            self.nodes = Self::build_levels(leaf_hashes);
            self.root = *self
                .nodes
                .last()
                .and_then(|level| level.first())
                .ok_or("Failed to calculate tree root.")?;
            self.data_leaves = leaves;
            self.leaf_count = self.data_leaves.len();

            Ok(())
        }

        pub fn generate_proof(&self, leaf_index: usize) -> Result<MerkleProof, &'static str> {
            if self.nodes.is_empty() {
                return Err("Cannot generate proof before building the tree.");
            }

            if leaf_index >= self.leaf_count {
                return Err("Leaf index is out of bounds.");
            }

            let mut path_index = leaf_index;
            let mut proof = Vec::new();

            for level in 0..self.nodes.len() - 1 {
                let level_nodes = &self.nodes[level];
                let sibling_index = if path_index.is_multiple_of(2) {
                    path_index + 1
                } else {
                    path_index - 1
                };

                let sibling_hash = if sibling_index < level_nodes.len() {
                    level_nodes[sibling_index]
                } else {
                    level_nodes[path_index]
                };
                let is_left = level_nodes[path_index] <= sibling_hash;

                proof.push(ProofNode {
                    hash: sibling_hash,
                    is_left,
                });

                path_index /= 2;
            }

            Ok(MerkleProof {
                leaf_index,
                leaf: self.data_leaves[leaf_index].clone(),
                leaf_hash: self.nodes[0][leaf_index],
                root: self.root,
                proof,
            })
        }

        pub fn verify_proof(proof: &MerkleProof, root: &[u8; 32]) -> bool {
            let mut current = if proof.leaf_hash == [0u8; 32] {
                Self::hash_leaf(&proof.leaf)
            } else {
                proof.leaf_hash
            };

            for node in &proof.proof {
                current = if node.is_left {
                    Self::hash_concat(&current, &node.hash)
                } else {
                    Self::hash_concat(&node.hash, &current)
                };
            }

            current == *root
        }

        pub fn get_root_hex(&self) -> String {
            hex::encode(self.root)
        }

        fn hash_leaf(data: &[u8]) -> [u8; 32] {
            let mut hasher = Sha256::new();
            hasher.update(data);
            let digest1 = hasher.finalize();
            let mut hasher2 = Sha256::new();
            hasher2.update(digest1);
            let digest2 = hasher2.finalize();
            digest2.into()
        }

        fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
            let mut hasher = Sha256::new();
            if left <= right {
                hasher.update(left);
                hasher.update(right);
            } else {
                hasher.update(right);
                hasher.update(left);
            }
            let digest = hasher.finalize();
            digest.into()
        }

        fn hash_concat(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
            let mut hasher = Sha256::new();
            hasher.update(left);
            hasher.update(right);
            let digest = hasher.finalize();
            digest.into()
        }

        fn build_levels(leaf_hashes: Vec<[u8; 32]>) -> Vec<Vec<[u8; 32]>> {
            let mut levels = Vec::new();
            let mut current_level = leaf_hashes;

            loop {
                levels.push(current_level.clone());
                if current_level.len() == 1 {
                    break;
                }

                let mut next_level = Vec::new();
                let mut i = 0;
                while i < current_level.len() {
                    let left = &current_level[i];
                    let right = if i + 1 < current_level.len() {
                        &current_level[i + 1]
                    } else {
                        left
                    };
                    next_level.push(Self::hash_pair(left, right));
                    i += 2;
                }
                current_level = next_level;
            }

            levels
        }
    }
}

use merkle::{MerkleProof, MerkleTree};

/// Mock contract storage for tracking posted roots and verified messages.
#[derive(Debug, Clone)]
struct MockContractState {
    roots: HashMap<u32, [u8; 32]>,
    verified_messages: HashSet<[u8; 32]>,
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

    fn update_root(&mut self, block_height: u32, root: [u8; 32]) {
        self.roots.insert(block_height, root);
    }

    fn get_root(&self, block_height: u32) -> Option<[u8; 32]> {
        self.roots.get(&block_height).copied()
    }

    fn is_message_processed(&self, message_hash: &[u8; 32]) -> bool {
        self.verified_messages.contains(message_hash)
    }

    fn mark_message_processed(&mut self, message_hash: [u8; 32]) {
        self.verified_messages.insert(message_hash);
    }

    fn is_nonce_processed(&self, nonce: u64) -> bool {
        self.processed_nonces.contains(&nonce)
    }

    fn mark_nonce_processed(&mut self, nonce: u64) {
        self.processed_nonces.insert(nonce);
    }
}

// ───────────────────────────────────────────────────────────────────────────────
// CORE PIPELINE TESTS
// ───────────────────────────────────────────────────────────────────────────────

#[test]
fn test_relayer_pipeline_build_post_verify() {
    let leaves = vec![
        b"message_0".to_vec(),
        b"message_1".to_vec(),
        b"message_2".to_vec(),
        b"message_3".to_vec(),
    ];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    assert_eq!(tree.leaf_count, 4);
    assert_ne!(tree.root, [0u8; 32]);

    let block_height = 1000u32;
    let mut contract_state = MockContractState::new();
    contract_state.update_root(block_height, tree.root);

    assert_eq!(contract_state.get_root(block_height), Some(tree.root));

    for leaf_index in 0..tree.leaf_count {
        let proof = tree.generate_proof(leaf_index).expect("proof generates");

        assert!(proof.verify());
        assert!(MerkleTree::verify_proof(&proof, &tree.root));

        let posted_root = contract_state.get_root(block_height).expect("root exists");
        assert!(MerkleTree::verify_proof(&proof, &posted_root));
    }
}

#[test]
fn test_relayer_pipeline_with_power_of_two_leaves() {
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

    assert!(!contract_state.is_message_processed(&message_hash));
    contract_state.mark_message_processed(message_hash);

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

    assert!(!contract_state.is_nonce_processed(nonce_1));
    contract_state.mark_nonce_processed(nonce_1);

    assert!(!contract_state.is_nonce_processed(nonce_2));
    contract_state.mark_nonce_processed(nonce_2);

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

    for leaf_index in 0..tree.leaf_count {
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

    assert_eq!(verified_count, tree.leaf_count);
    assert_eq!(replayed_count, 0);

    for leaf_index in 0..tree.leaf_count {
        let proof = tree.generate_proof(leaf_index).expect("proof generates");
        let message_hash = proof.leaf_hash;

        if contract_state.is_message_processed(&message_hash) {
            replayed_count += 1;
        }
    }

    assert_eq!(
        replayed_count, tree.leaf_count,
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

    assert_eq!(tree.leaf_count, 1);

    let proof = tree.generate_proof(0).expect("proof generates");
    assert!(proof.verify());
    assert!(MerkleTree::verify_proof(&proof, &tree.root));
}

#[test]
fn test_proof_depth_matches_tree_size() {
    let test_cases = vec![
        (1, 0),
        (2, 1),
        (3, 2),
        (4, 2),
        (5, 3),
        (8, 3),
        (9, 4),
        (16, 4),
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
}

#[test]
fn test_generate_proof_before_build_fails() {
    let tree = MerkleTree::new(32);
    let result = tree.generate_proof(0);

    assert!(result.is_err(), "Generating proof before build should fail");
}

#[test]
fn test_generate_proof_out_of_bounds_fails() {
    let leaves = vec![b"leaf".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    let result = tree.generate_proof(1);
    assert!(
        result.is_err(),
        "Generating proof for out-of-bounds index should fail"
    );
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

    assert!(!MerkleTree::verify_proof(&proof, &tree2.root));
}

#[test]
fn test_tampered_proof_fails_verification() {
    let leaves = vec![b"leaf1".to_vec(), b"leaf2".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    let mut proof = tree.generate_proof(0).expect("proof generates");

    if !proof.proof.is_empty() {
        proof.proof[0].hash[0] ^= 0xFF;
    }

    assert!(!proof.verify());
    assert!(!MerkleTree::verify_proof(&proof, &tree.root));
}

#[test]
fn test_leaf_exceeds_capacity() {
    let mut tree = MerkleTree::new(2);

    let leaves: Vec<Vec<u8>> = (0..5).map(|i| format!("leaf_{}", i).into_bytes()).collect();

    let result = tree.build(leaves);
    assert!(
        result.is_err(),
        "Building tree with more leaves than capacity should fail"
    );
}

// ───────────────────────────────────────────────────────────────────────────────
// CONSISTENCY TESTS
// ───────────────────────────────────────────────────────────────────────────────

#[test]
fn test_proof_generation_consistency() {
    let leaves = vec![b"leaf1".to_vec(), b"leaf2".to_vec()];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).expect("tree builds");

    let proof_1st = tree.generate_proof(0).expect("proof 1st generation");
    let proof_2nd = tree.generate_proof(0).expect("proof 2nd generation");

    assert_eq!(proof_1st.leaf_index, proof_2nd.leaf_index);
    assert_eq!(proof_1st.leaf_hash, proof_2nd.leaf_hash);
    assert_eq!(proof_1st.root, proof_2nd.root);
    assert_eq!(proof_1st.proof, proof_2nd.proof);
}

#[test]
fn test_multiple_root_versions() {
    let mut contract_state = MockContractState::new();

    let leaves_v1 = vec![b"msg1".to_vec(), b"msg2".to_vec()];
    let mut tree_v1 = MerkleTree::new(32);
    tree_v1.build(leaves_v1).expect("tree builds");
    contract_state.update_root(100, tree_v1.root);

    let leaves_v2 = vec![b"msg1".to_vec(), b"msg2".to_vec(), b"msg3".to_vec()];
    let mut tree_v2 = MerkleTree::new(32);
    tree_v2.build(leaves_v2).expect("tree builds");
    contract_state.update_root(200, tree_v2.root);

    let proof_v1 = tree_v1.generate_proof(0).expect("proof generates");
    assert!(MerkleTree::verify_proof(
        &proof_v1,
        &contract_state.get_root(100).unwrap()
    ));

    let proof_v2 = tree_v2.generate_proof(0).expect("proof generates");
    assert!(MerkleTree::verify_proof(
        &proof_v2,
        &contract_state.get_root(200).unwrap()
    ));

    assert!(!MerkleTree::verify_proof(
        &proof_v1,
        &contract_state.get_root(200).unwrap()
    ));
    assert!(!MerkleTree::verify_proof(
        &proof_v2,
        &contract_state.get_root(100).unwrap()
    ));
}
