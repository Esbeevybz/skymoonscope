#[path = "../src/merkle_tree.rs"]
mod merkle_tree;

use merkle_tree::MerkleTree;

#[test]
fn test_merkle_even() {
    let leaves: Vec<Vec<u8>> = vec![
        b"leaf_0".to_vec(),
        b"leaf_1".to_vec(),
        b"leaf_2".to_vec(),
        b"leaf_3".to_vec(),
    ];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).unwrap();
    assert_eq!(tree.leaf_count(), 4);
    for i in 0..4 {
        let proof = tree.generate_proof(i).unwrap();
        assert!(proof.verify());
        assert!(MerkleTree::verify_proof(&proof, &tree.root));
    }
}

#[test]
fn test_merkle_odd_leaves() {
    let leaves: Vec<Vec<u8>> = vec![
        b"leaf_0".to_vec(),
        b"leaf_1".to_vec(),
        b"leaf_2".to_vec(),
    ];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).unwrap();
    assert_eq!(tree.leaf_count(), 3);
    for i in 0..3 {
        let proof = tree.generate_proof(i).unwrap();
        assert!(proof.verify());
        assert!(MerkleTree::verify_proof(&proof, &tree.root));
    }
}

#[test]
fn test_empty() {
    let mut tree = MerkleTree::new(32);
    assert!(tree.build(vec![]).is_err());
}