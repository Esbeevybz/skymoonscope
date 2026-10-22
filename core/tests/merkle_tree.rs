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
    let leaves: Vec<Vec<u8>> = vec![b"leaf_0".to_vec(), b"leaf_1".to_vec(), b"leaf_2".to_vec()];
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

#[test]
fn test_merkle_ascii_art() {
    let leaves: Vec<Vec<u8>> = vec![
        b"alpha".to_vec(),
        b"beta".to_vec(),
        b"gamma".to_vec(),
        b"delta".to_vec(),
    ];
    let mut tree = MerkleTree::new(32);
    tree.build(leaves).unwrap();
    let ascii = tree.to_ascii_art();
    assert!(ascii.contains("MERKLE TREE VISUALIZATION"));
    assert!(ascii.contains("Leaf Count  : 4"));
    assert!(ascii.contains("Tree Height : 3 level(s)"));
    assert!(ascii.contains("[Leaf 0] \"alpha\""));
    assert!(ascii.contains("[Leaf 3] \"delta\""));
    assert_eq!(tree.render_ascii(), ascii);
}

#[test]
fn test_merkle_ascii_art_empty() {
    let tree = MerkleTree::new(32);
    let ascii = tree.to_ascii_art();
    assert_eq!(ascii, "(Empty Merkle Tree)\n");
}
