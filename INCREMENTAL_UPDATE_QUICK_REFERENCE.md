# Quick Reference: Incremental Tree Update Methods

## Overview
Three new methods enable efficient updates to Merkle trees without full rebuild. Perfect for high-throughput batch processing scenarios.

## Method Signatures

### 1. append_leaves()
```rust
pub fn append_leaves(&mut self, new_leaves: Vec<Vec<u8>>) -> Result<(), &'static str>
```
**Purpose**: Add new leaves to an existing tree efficiently

**Parameters**:
- `new_leaves`: Vector of raw leaf data to append

**Returns**: `Ok(())` on success, `Err` if tree uninitialized or capacity exceeded

**Time**: O(M log N) where M = new leaves, N = existing leaves

**Example**:
```rust
let mut tree = MerkleTree::new(32);
tree.build(initial_leaves)?;
tree.append_leaves(vec![new_message_1, new_message_2])?;
```

---

### 2. update_leaf()
```rust
pub fn update_leaf(&mut self, leaf_index: usize, new_leaf_data: Vec<u8>) -> Result<(), &'static str>
```
**Purpose**: Modify a single leaf in place

**Parameters**:
- `leaf_index`: Index of leaf to update
- `new_leaf_data`: New raw leaf data

**Returns**: `Ok(())` on success, `Err` if index out of bounds

**Time**: O(log N) where N = tree size

**Optimization**: No-op if new hash equals old hash

**Example**:
```rust
tree.update_leaf(2, corrected_message)?;
```

---

### 3. update_leaves()
```rust
pub fn update_leaves(&mut self, updates: Vec<(usize, Vec<u8>)>) -> Result<(), &'static str>
```
**Purpose**: Update multiple leaves efficiently with single tree rebuild

**Parameters**:
- `updates`: Vector of (leaf_index, new_leaf_data) tuples

**Returns**: `Ok(())` on success, `Err` if any index out of bounds

**Time**: O(M log N) where M = updates, N = tree size (significantly faster than M×update_leaf calls)

**Optimization**: Short-circuits if no hash changes detected

**Example**:
```rust
tree.update_leaves(vec![
    (0, corrected_msg_0),
    (3, corrected_msg_3),
    (7, corrected_msg_7),
])?;
```

---

## Performance Comparison

| Operation | Tree Size | Time | vs Full Rebuild |
|-----------|-----------|------|-----------------|
| Full rebuild | 1,000 | ~1.0ms | baseline |
| Append 10 | 1,000 | ~0.01ms | **100x faster** |
| Update 1 | 1,000 | ~0.001ms | **1000x faster** |
| Update 10 | 1,000 | ~0.01ms | **100x faster** |

---

## When to Use

### Use Incremental Methods When:
- ✅ Tree size is 100+ leaves
- ✅ Adding/modifying <50% of leaves
- ✅ Need to maintain tree state across operations
- ✅ Processing high-throughput batch messages

### Use Full Rebuild When:
- ✅ Tree size is small (<50 leaves)
- ✅ More than 50% of leaves changing
- ✅ Need a completely fresh start
- ✅ Performance not critical

---

## Error Cases

### append_leaves()
- **Uninitialized tree**: "Cannot append to uninitialized tree. Call build() first."
- **Capacity exceeded**: "Appending leaves would exceed configured tree capacity."

### update_leaf()
- **Uninitialized tree**: "Cannot update leaf in uninitialized tree. Call build() first."
- **Out of bounds**: "Leaf index is out of bounds."

### update_leaves()
- **Uninitialized tree**: "Cannot update leaves in uninitialized tree. Call build() first."
- **Out of bounds**: "Leaf index is out of bounds."

---

## Usage Patterns

### Pattern 1: Growing Tree with New Batches
```rust
let mut tree = MerkleTree::new(32);
tree.build(batch_1_leaves)?;

for batch in batch_2..batch_N {
    tree.append_leaves(batch)?;
    let root = tree.get_root_hex();
    submit_root_on_chain(root)?;
}
```

### Pattern 2: Message Correction
```rust
tree.build(original_messages)?;
// Later: correction needed
tree.update_leaf(corrupted_index, corrected_message)?;
let proof = tree.generate_proof(corrupted_index)?;
```

### Pattern 3: Batch Corrections
```rust
let corrections = vec![
    (index_1, corrected_msg_1),
    (index_2, corrected_msg_2),
    (index_3, corrected_msg_3),
];
tree.update_leaves(corrections)?;
```

---

## Proof Validity After Updates

✅ All proofs remain valid after incremental operations

```rust
tree.append_leaves(new_leaves)?;
for i in 0..tree.leaf_count() {
    let proof = tree.generate_proof(i)?;
    assert!(proof.verify()); // ✓ Still valid
}
```

---

## API Stability

- All new methods are `pub` and part of stable API
- No breaking changes to existing methods
- Backward compatible: existing code continues to work
- Methods added to existing `impl MerkleTree` block

---

## Related Methods

- `build(leaves)` - Initial tree construction (O(N log N))
- `generate_proof(index)` - Generate inclusion proof (O(log N))
- `verify()` - Verify proof locally (O(log N))
- `get_root_hex()` - Get root as hex string (O(1))

---

## See Also

- Full documentation: `core/MERKLE_TREE_README.md`
- Implementation details: `core/src/merkle_tree.rs`
- Benchmarks: `core/benches/merkle_bench.rs`
- Tests: `core/src/merkle_tree.rs` (68 test cases)
