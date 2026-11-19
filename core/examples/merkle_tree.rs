#[path = "../src/merkle_tree.rs"]
mod merkle_tree;

fn main() {
    let leaves: Vec<Vec<u8>> = vec![
        b"leaf_0".to_vec(),
        b"leaf_1".to_vec(),
        b"leaf_2".to_vec(),
    ];
    let mut tree = merkle_tree::MerkleTree::new(32);
    tree.build(leaves).unwrap();
    println!("Root: {}", tree.get_root_hex());
    for i in 0..tree.leaf_count() {
        let proof = tree.generate_proof(i).unwrap();
        assert!(proof.verify());
        println!("Proof verified for leaf {i}");
    }
}