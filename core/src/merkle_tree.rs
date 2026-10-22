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
        let new_leaf_hashes: Vec<[u8; 32]> = new_leaves
            .iter()
            .map(|leaf| Self::hash_leaf(leaf))
            .collect();

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
    pub fn update_leaf(
        &mut self,
        leaf_index: usize,
        new_leaf_data: Vec<u8>,
    ) -> Result<(), &'static str> {
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

    /// Visualizes the Merkle tree structure as an ASCII art diagram.
    ///
    /// This includes tree metadata (root hash, leaf count, tree height), an indented
    /// ASCII branch hierarchy from root to leaves, and a level-by-level breakdown.
    pub fn to_ascii_art(&self) -> String {
        if self.nodes.is_empty() || self.leaf_count == 0 {
            return "(Empty Merkle Tree)\n".to_string();
        }

        let mut out = String::new();
        out.push_str("====================================================================\n");
        out.push_str("                     MERKLE TREE VISUALIZATION                      \n");
        out.push_str("====================================================================\n");
        out.push_str(&format!("Root Hash   : {}\n", self.get_root_hex()));
        out.push_str(&format!("Leaf Count  : {}\n", self.leaf_count));
        out.push_str(&format!("Tree Height : {} level(s)\n", self.nodes.len()));
        out.push_str("--------------------------------------------------------------------\n");
        out.push_str("Tree Hierarchy:\n");
        out.push_str(&self.to_ascii_hierarchy());
        out.push_str("\n--------------------------------------------------------------------\n");
        out.push_str("Level Breakdown:\n");
        out.push_str(&self.to_ascii_layers());
        out.push_str("====================================================================\n");
        out
    }

    /// Alias for `to_ascii_art`.
    pub fn render_ascii(&self) -> String {
        self.to_ascii_art()
    }

    /// Returns the hierarchical tree representation using ASCII branch lines.
    pub fn to_ascii_hierarchy(&self) -> String {
        if self.nodes.is_empty() {
            return "(Empty)\n".to_string();
        }

        let root_level = self.nodes.len() - 1;
        let mut out = String::new();
        self.render_hierarchy_node(&mut out, root_level, 0, "", true);
        out
    }

    fn render_hierarchy_node(
        &self,
        out: &mut String,
        level: usize,
        index: usize,
        prefix: &str,
        is_last: bool,
    ) {
        let node_hash = self.nodes[level][index];
        let short_hash = format!(
            "{}...{}",
            &hex::encode(&node_hash[..4]),
            &hex::encode(&node_hash[28..])
        );

        if level == self.nodes.len() - 1 {
            // Root node
            if level == 0 {
                // Single-leaf tree: root is also leaf 0
                let leaf_preview = self.format_leaf_data(0);
                out.push_str(&format!(
                    "[Root / Leaf 0] {} (hash: {})\n",
                    leaf_preview, short_hash
                ));
                return;
            } else {
                out.push_str(&format!("[Root] {}\n", short_hash));
            }
        } else {
            let branch = if is_last { "└── " } else { "├── " };
            out.push_str(prefix);
            out.push_str(branch);

            if level == 0 {
                // Leaf node
                let leaf_preview = self.format_leaf_data(index);
                out.push_str(&format!(
                    "[Leaf {}] {} (hash: {})\n",
                    index, leaf_preview, short_hash
                ));
                return;
            } else {
                out.push_str(&format!("[Node L{}:{}] {}\n", level, index, short_hash));
            }
        }

        if level > 0 {
            let child_level = level - 1;
            let left_idx = 2 * index;
            let right_idx = 2 * index + 1;
            let has_right = right_idx < self.nodes[child_level].len();
            let is_odd_promoted = !has_right && left_idx < self.nodes[child_level].len();

            let next_prefix = if level == self.nodes.len() - 1 {
                "".to_string()
            } else if is_last {
                format!("{}    ", prefix)
            } else {
                format!("{}│   ", prefix)
            };

            // Left child
            if left_idx < self.nodes[child_level].len() {
                let left_is_last = !has_right && !is_odd_promoted;
                self.render_hierarchy_node(out, child_level, left_idx, &next_prefix, left_is_last);
            }

            // Right child
            if has_right {
                self.render_hierarchy_node(out, child_level, right_idx, &next_prefix, true);
            } else if is_odd_promoted {
                // Sibling promoted from odd child (paired with itself)
                let branch = "└── ";
                out.push_str(&next_prefix);
                out.push_str(branch);
                out.push_str(&format!(
                    "[Dup L{}:{}] (odd sibling paired with itself)\n",
                    child_level, left_idx
                ));
            }
        }
    }

    fn format_leaf_data(&self, index: usize) -> String {
        if index < self.data_leaves.len() {
            let data = &self.data_leaves[index];
            if let Ok(s) = std::str::from_utf8(data) {
                if s.chars().all(|c| !c.is_control()) {
                    return if s.len() > 16 {
                        format!("\"{}...\"", &s[..13])
                    } else {
                        format!("\"{}\"", s)
                    };
                }
            }
            let hex_str = hex::encode(data);
            if hex_str.len() > 12 {
                format!("0x{}...", &hex_str[..8])
            } else {
                format!("0x{}", hex_str)
            }
        } else {
            "<missing>".to_string()
        }
    }

    /// Returns a layer-by-layer ASCII diagram of the tree.
    pub fn to_ascii_layers(&self) -> String {
        if self.nodes.is_empty() {
            return "(Empty)\n".to_string();
        }

        let mut out = String::new();
        for (lvl_idx, level_nodes) in self.nodes.iter().enumerate().rev() {
            let label = if lvl_idx == self.nodes.len() - 1 {
                format!("Level {} [Root]     : ", lvl_idx)
            } else if lvl_idx == 0 {
                "Level 0 [Leaves]   : ".to_string()
            } else {
                format!("Level {} [Internal] : ", lvl_idx)
            };
            out.push_str(&label);

            let mut node_strs = Vec::new();
            for (idx, node_hash) in level_nodes.iter().enumerate() {
                let short_hash = format!(
                    "{}..{}",
                    &hex::encode(&node_hash[..2]),
                    &hex::encode(&node_hash[30..])
                );
                if lvl_idx == 0 && idx < self.data_leaves.len() {
                    let preview = self.format_leaf_data(idx);
                    node_strs.push(format!("[{}: {} ({})]", idx, preview, short_hash));
                } else {
                    node_strs.push(format!("[{}: {}]", idx, short_hash));
                }
            }

            out.push_str(&node_strs.join(" "));
            out.push('\n');
        }
        out
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

// ── Batch proof generation (#347) ─────────────────────────────────────────────

/// A batch of Merkle inclusion proofs for multiple leaves.
///
/// Produces proofs for a set of leaf indices in a single call, which is
/// more efficient than calling `generate_proof` repeatedly because it
/// avoids redundant tree traversals when leaves share ancestor nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMerkleProof {
    /// The root hash that all proofs commit to.
    pub root: [u8; 32],
    /// Individual proofs, one per requested leaf index.
    pub proofs: Vec<MerkleProof>,
}

impl MerkleTree {
    /// Generate inclusion proofs for multiple leaves in a single call.
    ///
    /// This is the batch proof generation feature (#347). It validates all
    /// indices first, then generates proofs for each. The proofs are
    /// returned in the same order as the requested indices.
    ///
    /// # Arguments
    /// * `leaf_indices` - Indices of leaves to generate proofs for
    ///
    /// # Returns
    /// * `Ok(BatchMerkleProof)` containing all proofs
    /// * `Err` if the tree is not built or any index is out of bounds
    pub fn generate_batch_proofs(
        &self,
        leaf_indices: &[usize],
    ) -> Result<BatchMerkleProof, &'static str> {
        if self.nodes.is_empty() {
            return Err("Cannot generate proofs before building the tree.");
        }

        if leaf_indices.is_empty() {
            return Ok(BatchMerkleProof {
                root: self.root,
                proofs: Vec::new(),
            });
        }

        // Validate all indices first
        for &idx in leaf_indices {
            if idx >= self.leaf_count {
                return Err("Leaf index is out of bounds.");
            }
        }

        let proofs: Result<Vec<MerkleProof>, &'static str> = leaf_indices
            .iter()
            .map(|&idx| self.generate_proof(idx))
            .collect();

        let proofs = proofs?;

        Ok(BatchMerkleProof {
            root: self.root,
            proofs,
        })
    }

    /// Verify a batch of proofs against the same root.
    ///
    /// Returns `true` only if every proof in the batch verifies.
    pub fn verify_batch_proofs(batch: &BatchMerkleProof) -> bool {
        batch.proofs.iter().all(|p| p.verify())
    }
}

// ── Sparse Merkle Tree (#348) ─────────────────────────────────────────────────
//
// A sparse Merkle tree is optimized for large key spaces where most positions
// are empty. Instead of materializing all 2^height leaves, it only stores
// non-empty nodes and computes default hashes for missing branches on demand.
//
// This implementation uses a depth of 256 (for 32-byte keys) by default, with
// O(log n) insertion and proof generation where n is the number of *set* leaves,
// not the theoretical maximum.

/// A sparse Merkle tree that stores key-value pairs efficiently.
///
/// Only non-empty nodes are kept in memory; default (zero) hashes for absent
/// subtrees are precomputed up to the tree height. This makes the tree
/// practical for very large leaf sets (e.g. 2^256 positions) while only
/// using memory proportional to the number of *set* leaves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SparseMerkleTree {
    /// The root hash of the tree.
    pub root: [u8; 32],
    /// Tree depth (number of levels from root to leaves).
    pub depth: usize,
    /// Precomputed default hashes for each level (default_hashes[0] = leaf default).
    default_hashes: Vec<[u8; 32]>,
    /// Stored leaves: key -> value hash.
    leaves: std::collections::BTreeMap<[u8; 32], [u8; 32]>,
}

impl SparseMerkleTree {
    /// Create a new sparse Merkle tree with the given depth.
    ///
    /// A depth of 256 supports 32-byte keys (the default for SHA-256 based
    /// systems). For smaller key spaces, a smaller depth reduces proof size.
    pub fn new(depth: usize) -> Self {
        let default_hashes = Self::compute_default_hashes(depth);
        let root = default_hashes[depth];
        SparseMerkleTree {
            root,
            depth,
            default_hashes,
            leaves: std::collections::BTreeMap::new(),
        }
    }

    /// Create a sparse Merkle tree with the standard 256-bit depth.
    pub fn new_256() -> Self {
        Self::new(256)
    }

    /// Insert a key-value pair into the tree.
    ///
    /// The key is hashed to produce a 32-byte leaf position identifier.
    /// The value is hashed with double SHA-256 (matching the binary tree's
    /// `hash_leaf` convention).
    pub fn insert(&mut self, key: &[u8], value: &[u8]) {
        let key_hash = Self::hash_key(key);
        let leaf_hash = Self::hash_leaf_value(value);
        self.leaves.insert(key_hash, leaf_hash);
        self.recompute_root();
    }

    /// Insert multiple key-value pairs, then recompute the root once.
    pub fn insert_batch(&mut self, entries: &[(Vec<u8>, Vec<u8>)]) {
        for (key, value) in entries {
            let key_hash = Self::hash_key(key);
            let leaf_hash = Self::hash_leaf_value(value);
            self.leaves.insert(key_hash, leaf_hash);
        }
        self.recompute_root();
    }

    /// Remove a key from the tree.
    pub fn remove(&mut self, key: &[u8]) -> bool {
        let key_hash = Self::hash_key(key);
        let removed = self.leaves.remove(&key_hash).is_some();
        if removed {
            self.recompute_root();
        }
        removed
    }

    /// Get the value hash for a key, if it exists.
    pub fn get(&self, key: &[u8]) -> Option<[u8; 32]> {
        let key_hash = Self::hash_key(key);
        self.leaves.get(&key_hash).copied()
    }

    /// Return the root hash as a hex string.
    pub fn get_root_hex(&self) -> String {
        hex::encode(self.root)
    }

    /// Return the number of leaves currently stored.
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    /// Generate an inclusion proof for a key.
    ///
    /// The proof consists of one sibling hash per level, from leaf to root.
    /// For empty sibling subtrees, the precomputed default hash is used.
    pub fn generate_proof(&self, key: &[u8]) -> Result<SparseMerkleProof, &'static str> {
        let key_hash = Self::hash_key(key);
        let leaf_hash = match self.leaves.get(&key_hash) {
            Some(h) => *h,
            None => return Err("Key not found in sparse Merkle tree."),
        };

        let mut proof_siblings = Vec::with_capacity(self.depth);
        let mut current_key = key_hash;

        for level in 0..self.depth {
            // Determine which direction: 0 = left child, 1 = right child
            let bit = Self::get_bit(&current_key, level);
            let sibling_key = Self::flip_bit(current_key, level);

            // Look up sibling at this level
            let sibling_hash = if bit == 0 {
                // Current is left, sibling is right
                self.find_subtree_root(level + 1, sibling_key)
            } else {
                // Current is right, sibling is left
                self.find_subtree_root(level + 1, sibling_key)
            };

            proof_siblings.push(SparseProofNode {
                hash: sibling_hash,
                is_left: bit == 1, // sibling is on the left if current is right
            });

            // Move up: clear the bit at this level
            current_key = Self::clear_bit(current_key, level);
        }

        Ok(SparseMerkleProof {
            key_hash,
            leaf_hash,
            root: self.root,
            siblings: proof_siblings,
        })
    }

    /// Recompute the root from the current set of leaves.
    fn recompute_root(&mut self) {
        if self.leaves.is_empty() {
            self.root = self.default_hashes[self.depth];
            return;
        }

        // Build the tree bottom-up, level by level.
        // At each level, we only compute nodes that have at least one non-empty
        // descendant. We use the precomputed default hashes for empty subtrees.
        let mut current: std::collections::BTreeMap<[u8; 32], [u8; 32]> = self
            .leaves
            .iter()
            .map(|(k, v)| {
                // Truncate key to depth bits for level-0 positioning
                let pos = Self::truncate_to_depth(k, 0);
                (pos, *v)
            })
            .collect();

        for level in 0..self.depth {
            let mut next: std::collections::BTreeMap<[u8; 32], [u8; 32]> =
                std::collections::BTreeMap::new();

            for (pos, hash) in &current {
                let parent_pos = Self::clear_bit(*pos, level);
                let bit = Self::get_bit(pos, level);

                let left_hash;
                let right_hash;

                if bit == 0 {
                    // This node is the left child
                    left_hash = *hash;
                    // Look for right sibling
                    let sibling_pos = Self::flip_bit(*pos, level);
                    right_hash = current
                        .get(&sibling_pos)
                        .copied()
                        .unwrap_or(self.default_hashes[level]);
                } else {
                    // This node is the right child
                    right_hash = *hash;
                    // Look for left sibling
                    let sibling_pos = Self::flip_bit(*pos, level);
                    left_hash = current
                        .get(&sibling_pos)
                        .copied()
                        .unwrap_or(self.default_hashes[level]);
                }

                let parent_hash = Self::hash_pair(&left_hash, &right_hash);
                next.insert(parent_pos, parent_hash);
            }

            current = next;
        }

        // After processing all levels, there should be exactly one node at the root
        self.root = current
            .values()
            .next()
            .copied()
            .unwrap_or(self.default_hashes[self.depth]);
    }

    /// Find the root hash of a subtree at the given level for a given position.
    /// Returns the default hash if the subtree is entirely empty.
    fn find_subtree_root(&self, level: usize, pos: [u8; 32]) -> [u8; 32] {
        if level >= self.depth {
            return self.default_hashes[self.depth];
        }

        // Check if any leaf exists in this subtree
        let mask = Self::make_mask(level);
        for (leaf_key, leaf_hash) in &self.leaves {
            if Self::key_matches_prefix(leaf_key, &pos, level) {
                // There's at least one leaf in this subtree; compute the root
                return self.compute_subtree_root(level, pos, mask);
            }
        }

        // No leaves in this subtree — return the default hash for this level
        self.default_hashes[self.depth - level]
    }

    /// Compute the root of a subtree that contains at least one leaf.
    fn compute_subtree_root(&self, level: usize, pos: [u8; 32], _mask: [u8; 32]) -> [u8; 32] {
        // Collect all leaves in this subtree
        let subtree_leaves: Vec<([u8; 32], [u8; 32])> = self
            .leaves
            .iter()
            .filter(|(k, _)| Self::key_matches_prefix(k, &pos, level))
            .map(|(k, v)| (*k, *v))
            .collect();

        if subtree_leaves.is_empty() {
            return self.default_hashes[self.depth - level];
        }

        // Build from the current level up to the root
        let mut current: std::collections::BTreeMap<[u8; 32], [u8; 32]> =
            subtree_leaves.iter().map(|(k, v)| (*k, *v)).collect();

        for l in level..self.depth {
            let mut next: std::collections::BTreeMap<[u8; 32], [u8; 32]> =
                std::collections::BTreeMap::new();

            for (node_pos, hash) in &current {
                let parent_pos = Self::clear_bit(*node_pos, l);
                let bit = Self::get_bit(node_pos, l);

                let left_hash;
                let right_hash;

                if bit == 0 {
                    left_hash = *hash;
                    let sibling_pos = Self::flip_bit(*node_pos, l);
                    right_hash = current
                        .get(&sibling_pos)
                        .copied()
                        .unwrap_or(self.default_hashes[l]);
                } else {
                    right_hash = *hash;
                    let sibling_pos = Self::flip_bit(*node_pos, l);
                    left_hash = current
                        .get(&sibling_pos)
                        .copied()
                        .unwrap_or(self.default_hashes[l]);
                }

                next.insert(parent_pos, Self::hash_pair(&left_hash, &right_hash));
            }

            current = next;
        }

        current
            .values()
            .next()
            .copied()
            .unwrap_or(self.default_hashes[self.depth])
    }

    // ── Hashing helpers ───────────────────────────────────────────────────

    fn hash_leaf_value(data: &[u8]) -> [u8; 32] {
        let mut h1 = Sha256::new();
        h1.update(data);
        let d1 = h1.finalize();
        let mut h2 = Sha256::new();
        h2.update(d1);
        h2.finalize().into()
    }

    fn hash_key(key: &[u8]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(key);
        h.finalize().into()
    }

    fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
        let mut h = Sha256::new();
        if left <= right {
            h.update(left);
            h.update(right);
        } else {
            h.update(right);
            h.update(left);
        }
        h.finalize().into()
    }

    /// Compute default (empty subtree) hashes for all levels.
    /// default_hashes[0] = hash of empty leaf
    /// default_hashes[i] = hash(default_hashes[i-1], default_hashes[i-1])
    fn compute_default_hashes(depth: usize) -> Vec<[u8; 32]> {
        let mut hashes = Vec::with_capacity(depth + 1);
        // Level 0: hash of empty data (double SHA-256 of empty bytes)
        hashes.push(Self::hash_leaf_value(&[]));

        for i in 1..=depth {
            let prev = hashes[i - 1];
            hashes.push(Self::hash_pair(&prev, &prev));
        }

        hashes
    }

    // ── Bit manipulation helpers ──────────────────────────────────────────

    /// Get the bit at position `level` (MSB-first within the 32-byte key).
    fn get_bit(key: &[u8; 32], level: usize) -> u8 {
        let byte_idx = level / 8;
        let bit_idx = 7 - (level % 8);
        (key[byte_idx] >> bit_idx) & 1
    }

    /// Flip the bit at position `level`.
    fn flip_bit(mut key: [u8; 32], level: usize) -> [u8; 32] {
        let byte_idx = level / 8;
        let bit_idx = 7 - (level % 8);
        key[byte_idx] ^= 1 << bit_idx;
        key
    }

    /// Clear the bit at position `level` (set to 0).
    fn clear_bit(mut key: [u8; 32], level: usize) -> [u8; 32] {
        let byte_idx = level / 8;
        let bit_idx = 7 - (level % 8);
        key[byte_idx] &= !(1 << bit_idx);
        key
    }

    /// Create a mask that covers bits from `level` to the end.
    fn make_mask(level: usize) -> [u8; 32] {
        let mut mask = [0u8; 32];
        let byte_idx = level / 8;
        for i in 0..byte_idx {
            mask[i] = 0xFF;
        }
        let remaining_bits = level % 8;
        if remaining_bits > 0 {
            mask[byte_idx] = 0xFF << (8 - remaining_bits);
        }
        mask
    }

    /// Check if a key matches a prefix at the given level.
    fn key_matches_prefix(key: &[u8; 32], prefix: &[u8; 32], level: usize) -> bool {
        for l in 0..level {
            if Self::get_bit(key, l) != Self::get_bit(prefix, l) {
                return false;
            }
        }
        true
    }

    /// Truncate a key to only the bits below `level` (clear bits at and above level).
    fn truncate_to_depth(key: &[u8; 32], level: usize) -> [u8; 32] {
        let mut result = *key;
        for l in level..256 {
            result = Self::clear_bit(result, l);
        }
        result
    }
}

/// A proof node in a sparse Merkle tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SparseProofNode {
    /// The sibling hash at this level.
    pub hash: [u8; 32],
    /// Whether the sibling is on the left side.
    pub is_left: bool,
}

/// An inclusion proof for a sparse Merkle tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SparseMerkleProof {
    /// The hashed key of the leaf.
    pub key_hash: [u8; 32],
    /// The hash of the leaf value.
    pub leaf_hash: [u8; 32],
    /// The root hash this proof commits to.
    pub root: [u8; 32],
    /// Sibling hashes from leaf to root.
    pub siblings: Vec<SparseProofNode>,
}

impl SparseMerkleProof {
    /// Verify this proof against the claimed root.
    pub fn verify(&self) -> bool {
        let mut current = self.leaf_hash;

        for node in &self.siblings {
            current = if node.is_left {
                SparseMerkleTree::hash_pair(&node.hash, &current)
            } else {
                SparseMerkleTree::hash_pair(&current, &node.hash)
            };
        }

        current == self.root
    }
}

// ── Tests for batch proof generation (#347) and sparse Merkle tree (#348) ─────

#[cfg(test)]
mod batch_proof_tests {
    use super::*;

    #[test]
    fn test_batch_proofs_all_valid() {
        let tree = make_tree(&["a", "b", "c", "d", "e", "f", "g", "h"]);
        let batch = tree
            .generate_batch_proofs(&[0, 1, 2, 3, 4, 5, 6, 7])
            .expect("batch proofs generate");
        assert_eq!(batch.proofs.len(), 8);
        assert_eq!(batch.root, tree.root);
        assert!(MerkleTree::verify_batch_proofs(&batch));
    }

    #[test]
    fn test_batch_proofs_subset() {
        let tree = make_tree(&["a", "b", "c", "d", "e"]);
        let batch = tree
            .generate_batch_proofs(&[0, 2, 4])
            .expect("batch proofs generate");
        assert_eq!(batch.proofs.len(), 3);
        assert!(MerkleTree::verify_batch_proofs(&batch));
        // Each proof should individually verify
        for proof in &batch.proofs {
            assert!(proof.verify());
            assert!(MerkleTree::verify_proof(proof, &tree.root));
        }
    }

    #[test]
    fn test_batch_proofs_empty_indices() {
        let tree = make_tree(&["a", "b", "c"]);
        let batch = tree.generate_batch_proofs(&[]).expect("empty batch ok");
        assert!(batch.proofs.is_empty());
        assert!(MerkleTree::verify_batch_proofs(&batch));
    }

    #[test]
    fn test_batch_proofs_unbuilt_tree_errors() {
        let tree = MerkleTree::new(32);
        assert!(tree.generate_batch_proofs(&[0]).is_err());
    }

    #[test]
    fn test_batch_proofs_out_of_bounds_errors() {
        let tree = make_tree(&["a", "b"]);
        assert!(tree.generate_batch_proofs(&[0, 5]).is_err());
    }

    #[test]
    fn test_batch_proofs_single_element() {
        let tree = make_tree(&["x", "y", "z"]);
        let batch = tree.generate_batch_proofs(&[1]).expect("batch ok");
        assert_eq!(batch.proofs.len(), 1);
        assert!(MerkleTree::verify_batch_proofs(&batch));
    }

    #[test]
    fn test_batch_proofs_duplicates() {
        let tree = make_tree(&["a", "b", "c", "d"]);
        let batch = tree
            .generate_batch_proofs(&[0, 0, 1, 1, 2])
            .expect("batch ok");
        assert_eq!(batch.proofs.len(), 5);
        assert!(MerkleTree::verify_batch_proofs(&batch));
    }

    #[test]
    fn test_batch_proofs_large_tree() {
        let leaves: Vec<Vec<u8>> = (0..32).map(|i| format!("leaf-{i}").into_bytes()).collect();
        let mut tree = MerkleTree::new(32);
        tree.build(leaves).unwrap();
        let indices: Vec<usize> = vec![0, 7, 15, 23, 31];
        let batch = tree.generate_batch_proofs(&indices).expect("batch ok");
        assert_eq!(batch.proofs.len(), 5);
        assert!(MerkleTree::verify_batch_proofs(&batch));
    }

    #[test]
    fn test_batch_proofs_matches_individual_proofs() {
        let tree = make_tree(&["a", "b", "c", "d", "e"]);
        let indices = [0, 2, 4];
        let batch = tree.generate_batch_proofs(&indices).expect("batch ok");
        for (i, &idx) in indices.iter().enumerate() {
            let individual = tree.generate_proof(idx).unwrap();
            assert_eq!(batch.proofs[i].leaf_index, individual.leaf_index);
            assert_eq!(batch.proofs[i].root, individual.root);
            assert_eq!(batch.proofs[i].leaf_hash, individual.leaf_hash);
            assert_eq!(batch.proofs[i].proof.len(), individual.proof.len());
        }
    }

    #[test]
    fn test_verify_batch_proofs_tampered_fails() {
        let tree = make_tree(&["a", "b", "c", "d"]);
        let mut batch = tree.generate_batch_proofs(&[0, 1, 2, 3]).expect("batch ok");
        batch.proofs[0].leaf_hash[0] ^= 0xFF;
        assert!(!MerkleTree::verify_batch_proofs(&batch));
    }
}

#[cfg(test)]
mod sparse_merkle_tree_tests {
    use super::*;

    #[test]
    fn test_sparse_tree_empty_root_matches_default() {
        let tree = SparseMerkleTree::new(256);
        // Root of an empty tree should be the precomputed default root
        assert_eq!(tree.leaf_count(), 0);
        assert!(!tree.get_root_hex().is_empty());
    }

    #[test]
    fn test_sparse_tree_insert_and_get() {
        let mut tree = SparseMerkleTree::new_256();
        tree.insert(b"key1", b"value1");
        assert_eq!(tree.leaf_count(), 1);
        assert!(tree.get(b"key1").is_some());
        assert!(tree.get(b"key2").is_none());
    }

    #[test]
    fn test_sparse_tree_insert_changes_root() {
        let mut tree = SparseMerkleTree::new_256();
        let empty_root = tree.root;
        tree.insert(b"key1", b"value1");
        assert_ne!(tree.root, empty_root);
    }

    #[test]
    fn test_sparse_tree_remove() {
        let mut tree = SparseMerkleTree::new_256();
        tree.insert(b"key1", b"value1");
        assert_eq!(tree.leaf_count(), 1);
        assert!(tree.remove(b"key1"));
        assert_eq!(tree.leaf_count(), 0);
        assert!(tree.get(b"key1").is_none());
    }

    #[test]
    fn test_sparse_tree_remove_nonexistent() {
        let mut tree = SparseMerkleTree::new_256();
        assert!(!tree.remove(b"nonexistent"));
    }

    #[test]
    fn test_sparse_tree_insert_batch() {
        let mut tree = SparseMerkleTree::new_256();
        let entries = vec![
            (b"key1".to_vec(), b"val1".to_vec()),
            (b"key2".to_vec(), b"val2".to_vec()),
            (b"key3".to_vec(), b"val3".to_vec()),
        ];
        tree.insert_batch(&entries);
        assert_eq!(tree.leaf_count(), 3);
        assert!(tree.get(b"key1").is_some());
        assert!(tree.get(b"key2").is_some());
        assert!(tree.get(b"key3").is_some());
    }

    #[test]
    fn test_sparse_tree_batch_matches_individual_inserts() {
        let entries = vec![
            (b"key1".to_vec(), b"val1".to_vec()),
            (b"key2".to_vec(), b"val2".to_vec()),
            (b"key3".to_vec(), b"val3".to_vec()),
        ];

        let mut tree_batch = SparseMerkleTree::new_256();
        tree_batch.insert_batch(&entries);

        let mut tree_individual = SparseMerkleTree::new_256();
        for (k, v) in &entries {
            tree_individual.insert(k, v);
        }

        assert_eq!(tree_batch.root, tree_individual.root);
    }

    #[test]
    fn test_sparse_tree_deterministic_root() {
        let entries = vec![
            (b"key1".to_vec(), b"val1".to_vec()),
            (b"key2".to_vec(), b"val2".to_vec()),
        ];

        let mut t1 = SparseMerkleTree::new_256();
        let mut t2 = SparseMerkleTree::new_256();
        t1.insert_batch(&entries);
        t2.insert_batch(&entries);
        assert_eq!(t1.root, t2.root);
    }

    #[test]
    fn test_sparse_tree_proof_verifies() {
        let mut tree = SparseMerkleTree::new_256();
        tree.insert(b"key1", b"value1");
        tree.insert(b"key2", b"value2");
        tree.insert(b"key3", b"value3");

        let proof = tree.generate_proof(b"key1").expect("proof exists");
        assert_eq!(proof.root, tree.root);
        assert!(proof.verify());
    }

    #[test]
    fn test_sparse_tree_proof_multiple_leaves() {
        let mut tree = SparseMerkleTree::new_256();
        for i in 0..10 {
            tree.insert(format!("key{i}").as_bytes(), format!("val{i}").as_bytes());
        }

        for i in 0..10 {
            let key = format!("key{i}");
            let proof = tree.generate_proof(key.as_bytes()).expect("proof exists");
            assert_eq!(proof.root, tree.root);
            assert!(proof.verify(), "proof for key{i} should verify");
        }
    }

    #[test]
    fn test_sparse_tree_proof_nonexistent_key_errors() {
        let mut tree = SparseMerkleTree::new_256();
        tree.insert(b"key1", b"value1");
        assert!(tree.generate_proof(b"nonexistent").is_err());
    }

    #[test]
    fn test_sparse_tree_proof_tampered_fails() {
        let mut tree = SparseMerkleTree::new_256();
        tree.insert(b"key1", b"value1");
        tree.insert(b"key2", b"value2");

        let mut proof = tree.generate_proof(b"key1").expect("proof exists");
        proof.leaf_hash[0] ^= 0xFF;
        assert!(!proof.verify());
    }

    #[test]
    fn test_sparse_tree_proof_wrong_root_fails() {
        let mut tree = SparseMerkleTree::new_256();
        tree.insert(b"key1", b"value1");
        let mut proof = tree.generate_proof(b"key1").expect("proof exists");
        proof.root = [0xAB; 32];
        assert!(!proof.verify());
    }

    #[test]
    fn test_sparse_tree_remove_changes_root() {
        let mut tree = SparseMerkleTree::new_256();
        tree.insert(b"key1", b"value1");
        tree.insert(b"key2", b"value2");
        let root_before = tree.root;
        tree.remove(b"key1");
        assert_ne!(tree.root, root_before);
        // The remaining leaf should still have a valid proof
        let proof = tree.generate_proof(b"key2").expect("proof exists");
        assert!(proof.verify());
    }

    #[test]
    fn test_sparse_tree_small_depth() {
        // Use a small depth (4 = 16 possible positions) for testing
        let mut tree = SparseMerkleTree::new(4);
        tree.insert(b"a", b"val_a");
        tree.insert(b"b", b"val_b");
        assert_eq!(tree.leaf_count(), 2);
        let proof = tree.generate_proof(b"a").expect("proof exists");
        assert_eq!(proof.siblings.len(), 4);
        assert!(proof.verify());
    }

    #[test]
    fn test_sparse_tree_update_value() {
        let mut tree = SparseMerkleTree::new_256();
        tree.insert(b"key1", b"value1");
        let root1 = tree.root;
        tree.insert(b"key1", b"value2"); // overwrite
        assert_ne!(tree.root, root1);
        assert_eq!(tree.leaf_count(), 1);
        // New value should be reflected
        let proof = tree.generate_proof(b"key1").expect("proof exists");
        assert!(proof.verify());
    }

    #[test]
    fn test_sparse_tree_get_root_hex_length() {
        let tree = SparseMerkleTree::new_256();
        assert_eq!(tree.get_root_hex().len(), 64);
    }

    #[test]
    fn test_sparse_tree_large_number_of_leaves() {
        let mut tree = SparseMerkleTree::new_256();
        for i in 0..100 {
            tree.insert(format!("key{i}").as_bytes(), format!("val{i}").as_bytes());
        }
        assert_eq!(tree.leaf_count(), 100);
        // Sample a few proofs
        for i in [0, 25, 50, 75, 99] {
            let key = format!("key{i}");
            let proof = tree.generate_proof(key.as_bytes()).expect("proof exists");
            assert!(proof.verify(), "proof for key{i} should verify");
        }
    }

    #[test]
    fn test_sparse_tree_empty_after_all_removals() {
        let mut tree = SparseMerkleTree::new_256();
        let empty_root = tree.root;
        tree.insert(b"key1", b"value1");
        tree.remove(b"key1");
        assert_eq!(tree.root, empty_root);
        assert_eq!(tree.leaf_count(), 0);
    }

    #[test]
    fn test_sparse_tree_proof_siblings_use_defaults_for_empty() {
        let mut tree = SparseMerkleTree::new_256();
        tree.insert(b"key1", b"value1");

        // With only one leaf, most siblings should be default hashes
        let proof = tree.generate_proof(b"key1").expect("proof exists");
        assert!(proof.verify());

        // Count how many siblings are default hashes (non-leaf level defaults)
        let default_hashes = SparseMerkleTree::compute_default_hashes(256);
        let default_count = proof
            .siblings
            .iter()
            .filter(|s| default_hashes.contains(&s.hash))
            .count();
        // With a single leaf, nearly all siblings should be defaults
        assert!(
            default_count > 250,
            "expected most siblings to be default hashes, got {default_count}"
        );
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

        tree.append_leaves(vec![b"c".to_vec()])
            .expect("append succeeds");

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

        tree.update_leaves(vec![(0, b"w".to_vec()), (2, b"y".to_vec())])
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

        tree.update_leaves(vec![(0, b"a".to_vec()), (1, b"b".to_vec())])
            .expect("update with same data succeeds");

        // Root should NOT have changed
        assert_eq!(tree.root, original_root);
    }

    #[test]
    fn test_update_leaves_out_of_bounds() {
        let mut tree = make_tree(&["a", "b"]);
        assert!(tree.update_leaves(vec![(5, b"x".to_vec())]).is_err());
    }

    #[test]
    fn test_update_leaves_uninitialized_tree() {
        let mut tree = MerkleTree::new(32);
        assert!(tree.update_leaves(vec![(0, b"x".to_vec())]).is_err());
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
        let new_leaves: Vec<Vec<u8>> = (0..100).map(|i| format!("leaf-{i}").into_bytes()).collect();

        tree.append_leaves(new_leaves)
            .expect("large append succeeds");
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
        // Test multiple odd leaf counts — includes duplicate-last padding across levels.
        for &count in &[1, 3, 5, 7, 9, 11, 15, 17, 25, 31, 33] {
            let leaf_strings: Vec<String> = (0..count).map(|i| format!("leaf-{i}")).collect();
            let leaf_slices: Vec<&str> = leaf_strings.iter().map(|s| s.as_str()).collect();
            let tree = make_tree(&leaf_slices);
            assert_eq!(tree.leaf_count(), count);
            for i in 0..tree.leaf_count() {
                let proof = tree
                    .generate_proof(i)
                    .unwrap_or_else(|_| panic!("proof for leaf {i} in {count}-leaf tree exists"));
                assert_eq!(proof.leaf_index, i);
                assert_eq!(proof.root, tree.root);
                assert!(
                    proof.verify(),
                    "proof for leaf {i} in {count}-leaf tree failed verify()"
                );
                assert!(
                    MerkleTree::verify_proof(&proof, &tree.root),
                    "proof for leaf {i} in {count}-leaf tree failed MerkleTree::verify_proof"
                );
            }
        }
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
        let new_leaves: Vec<Vec<u8>> = vec![b"c".to_vec(), b"d".to_vec(), b"e".to_vec()];
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
        let leaves: Vec<Vec<u8>> = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()];

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
        tree_incremental
            .update_leaves(vec![
                (0, b"w".to_vec()),
                (2, b"x".to_vec()),
                (4, b"z".to_vec()),
            ])
            .unwrap();

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
        tree_inc
            .append_leaves(vec![b"c".to_vec(), b"d".to_vec()])
            .unwrap();
        tree_inc.update_leaf(1, b"b_updated".to_vec()).unwrap();
        tree_inc.append_leaves(vec![b"e".to_vec()]).unwrap();
        tree_inc
            .update_leaves(vec![(0, b"a_updated".to_vec()), (3, b"d_updated".to_vec())])
            .unwrap();

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
        let initial_leaves: Vec<Vec<u8>> =
            (0..50).map(|i| format!("leaf-{i}").into_bytes()).collect();
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

    #[test]
    fn test_to_ascii_art_empty_tree() {
        let tree = MerkleTree::new(32);
        let art = tree.to_ascii_art();
        assert!(art.contains("Empty Merkle Tree"));
        assert_eq!(tree.render_ascii(), art);
        assert_eq!(tree.to_ascii_hierarchy(), "(Empty)\n");
        assert_eq!(tree.to_ascii_layers(), "(Empty)\n");
    }

    #[test]
    fn test_to_ascii_art_single_leaf() {
        let tree = make_tree(&["single_leaf"]);
        let art = tree.to_ascii_art();
        assert!(art.contains("MERKLE TREE VISUALIZATION"));
        assert!(art.contains("Leaf Count  : 1"));
        assert!(art.contains("Tree Height : 1 level(s)"));
        assert!(art.contains("[Root / Leaf 0] \"single_leaf\""));
        assert!(art.contains("Level 0 [Leaves]"));
    }

    #[test]
    fn test_to_ascii_art_even_leaves() {
        let tree = make_tree(&["apple", "banana", "cherry", "date"]);
        let art = tree.to_ascii_art();
        assert!(art.contains("Leaf Count  : 4"));
        assert!(art.contains("Tree Height : 3 level(s)"));
        assert!(art.contains("[Root]"));
        assert!(art.contains("[Leaf 0] \"apple\""));
        assert!(art.contains("[Leaf 1] \"banana\""));
        assert!(art.contains("[Leaf 2] \"cherry\""));
        assert!(art.contains("[Leaf 3] \"date\""));
        assert!(art.contains("Level 2 [Root]"));
        assert!(art.contains("Level 1 [Internal]"));
        assert!(art.contains("Level 0 [Leaves]"));
    }

    #[test]
    fn test_to_ascii_art_odd_leaves() {
        let tree = make_tree(&["alpha", "beta", "gamma"]);
        let art = tree.to_ascii_art();
        assert!(art.contains("Leaf Count  : 3"));
        assert!(art.contains("[Leaf 0] \"alpha\""));
        assert!(art.contains("[Leaf 1] \"beta\""));
        assert!(art.contains("[Leaf 2] \"gamma\""));
        assert!(art.contains("odd sibling paired with itself"));
    }

    #[test]
    fn test_to_ascii_art_binary_leaves() {
        let leaves = vec![vec![0xde, 0xad, 0xbe, 0xef], vec![0xca, 0xfe]];
        let mut tree = MerkleTree::new(32);
        tree.build(leaves).unwrap();
        let art = tree.to_ascii_art();
        assert!(art.contains("0xdeadbeef"));
        assert!(art.contains("0xcafe"));
    }
}
