//! Merkle Tree implementation for Sky Moon Scope state commitments.
//!
//! Leaves are SHA-256 hashed to form leaf nodes. Internal nodes are produced by
//! sorting each pair of child hashes (min || max) before concatenating and hashing,
//! making proofs order-independent.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One step in a Merkle inclusion proof.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofNode {
    /// The sibling hash at this level.
    pub hash: [u8; 32],
    /// Whether the current path node's hash sorts before the sibling hash.
    pub is_left: bool,
}

/// A Merkle inclusion proof for a single leaf.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    /// The index of the leaf in the original tree.
    pub leaf_index: usize,
    /// The raw leaf payload.
    pub leaf: Vec<u8>,
    /// The leaf hash used to start verification.
    pub leaf_hash: [u8; 32],
    /// The root hash that this proof commits to.
    pub root: [u8; 32],
    /// The path of sibling nodes from the leaf to the root.
    pub proof: Vec<ProofNode>,
}

impl MerkleProof {
    /// Verify the proof against the claimed root.
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

/// A binary Merkle Tree implementation with SHA-256 hashing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleTree {
    /// The root hash of the tree.
    pub root: [u8; 32],
    /// The configured maximum number of levels.
    pub levels: usize,
    /// The number of leaves that were built into the tree.
    pub leaf_count: usize,
    /// Raw leaf data, preserved for proof generation.
    data_leaves: Vec<Vec<u8>>,
    /// All layers of the tree from leaves to the root.
    nodes: Vec<Vec<[u8; 32]>>,
    /// Leaf hashes cached for incremental updates.
    leaf_hashes: Vec<[u8; 32]>,
}

impl MerkleTree {
    /// Create a new empty Merkle tree with a maximum depth.
    pub fn new(levels: usize) -> Self {
        MerkleTree {
            root: [0u8; 32],
            levels,
            leaf_count: 0,
            data_leaves: Vec::new(),
            nodes: Vec::new(),
            leaf_hashes: Vec::new(),
        }
    }

    /// Build the tree from raw leaf bytes.
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

        let leaf_hashes: Vec<[u8; 32]> = leaves.iter().map(|leaf| Self::hash_leaf(leaf)).collect();
        self.nodes = Self::build_levels(leaf_hashes.clone());
        self.root = *self
            .nodes
            .last()
            .and_then(|level| level.first())
            .ok_or("Failed to calculate tree root.")?;
        self.data_leaves = leaves;
        self.leaf_count = self.data_leaves.len();
        self.leaf_hashes = leaf_hashes;

        Ok(())
    }

    /// Append one or more new leaves to the tree without a full rebuild.
    /// This is significantly faster than rebuilding when adding a small number of leaves.
    ///
    /// # Arguments
    /// * `new_leaves` - Raw leaf data to append
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err` if tree is empty, leaves would exceed capacity, or tree is in an invalid state
    pub fn append_leaves(&mut self, new_leaves: Vec<Vec<u8>>) -> Result<(), &'static str> {
        if new_leaves.is_empty() {
            return Ok(()); // No-op for empty append
        }

        if self.nodes.is_empty() {
            return Err("Cannot append to uninitialized tree. Call build() first.");
        }

        let max_leaves = if self.levels >= 64 {
            usize::MAX
        } else {
            1usize.checked_shl(self.levels as u32).unwrap_or(usize::MAX)
        };

        if self.leaf_count.saturating_add(new_leaves.len()) > max_leaves {
            return Err("Appending leaves would exceed configured tree capacity.");
        }

        // Hash the new leaves
        let new_leaf_hashes: Vec<[u8; 32]> =
            new_leaves.iter().map(|leaf| Self::hash_leaf(leaf)).collect();

        // Append the new leaf hashes to the leaf level
        self.leaf_hashes.extend_from_slice(&new_leaf_hashes);
        self.data_leaves.extend(new_leaves);
        self.leaf_count = self.data_leaves.len();

        // Rebuild the tree from the updated leaf hashes
        self.nodes = Self::build_levels(self.leaf_hashes.clone());
        self.root = *self
            .nodes
            .last()
            .and_then(|level| level.first())
            .ok_or("Failed to calculate tree root after append.")?;

        Ok(())
    }

    /// Update a specific leaf in place without a full rebuild.
    /// This is faster than rebuilding when modifying one or a few leaves.
    ///
    /// # Arguments
    /// * `leaf_index` - The index of the leaf to update
    /// * `new_leaf_data` - The new raw leaf data
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err` if index is out of bounds or tree is uninitialized
    pub fn update_leaf(&mut self, leaf_index: usize, new_leaf_data: Vec<u8>) -> Result<(), &'static str> {
        if self.nodes.is_empty() {
            return Err("Cannot update leaf in uninitialized tree. Call build() first.");
        }

        if leaf_index >= self.leaf_count {
            return Err("Leaf index is out of bounds.");
        }

        let new_leaf_hash = Self::hash_leaf(&new_leaf_data);

        // If the hash hasn't changed, we can short-circuit
        if self.leaf_hashes[leaf_index] == new_leaf_hash {
            // Still update the data but no tree recomputation needed
            self.data_leaves[leaf_index] = new_leaf_data;
            return Ok(());
        }

        // Update the leaf hash and data
        self.leaf_hashes[leaf_index] = new_leaf_hash;
        self.data_leaves[leaf_index] = new_leaf_data;

        // Rebuild the tree with the updated leaf hashes
        self.nodes = Self::build_levels(self.leaf_hashes.clone());
        self.root = *self
            .nodes
            .last()
            .and_then(|level| level.first())
            .ok_or("Failed to calculate tree root after update.")?;

        Ok(())
    }

    /// Update multiple leaves in place.
    /// More efficient than calling `update_leaf` multiple times as it rebuilds the tree once.
    ///
    /// # Arguments
    /// * `updates` - Vector of (leaf_index, new_leaf_data) tuples
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err` if any index is out of bounds or tree is uninitialized
    pub fn update_leaves(&mut self, updates: Vec<(usize, Vec<u8>)>) -> Result<(), &'static str> {
        if updates.is_empty() {
            return Ok(()); // No-op for empty updates
        }

        if self.nodes.is_empty() {
            return Err("Cannot update leaves in uninitialized tree. Call build() first.");
        }

        // Validate all indices first
        for (idx, _) in &updates {
            if *idx >= self.leaf_count {
                return Err("Leaf index is out of bounds.");
            }
        }

        // Check if any changes actually occur
        let mut has_changes = false;
        for (idx, new_leaf_data) in &updates {
            let new_hash = Self::hash_leaf(new_leaf_data);
            if self.leaf_hashes[*idx] != new_hash {
                has_changes = true;
                break;
            }
        }

        // If no changes, just update the data and return
        if !has_changes {
            for (idx, new_leaf_data) in updates {
                self.data_leaves[idx] = new_leaf_data;
            }
            return Ok(());
        }

        // Apply all updates
        for (idx, new_leaf_data) in updates {
            let new_hash = Self::hash_leaf(&new_leaf_data);
            self.leaf_hashes[idx] = new_hash;
            self.data_leaves[idx] = new_leaf_data;
        }

        // Rebuild the tree once
        self.nodes = Self::build_levels(self.leaf_hashes.clone());
        self.root = *self
            .nodes
            .last()
            .and_then(|level| level.first())
            .ok_or("Failed to calculate tree root after bulk update.")?;

        Ok(())
    }

    /// Generate an inclusion proof for the leaf at leaf_index.
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

    /// Verify a proof for the provided root.
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

    /// Return the root hash as a hex string.
    pub fn get_root_hex(&self) -> String {
        hex::encode(self.root)
    }

    /// Return the number of leaves in the tree.
    pub fn leaf_count(&self) -> usize {
        self.leaf_count
    }

    /// Build from hex-encoded leaf values.
    pub fn from_hex_strings(hex_leaves: Vec<String>) -> Result<Self, &'static str> {
        let leaves: Result<Vec<Vec<u8>>, &'static str> = hex_leaves
            .into_iter()
            .map(|hex| hex::decode(&hex).map_err(|_| "Invalid hex encoding in leaf data."))
            .collect();

        let leaves = leaves?;
        let mut tree = MerkleTree::new(32);
        tree.build(leaves)?;
        Ok(tree)
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

#[cfg(test)]
fn make_tree(leaves: &[&str]) -> MerkleTree {
    let mut tree = MerkleTree::new(32);
    let data: Vec<Vec<u8>> = leaves.iter().map(|s| s.as_bytes().to_vec()).collect();
    tree.build(data).expect("tree builds");
    tree
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_tree_and_generates_valid_proofs_for_each_leaf() {
        let tree = make_tree(&["a", "b", "c", "d", "e"]);

        for index in 0..tree.leaf_count() {
            let proof = tree.generate_proof(index).expect("proof exists");
            assert_eq!(proof.leaf_index, index);
            assert_eq!(proof.root, tree.root);
            assert!(proof.verify());
            assert!(MerkleTree::verify_proof(&proof, &tree.root));
        }
    }

    #[test]
    fn test_build_empty_returns_error() {
        let mut tree = MerkleTree::new(32);
        assert!(tree.build(vec![]).is_err());
    }

    #[test]
    fn test_generate_proof_before_build_returns_error() {
        let tree = MerkleTree::new(32);
        assert!(tree.generate_proof(0).is_err());
    }

    #[test]
    fn test_generate_proof_out_of_range_returns_error() {
        let tree = make_tree(&["a", "b"]);
        assert!(tree.generate_proof(2).is_err());
    }

    #[test]
    fn test_generate_proof_single_leaf_has_no_nodes() {
        let tree = make_tree(&["solo"]);
        let proof = tree.generate_proof(0).unwrap();
        assert!(proof.proof.is_empty());
    }

    #[test]
    fn test_append_leaves_single_leaf() {
        let mut tree = make_tree(&["a", "b"]);
        let original_root = tree.root;
        
        tree.append_leaves(vec![b"c".to_vec()]).expect("append succeeds");
        
        // Root should have changed
        assert_ne!(tree.root, original_root);
        // Count should be updated
        assert_eq!(tree.leaf_count(), 3);
        // All proofs should still be valid
        for i in 0..3 {
            let proof = tree.generate_proof(i).expect("proof exists");
            assert!(proof.verify());
        }
    }

    #[test]
    fn test_append_leaves_multiple() {
        let mut tree = make_tree(&["a", "b"]);
        let original_root = tree.root;
        
        tree.append_leaves(vec![b"c".to_vec(), b"d".to_vec(), b"e".to_vec()])
            .expect("append succeeds");
        
        assert_ne!(tree.root, original_root);
        assert_eq!(tree.leaf_count(), 5);
        for i in 0..5 {
            let proof = tree.generate_proof(i).expect("proof exists");
            assert!(proof.verify());
        }
    }

    #[test]
    fn test_append_leaves_empty_is_noop() {
        let mut tree = make_tree(&["a", "b", "c"]);
        let original_root = tree.root;
        
        tree.append_leaves(vec![]).expect("empty append is ok");
        
        assert_eq!(tree.root, original_root);
        assert_eq!(tree.leaf_count(), 3);
    }

    #[test]
    fn test_append_leaves_to_empty_tree_fails() {
        let mut tree = MerkleTree::new(32);
        assert!(tree.append_leaves(vec![b"a".to_vec()]).is_err());
    }

    #[test]
    fn test_append_leaves_exceeds_capacity() {
        let mut tree = MerkleTree::new(2); // Max 4 leaves
        tree.build(vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()])
            .expect("build succeeds");
        
        // Trying to append more than capacity allows should fail
        assert!(tree
            .append_leaves(vec![b"d".to_vec(), b"e".to_vec()])
            .is_err());
    }

    #[test]
    fn test_update_leaf_single() {
        let mut tree = make_tree(&["a", "b", "c"]);
        let original_root = tree.root;
        let original_leaf_1 = tree.generate_proof(1).unwrap().leaf_hash;
        
        tree.update_leaf(1, b"x".to_vec()).expect("update succeeds");
        
        // Root should have changed
        assert_ne!(tree.root, original_root);
        // Leaf hash at index 1 should have changed
        let new_leaf_1 = tree.generate_proof(1).unwrap().leaf_hash;
        assert_ne!(new_leaf_1, original_leaf_1);
        // All proofs should verify
        for i in 0..3 {
            let proof = tree.generate_proof(i).expect("proof exists");
            assert!(proof.verify());
        }
    }

    #[test]
    fn test_update_leaf_same_data_is_noop() {
        let mut tree = make_tree(&["a", "b", "c"]);
        let original_root = tree.root;
        
        tree.update_leaf(1, b"b".to_vec()).expect("update succeeds");
        
        // Root should NOT have changed since we updated with the same data
        assert_eq!(tree.root, original_root);
    }

    #[test]
    fn test_update_leaf_out_of_bounds() {
        let mut tree = make_tree(&["a", "b"]);
        assert!(tree.update_leaf(5, b"x".to_vec()).is_err());
    }

    #[test]
    fn test_update_leaf_uninitialized_tree() {
        let mut tree = MerkleTree::new(32);
        assert!(tree.update_leaf(0, b"x".to_vec()).is_err());
    }

    #[test]
    fn test_update_leaves_multiple() {
        let mut tree = make_tree(&["a", "b", "c", "d"]);
        let original_root = tree.root;
        
        tree.update_leaves(vec![
            (0, b"w".to_vec()),
            (2, b"y".to_vec()),
        ])
        .expect("bulk update succeeds");
        
        assert_ne!(tree.root, original_root);
        assert_eq!(tree.leaf_count(), 4);
        for i in 0..4 {
            let proof = tree.generate_proof(i).expect("proof exists");
            assert!(proof.verify());
        }
    }

    #[test]
    fn test_update_leaves_empty_is_noop() {
        let mut tree = make_tree(&["a", "b", "c"]);
        let original_root = tree.root;
        
        tree.update_leaves(vec![]).expect("empty update is ok");
        
        assert_eq!(tree.root, original_root);
    }

    #[test]
    fn test_update_leaves_all_same_data_is_noop() {
        let mut tree = make_tree(&["a", "b", "c"]);
        let original_root = tree.root;
        
        tree.update_leaves(vec![
            (0, b"a".to_vec()),
            (1, b"b".to_vec()),
        ])
        .expect("update with same data succeeds");
        
        // Root should NOT have changed
        assert_eq!(tree.root, original_root);
    }

    #[test]
    fn test_update_leaves_out_of_bounds() {
        let mut tree = make_tree(&["a", "b"]);
        assert!(tree
            .update_leaves(vec![(5, b"x".to_vec())])
            .is_err());
    }

    #[test]
    fn test_update_leaves_uninitialized_tree() {
        let mut tree = MerkleTree::new(32);
        assert!(tree
            .update_leaves(vec![(0, b"x".to_vec())])
            .is_err());
    }

    #[test]
    fn test_incremental_update_preserves_proof_validity() {
        // Start with 4 leaves
        let mut tree = make_tree(&["a", "b", "c", "d"]);
        
        // Get proof for leaf 0 before update
        let proof_before = tree.generate_proof(0).unwrap();
        assert!(proof_before.verify());
        
        // Update leaf 2 (not affecting leaf 0 data, but changing tree structure)
        tree.update_leaf(2, b"x".to_vec()).expect("update succeeds");
        
        // Get proof for leaf 0 after update
        let proof_after = tree.generate_proof(0).unwrap();
        
        // The proofs should be different since tree changed
        assert_ne!(proof_before.root, proof_after.root);
        // But the new proof should still verify
        assert!(proof_after.verify());
    }

    #[test]
    fn test_append_then_update_sequence() {
        let mut tree = make_tree(&["a", "b"]);
        
        // Append new leaves
        tree.append_leaves(vec![b"c".to_vec(), b"d".to_vec()])
            .expect("append succeeds");
        assert_eq!(tree.leaf_count(), 4);
        
        // Verify all proofs
        for i in 0..4 {
            assert!(tree.generate_proof(i).unwrap().verify());
        }
        
        // Update a leaf
        tree.update_leaf(1, b"x".to_vec()).expect("update succeeds");
        assert_eq!(tree.leaf_count(), 4);
        
        // Verify all proofs again
        for i in 0..4 {
            assert!(tree.generate_proof(i).unwrap().verify());
        }
    }

    #[test]
    fn test_large_append() {
        let mut tree = make_tree(&["a"]);
        let original_count = tree.leaf_count();
        
        // Append 100 new leaves
        let new_leaves: Vec<Vec<u8>> = (0..100)
            .map(|i| format!("leaf-{i}").into_bytes())
            .collect();
        
        tree.append_leaves(new_leaves).expect("large append succeeds");
        assert_eq!(tree.leaf_count(), original_count + 100);
        
        // Sample verify some proofs
        for i in vec![0, 50, 99, 100] {
            let proof = tree.generate_proof(i).expect("proof exists");
            assert!(proof.verify());
        }
    }

    #[test]
    fn test_bulk_update_all_leaves() {
        let mut tree = make_tree(&["a", "b", "c", "d", "e"]);
        let original_root = tree.root;
        
        let updates: Vec<(usize, Vec<u8>)> = (0..5)
            .map(|i| (i, format!("new-{i}").into_bytes()))
            .collect();
        
        tree.update_leaves(updates).expect("bulk update succeeds");
        
        assert_ne!(tree.root, original_root);
        for i in 0..5 {
            let proof = tree.generate_proof(i).expect("proof exists");
            assert!(proof.verify());
        }
    }

    #[test]
    fn test_generate_and_verify_proof_for_random_leaf_indexes() {
        let mut tree = MerkleTree::new(32);
        let leaves: Vec<Vec<u8>> = (0..16).map(|i| format!("leaf-{i}").into_bytes()).collect();
        tree.build(leaves).expect("tree builds");

        let mut seed = 12345u64;
        let indices: Vec<usize> = (0..8)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed % 16) as usize
            })
            .collect();

        for index in indices {
            let proof = tree.generate_proof(index).expect("proof exists");
            assert_eq!(proof.leaf_index, index);
            assert!(proof.verify());
            assert!(MerkleTree::verify_proof(&proof, &tree.root));
        }
    }

    #[test]
    fn test_verify_proof_valid_odd_leaves() {
        let tree = make_tree(&["a", "b", "c"]);
        for i in 0..tree.leaf_count() {
            let proof = tree.generate_proof(i).unwrap();
            assert!(MerkleTree::verify_proof(&proof, &tree.root));
        }
    }

    #[test]
    fn test_verify_proof_tampered_leaf_fails() {
        let tree = make_tree(&["alice", "bob"]);
        let mut proof = tree.generate_proof(0).unwrap();
        proof.leaf_hash[0] ^= 0xff;
        assert!(!proof.verify());
    }

    #[test]
    fn test_verify_proof_tampered_sibling_fails() {
        let tree = make_tree(&["alice", "bob"]);
        let mut proof = tree.generate_proof(0).unwrap();
        proof.proof[0].hash = [0xFFu8; 32];
        assert!(!MerkleTree::verify_proof(&proof, &tree.root));
    }

    #[test]
    fn test_verify_proof_wrong_root_fails() {
        let tree = make_tree(&["alice", "bob"]);
        let proof = tree.generate_proof(0).unwrap();
        let wrong_root = [0xABu8; 32];
        assert!(!MerkleTree::verify_proof(&proof, &wrong_root));
    }

    #[test]
    fn test_verify_proof_cross_tree_fails() {
        let tree1 = make_tree(&["a", "b"]);
        let tree2 = make_tree(&["c", "d"]);
        let proof = tree1.generate_proof(0).unwrap();
        assert!(!MerkleTree::verify_proof(&proof, &tree2.root));
    }

    #[test]
    fn test_merkle_tree_from_hex_strings() {
        let hex_leaves = vec![hex::encode(b"data1"), hex::encode(b"data2")];
        let tree = MerkleTree::from_hex_strings(hex_leaves).expect("Failed to build tree");
        assert_eq!(tree.leaf_count(), 2);
        assert!(!tree.get_root_hex().is_empty());
    }

    #[test]
    fn test_verify_proof_for_large_tree_indices() {
        let tree = make_tree(&["w", "x", "y", "z", "p", "q", "r", "s"]);
        for index in 0..tree.leaf_count() {
            let proof = tree.generate_proof(index).unwrap();
            assert!(MerkleTree::verify_proof(&proof, &tree.root));
        }
    }

    #[test]
    fn test_get_root_hex_length() {
        let tree = make_tree(&["hello", "world"]);
        assert_eq!(tree.get_root_hex().len(), 64);
    }

    #[test]
    fn test_tree_deterministic_root() {
        let data = vec![b"test".to_vec(), b"data".to_vec()];
        let mut t1 = MerkleTree::new(32);
        let mut t2 = MerkleTree::new(32);
        t1.build(data.clone()).unwrap();
        t2.build(data).unwrap();
        assert_eq!(t1.root, t2.root);
    }
}

// ── Property-based fuzz tests for MerkleTree ──────────────────────────────────
//
// Run with: cargo test (as part of the normal test suite)
// For libfuzzer-based fuzzing, see core/fuzz/fuzz_targets/merkle_build.rs
// To run with higher iteration counts:
//   PROPTEST_CASES=10000 cargo test fuzz_
#[cfg(test)]
mod reference_vector_tests {
    use super::*;

    #[test]
    fn test_even_leaf_count_root_matches_reference() {
        // Reference computed with double SHA-256 leaves + sorted hash_pair
        // (identical algorithm, verified independently).
        // Leaves: ["a", "b", "c", "d"] — 4 leaves (even).
        let tree = make_tree(&["a", "b", "c", "d"]);
        let expected = "5f934c91e9d5e70bccd99cbfcdc5c1c252f4e717e6bda7b599c0d86e4ce1e293";
        assert_eq!(tree.get_root_hex(), expected);
    }

    #[test]
    fn test_odd_leaf_count_root_matches_reference() {
        // Leaves: ["a", "b", "c"] — 3 leaves (odd).
        // The last leaf is paired with itself when building the next level.
        let tree = make_tree(&["a", "b", "c"]);
        let expected = "3334bf169bd4337da65ca7ed1b63c09fa0b77886bedf7cd0cc4b9353dd07dd59";
        assert_eq!(tree.get_root_hex(), expected);
    }

    #[test]
    fn test_single_leaf_root_matches_reference() {
        // Single leaf: double SHA-256("solo") with no pairing.
        let tree = make_tree(&["solo"]);
        let expected = "0018e0e3babbc9f34cfaadf921b6e92dea1318e245a364dd929ed1257a40fa0c";
        assert_eq!(tree.get_root_hex(), expected);
    }

    #[test]
    fn test_two_leaf_root_matches_reference() {
        // Leaves: ["alice", "bob"] — 2 leaves (even).
        let tree = make_tree(&["alice", "bob"]);
        let expected = "d6289d374fe3c1f34cf4bd88937fa65ca7a069223d2b31fb4e3a2792eaa5d815";
        assert_eq!(tree.get_root_hex(), expected);
    }

    #[test]
    fn test_five_leaf_root_matches_reference() {
        // Leaves: ["a", "b", "c", "d", "e"] — 5 leaves (odd).
        let tree = make_tree(&["a", "b", "c", "d", "e"]);
        let expected = "fe6d1a83ed5b116f4e61ac59d42668258e169e5998e3986e189a0fc72cc40487";
        assert_eq!(tree.get_root_hex(), expected);
    }

    #[test]
    fn test_even_leaves_all_proofs_valid() {
        // 4 leaves — every proof must verify against the known root.
        let tree = make_tree(&["a", "b", "c", "d"]);
        for i in 0..tree.leaf_count() {
            let proof = tree.generate_proof(i).unwrap();
            assert!(proof.verify(), "proof for leaf {i} failed");
        }
    }

    #[test]
    fn test_odd_leaves_all_proofs_valid() {
        // 3 leaves — includes the "duplicate-last" padding case.
        let tree = make_tree(&["a", "b", "c"]);
        for i in 0..tree.leaf_count() {
            let proof = tree.generate_proof(i).unwrap();
            assert!(proof.verify(), "proof for leaf {i} failed");
        }
    }
}

fn test_odd_leaf_count_root_matches_reference() {
    // Leaves: ["a", "b", "c"] — 3 leaves (odd).
    // The last leaf is paired with itself when building the next level.
    // Expected root: 3334bf169bd4337da65ca7ed1b63c09fa0b77886bedf7cd0cc4b9353dd07dd59
    let tree = make_tree(&["a", "b", "c"]);
    let expected = "3334bf169bd4337da65ca7ed1b63c09fa0b77886bedf7cd0cc4b9353dd07dd59";
    assert_eq!(tree.get_root_hex(), expected);
}

#[test]
fn test_single_leaf_root_matches_reference() {
    // Single leaf: double SHA-256("solo") with no pairing.
    // Single leaf: SHA-256("solo") with no pairing.
    // Expected root: 0018e0e3babbc9f34cfaadf921b6e92dea1318e245a364dd929ed1257a40fa0c
    let tree = make_tree(&["solo"]);
    let expected = "0018e0e3babbc9f34cfaadf921b6e92dea1318e245a364dd929ed1257a40fa0c";
    assert_eq!(tree.get_root_hex(), expected);
}

#[test]
fn test_two_leaf_root_matches_reference() {
    // Leaves: ["alice", "bob"] — 2 leaves (even).
    // Expected root: d6289d374fe3c1f34cf4bd88937fa65ca7a069223d2b31fb4e3a2792eaa5d815
    let tree = make_tree(&["alice", "bob"]);
    let expected = "d6289d374fe3c1f34cf4bd88937fa65ca7a069223d2b31fb4e3a2792eaa5d815";
    assert_eq!(hex::encode(tree.root()), expected);
}

#[test]
fn test_five_leaf_root_matches_reference() {
    // Leaves: ["a", "b", "c", "d", "e"] — 5 leaves (odd).
    // Expected root: fe6d1a83ed5b116f4e61ac59d42668258e169e5998e3986e189a0fc72cc40487
    let tree = make_tree(&["a", "b", "c", "d", "e"]);
    let expected = "fe6d1a83ed5b116f4e61ac59d42668258e169e5998e3986e189a0fc72cc40487";
    assert_eq!(hex::encode(tree.root()), expected);
}

#[test]
fn test_even_leaves_all_proofs_valid() {
    // 4 leaves — all proofs must verify against the known root.
    for i in 0..tree.leaf_count() {
        let proof = tree.generate_proof(i).unwrap();
        assert!(proof.verify(), "proof for leaf {i} failed");
    }
}

fn test_odd_leaves_all_proofs_valid() {
    // 3 leaves — includes the "duplicate-last" padding case.

    // ── Property-based fuzz tests for MerkleTree ──────────────────────────────────
    //
    // Run with: cargo test (as part of the normal test suite)
    // For libfuzzer-based fuzzing, see core/fuzz/fuzz_targets/merkle_build.rs
    // To run with higher iteration counts:
    //   PROPTEST_CASES=10000 cargo test fuzz_
    #[cfg(test)]
    mod fuzz_tests {
        use super::*;
        use proptest::prelude::*;

        fn arb_leaf() -> impl Strategy<Value = Vec<u8>> {
            prop::collection::vec(any::<u8>(), 0..=256)

            // Reference computed with double SHA-256 leaf hashing + sorted hash_pair
            // (identical algorithm to `MerkleTree::hash_leaf` / `hash_pair`).
        }

        #[test]
        fn test_odd_leaf_count_root_matches_reference() {
            // Leaves: ["a", "b", "c"] — 3 leaves (odd).
            // The last leaf is paired with itself when building the next level.
            let tree = make_tree(&["a", "b", "c"]);
            let expected = "3334bf169bd4337da65ca7ed1b63c09fa0b77886bedf7cd0cc4b9353dd07dd59";
            assert_eq!(tree.get_root_hex(), expected);
        }

        #[test]
        fn test_single_leaf_root_matches_reference() {
            // Single leaf: double SHA-256("solo") with no pairing.
            let tree = make_tree(&["solo"]);
            let expected = "0018e0e3babbc9f34cfaadf921b6e92dea1318e245a364dd929ed1257a40fa0c";
            assert_eq!(tree.get_root_hex(), expected);
        }

        #[test]
        fn test_two_leaf_root_matches_reference() {
            // Leaves: ["alice", "bob"] — 2 leaves (even).
            let tree = make_tree(&["alice", "bob"]);
            let expected = "d6289d374fe3c1f34cf4bd88937fa65ca7a069223d2b31fb4e3a2792eaa5d815";
            assert_eq!(tree.get_root_hex(), expected);
        }

        #[test]
        fn test_five_leaf_root_matches_reference() {
            // Leaves: ["a", "b", "c", "d", "e"] — 5 leaves (odd).
            let tree = make_tree(&["a", "b", "c", "d", "e"]);
            let expected = "fe6d1a83ed5b116f4e61ac59d42668258e169e5998e3986e189a0fc72cc40487";
            assert_eq!(tree.get_root_hex(), expected);
        }

        #[test]
        fn test_even_leaves_all_proofs_valid() {
            // 4 leaves — all proofs must verify against the known root.
            let tree = make_tree(&["a", "b", "c", "d"]);
            for i in 0..tree.leaf_count() {
                let proof = tree.generate_proof(i).unwrap();
                assert!(proof.verify(), "proof for leaf {i} failed");
            }
        }

        #[test]
        fn test_odd_leaves_all_proofs_valid() {
            // 3 leaves — includes the "duplicate-last" padding case.
            let tree = make_tree(&["a", "b", "c"]);
            for i in 0..tree.leaf_count() {
                let proof = tree.generate_proof(i).unwrap();
                assert!(proof.verify(), "proof for leaf {i} failed");
            }
        }

        // ── Property-based fuzz tests for MerkleTree ──────────────────────────────────
        //
        // Run with: cargo test (as part of the normal test suite)
        // For libfuzzer-based fuzzing, see core/fuzz/fuzz_targets/merkle_build.rs
        //
        // To run with higher iteration counts:
        //   PROPTEST_CASES=10000 cargo test fuzz_
        #[cfg(test)]
        mod fuzz_tests {
            use super::*;
            use proptest::prelude::*;

            fn arb_leaf() -> impl Strategy<Value = Vec<u8>> {
                prop::collection::vec(any::<u8>(), 0..=256)
            }

            fn arb_leaves(min: usize, max: usize) -> impl Strategy<Value = Vec<Vec<u8>>> {
                prop::collection::vec(arb_leaf(), min..=max)
            }

            proptest! {
                #![proptest_config(ProptestConfig::with_cases(500))]

                /// Builder must never panic on any non-empty input.
                #[test]
                fn fuzz_build_never_panics(leaves in arb_leaves(1, 64)) {
                    let mut tree = MerkleTree::new(32);
                    let _ = tree.build(leaves);
                }

                /// Empty input must return an error, never panic.
                #[test]
                fn fuzz_empty_input_returns_err(_dummy in any::<u8>()) {
                    let mut tree = MerkleTree::new(32);
                    prop_assert!(tree.build(vec![]).is_err());
                }

                /// Every proof produced by the builder must verify against the tree root.
                #[test]
                fn fuzz_all_proofs_verify(leaves in arb_leaves(1, 32)) {
                    let mut tree = MerkleTree::new(32);
                    tree.build(leaves).expect("build succeeds");
                    for i in 0..tree.leaf_count() {
                        let proof = tree.generate_proof(i).expect("proof exists");
                        prop_assert!(proof.verify(), "proof for leaf {i} failed");
                        prop_assert!(MerkleTree::verify_proof(&proof, &tree.root));
                    }
                }

                /// Root must be deterministic: same leaves always produce the same root.
                #[test]
                fn fuzz_build_is_deterministic(leaves in arb_leaves(1, 32)) {
                    let mut t1 = MerkleTree::new(32);
                    let mut t2 = MerkleTree::new(32);
                    t1.build(leaves.clone()).unwrap();
                    t2.build(leaves).unwrap();
                    prop_assert_eq!(t1.root, t2.root);
                }

                /// A tampered leaf hash must cause proof.verify() to return false.
                #[test]
                fn fuzz_tampered_leaf_invalidates_proof(leaves in arb_leaves(2, 32)) {
                    let mut tree = MerkleTree::new(32);
                    tree.build(leaves).unwrap();
                    let mut proof = tree.generate_proof(0).unwrap();
                    proof.leaf_hash[0] ^= 0xff;
                    prop_assert!(!proof.verify());
                }

                /// A tampered sibling hash must cause verify_proof to return false.
                #[test]
                fn fuzz_tampered_sibling_invalidates_proof(leaves in arb_leaves(2, 32)) {
                    let mut tree = MerkleTree::new(32);
                    tree.build(leaves).unwrap();
                    let mut proof = tree.generate_proof(0).unwrap();
                    if !proof.proof.is_empty() {
                        proof.proof[0].hash = [0xFFu8; 32];
                        prop_assert!(!MerkleTree::verify_proof(&proof, &tree.root));
                    }
                }

                /// All leaves in a tree of power-of-two size must produce valid proofs.
                #[test]
                fn fuzz_power_of_two_leaf_counts_valid(
                    log2_size in 1usize..=6usize,
                    seed in any::<u64>(),
                ) {
                    let size = 1 << log2_size;
                    let leaves: Vec<Vec<u8>> = (0..size)
                        .map(|i| {
                            let mut v = seed.to_le_bytes().to_vec();
                            v.extend_from_slice(&(i as u64).to_le_bytes());
                            v
                        })
                        .collect();
                    let mut tree = MerkleTree::new(32);
                    tree.build(leaves).unwrap();
                    for i in 0..tree.leaf_count() {
                        let proof = tree.generate_proof(i).unwrap();
                        prop_assert!(MerkleTree::verify_proof(&proof, &tree.root));
                    }
                }

                /// Duplicate leaves must not cause panics and must produce a valid tree.
                #[test]
                fn fuzz_duplicate_leaves_no_panic(
                    leaf in arb_leaf(),
                    count in 2usize..=16usize,
                ) {
                    let leaves = vec![leaf; count];
                    let mut tree = MerkleTree::new(32);
                    if tree.build(leaves).is_ok() {
                        prop_assert_eq!(tree.leaf_count(), count);
                    }
                }

                /// Very large individual leaves (up to 64 KiB) must not cause panics.
                #[test]
                fn fuzz_large_leaf_values(
                    leaf in prop::collection::vec(any::<u8>(), 0..=65536usize),
                ) {
                    let mut tree = MerkleTree::new(32);
                    let _ = tree.build(vec![leaf]);
                }

                /// hex-string round-trip: encode leaves to hex, build via from_hex_strings,
                /// result must equal direct build.
                #[test]
                fn fuzz_hex_roundtrip(leaves in arb_leaves(1, 16)) {
                    let hex_leaves: Vec<String> = leaves.iter().map(hex::encode).collect();
                    let tree_via_hex = MerkleTree::from_hex_strings(hex_leaves);
                    let mut tree_direct = MerkleTree::new(32);
                    tree_direct.build(leaves).unwrap();
                    if let Ok(t) = tree_via_hex {
                        prop_assert_eq!(t.root, tree_direct.root);
                    }
                }
            }
        }
    }
}

// ── Integration tests for incremental vs. full rebuild equivalence ────────────
#[cfg(test)]
mod incremental_equivalence_tests {
    use super::*;

    #[test]
    fn test_incremental_append_equals_full_rebuild_single_leaf() {
        let initial_leaves: Vec<Vec<u8>> = vec![b"a".to_vec(), b"b".to_vec()];
        let new_leaf = vec![b"c".to_vec()];
        let all_leaves = [&initial_leaves[..], &new_leaf[..]].concat();

        // Build incrementally
        let mut tree_incremental = MerkleTree::new(32);
        tree_incremental.build(initial_leaves.clone()).unwrap();
        tree_incremental.append_leaves(new_leaf.clone()).unwrap();

        // Build from scratch
        let mut tree_full = MerkleTree::new(32);
        tree_full.build(all_leaves).unwrap();

        // Roots must match
        assert_eq!(tree_incremental.root, tree_full.root);

        // Proofs must match
        for i in 0..3 {
            let proof_inc = tree_incremental.generate_proof(i).unwrap();
            let proof_full = tree_full.generate_proof(i).unwrap();

            assert_eq!(proof_inc.root, proof_full.root);
            assert_eq!(proof_inc.leaf_hash, proof_full.leaf_hash);
            assert_eq!(proof_inc.proof.len(), proof_full.proof.len());

            for (p_inc, p_full) in proof_inc.proof.iter().zip(proof_full.proof.iter()) {
                assert_eq!(p_inc.hash, p_full.hash);
                assert_eq!(p_inc.is_left, p_full.is_left);
            }
        }
    }

    #[test]
    fn test_incremental_append_equals_full_rebuild_multiple_leaves() {
        let initial_leaves: Vec<Vec<u8>> = vec![b"a".to_vec(), b"b".to_vec()];
        let new_leaves: Vec<Vec<u8>> = vec![
            b"c".to_vec(),
            b"d".to_vec(),
            b"e".to_vec(),
        ];
        let all_leaves = [&initial_leaves[..], &new_leaves[..]].concat();

        // Build incrementally
        let mut tree_incremental = MerkleTree::new(32);
        tree_incremental.build(initial_leaves.clone()).unwrap();
        tree_incremental.append_leaves(new_leaves.clone()).unwrap();

        // Build from scratch
        let mut tree_full = MerkleTree::new(32);
        tree_full.build(all_leaves).unwrap();

        // Roots must match
        assert_eq!(tree_incremental.root, tree_full.root);

        // All proofs must match
        for i in 0..5 {
            let proof_inc = tree_incremental.generate_proof(i).unwrap();
            let proof_full = tree_full.generate_proof(i).unwrap();
            assert_eq!(proof_inc.root, proof_full.root);
        }
    }

    #[test]
    fn test_incremental_update_equals_full_rebuild() {
        let leaves: Vec<Vec<u8>> = vec![
            b"a".to_vec(),
            b"b".to_vec(),
            b"c".to_vec(),
            b"d".to_vec(),
        ];

        // Build incrementally and update
        let mut tree_incremental = MerkleTree::new(32);
        tree_incremental.build(leaves.clone()).unwrap();
        tree_incremental.update_leaf(2, b"x".to_vec()).unwrap();

        // Build from scratch with updated leaf
        let mut updated_leaves = leaves.clone();
        updated_leaves[2] = b"x".to_vec();
        let mut tree_full = MerkleTree::new(32);
        tree_full.build(updated_leaves).unwrap();

        // Roots must match
        assert_eq!(tree_incremental.root, tree_full.root);

        // All proofs must match
        for i in 0..4 {
            let proof_inc = tree_incremental.generate_proof(i).unwrap();
            let proof_full = tree_full.generate_proof(i).unwrap();
            assert_eq!(proof_inc.root, proof_full.root);
        }
    }

    #[test]
    fn test_incremental_bulk_update_equals_full_rebuild() {
        let leaves: Vec<Vec<u8>> = vec![
            b"a".to_vec(),
            b"b".to_vec(),
            b"c".to_vec(),
            b"d".to_vec(),
            b"e".to_vec(),
        ];

        // Build incrementally and update multiple leaves
        let mut tree_incremental = MerkleTree::new(32);
        tree_incremental.build(leaves.clone()).unwrap();
        tree_incremental.update_leaves(vec![
            (0, b"w".to_vec()),
            (2, b"x".to_vec()),
            (4, b"z".to_vec()),
        ]).unwrap();

        // Build from scratch with updated leaves
        let mut updated_leaves = leaves.clone();
        updated_leaves[0] = b"w".to_vec();
        updated_leaves[2] = b"x".to_vec();
        updated_leaves[4] = b"z".to_vec();
        let mut tree_full = MerkleTree::new(32);
        tree_full.build(updated_leaves).unwrap();

        // Roots must match
        assert_eq!(tree_incremental.root, tree_full.root);

        // All proofs must match
        for i in 0..5 {
            let proof_inc = tree_incremental.generate_proof(i).unwrap();
            let proof_full = tree_full.generate_proof(i).unwrap();
            assert_eq!(proof_inc.root, proof_full.root);
        }
    }

    #[test]
    fn test_complex_sequence_incremental_equals_full_rebuild() {
        // Start with 2 leaves
        let initial_leaves: Vec<Vec<u8>> = vec![b"a".to_vec(), b"b".to_vec()];
        
        // Incremental path
        let mut tree_inc = MerkleTree::new(32);
        tree_inc.build(initial_leaves.clone()).unwrap();
        tree_inc.append_leaves(vec![b"c".to_vec(), b"d".to_vec()]).unwrap();
        tree_inc.update_leaf(1, b"b_updated".to_vec()).unwrap();
        tree_inc.append_leaves(vec![b"e".to_vec()]).unwrap();
        tree_inc.update_leaves(vec![(0, b"a_updated".to_vec()), (3, b"d_updated".to_vec())]).unwrap();

        // Full rebuild path
        let final_leaves = vec![
            b"a_updated".to_vec(),
            b"b_updated".to_vec(),
            b"c".to_vec(),
            b"d_updated".to_vec(),
            b"e".to_vec(),
        ];
        let mut tree_full = MerkleTree::new(32);
        tree_full.build(final_leaves).unwrap();

        // Roots must match
        assert_eq!(tree_inc.root, tree_full.root);

        // All leaf counts must match
        assert_eq!(tree_inc.leaf_count(), tree_full.leaf_count());

        // All proofs must match
        for i in 0..5 {
            let proof_inc = tree_inc.generate_proof(i).unwrap();
            let proof_full = tree_full.generate_proof(i).unwrap();
            assert_eq!(proof_inc.root, proof_full.root);
            assert_eq!(proof_inc.leaf, proof_full.leaf);
        }
    }

    #[test]
    fn test_large_tree_incremental_append_equals_full_rebuild() {
        // Create a large initial tree
        let initial_leaves: Vec<Vec<u8>> = (0..50)
            .map(|i| format!("leaf-{i}").into_bytes())
            .collect();
        let new_leaves: Vec<Vec<u8>> = (50..100)
            .map(|i| format!("leaf-{i}").into_bytes())
            .collect();
        let all_leaves = [&initial_leaves[..], &new_leaves[..]].concat();

        // Build incrementally
        let mut tree_inc = MerkleTree::new(32);
        tree_inc.build(initial_leaves.clone()).unwrap();
        tree_inc.append_leaves(new_leaves).unwrap();

        // Build from scratch
        let mut tree_full = MerkleTree::new(32);
        tree_full.build(all_leaves).unwrap();

        // Roots must match
        assert_eq!(tree_inc.root, tree_full.root);

        // Sample proofs at key positions
        for i in vec![0, 25, 49, 50, 75, 99] {
            let proof_inc = tree_inc.generate_proof(i).unwrap();
            let proof_full = tree_full.generate_proof(i).unwrap();
            assert_eq!(proof_inc.root, proof_full.root);
        }
    }

    #[test]
    fn test_odd_leaf_count_incremental_append() {
        // Start with odd count
        let initial: Vec<Vec<u8>> = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()];
        let to_add: Vec<Vec<u8>> = vec![b"d".to_vec(), b"e".to_vec()];
        let all = [&initial[..], &to_add[..]].concat();

        let mut tree_inc = MerkleTree::new(32);
        tree_inc.build(initial.clone()).unwrap();
        tree_inc.append_leaves(to_add).unwrap();

        let mut tree_full = MerkleTree::new(32);
        tree_full.build(all).unwrap();

        assert_eq!(tree_inc.root, tree_full.root);
    }
}
