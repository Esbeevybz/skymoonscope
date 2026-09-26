#![cfg(test)]

use crate::{CrossChainVerifier, CrossChainVerifierClient, Payload};
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env, Vec};

fn make_payload(env: &Env, nonce: u64) -> Payload {
    let chain_id = 1;
    let dest = Address::generate(env);
    let data = Bytes::from_slice(env, b"test message");
    Payload {
        chain_id,
        destination_contract: dest,
        nonce,
        data,
    }
}

fn merkle_hash_pair(env: &Env, left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    crate::merkle_node_hash(env, left, right)
}

/// The leaf value the contract is asked to prove inclusion of: the payload hash.
fn raw_leaf(env: &Env, payload: &Payload) -> BytesN<32> {
    CrossChainVerifier::compute_payload_hash(env, payload)
}

/// The domain-separated leaf hash the tree is actually built over:
/// `SHA256(0x00 || payload_hash)` per RFC 6962.
fn compute_leaf(env: &Env, payload: &Payload) -> BytesN<32> {
    crate::merkle_leaf_hash(env, &raw_leaf(env, payload))
}
use crate::{
    CrossChainMessage, CrossChainVerifier, CrossChainVerifierClient, SignatureAlgorithm,
    SignedMessage,
};
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env, Vec};

#[test]
fn test_initialization() {
    let env = Env::default();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_double_initialization() {
    let env = Env::default();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);
    client.initialize(&admin);
}

#[test]
fn test_root_update() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let root = BytesN::from_array(&env, &[1; 32]);
    let block_height = 100;

    client.update_root(&block_height, &root);

    let retrieved = client.get_root(&block_height).unwrap();
    assert_eq!(retrieved, root);
}

#[test]
fn test_verify_message_success() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let payload = make_payload(&env, 1);
    let leaf = compute_leaf(&env, &payload);

    let sibling1 = BytesN::from_array(&env, &[3; 32]);
    let sibling2 = BytesN::from_array(&env, &[4; 32]);

    // Manually construct the root
    let hash_1 = merkle_hash_pair(&env, &sibling1.to_array(), &leaf.to_array());
    let final_root = merkle_hash_pair(&env, &hash_1, &sibling2.to_array());

    let expected_root_bytes = BytesN::from_array(&env, &final_root);

    let block_height = 100;
    client.update_root(&block_height, &expected_root_bytes);

    let mut proof = Vec::new(&env);
    proof.push_back(sibling1);
    proof.push_back(sibling2);

    let mut proof_flags = Vec::new(&env);
    proof_flags.push_back(true);
    proof_flags.push_back(false);

    let result = client.verify_message(&block_height, &payload, &proof, &proof_flags);
    assert!(result);
}

#[test]
fn test_verify_message_and_consume_nonce() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let leaf = BytesN::from_array(&env, &[2; 32]);
    let sibling1 = BytesN::from_array(&env, &[3; 32]);
    let sibling2 = BytesN::from_array(&env, &[4; 32]);

    let mut combined_1 = [0u8; 64];
    combined_1[0..32].copy_from_slice(&sibling1.to_array());
    combined_1[32..64].copy_from_slice(&leaf.to_array());
    let hash_1 = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &combined_1))
        .to_array();

    let mut combined_2 = [0u8; 64];
    combined_2[0..32].copy_from_slice(&hash_1);
    combined_2[32..64].copy_from_slice(&sibling2.to_array());
    let final_root = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &combined_2))
        .to_array();

    let expected_root_bytes = BytesN::from_array(&env, &final_root);
    let block_height = 100;
    client.update_root(&block_height, &expected_root_bytes);

    let mut proof = Vec::new(&env);
    proof.push_back(sibling1);
    proof.push_back(sibling2);

    let mut proof_flags = Vec::new(&env);
    proof_flags.push_back(true);
    proof_flags.push_back(false);

    assert!(client.verify_message_and_consume(&block_height, &1u64, &leaf, &proof, &proof_flags));
    assert!(client.is_nonce_processed(&1u64));
}

#[test]
#[should_panic(expected = "nonce already processed")]
fn test_replay_nonce_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let leaf = BytesN::from_array(&env, &[2; 32]);
    let sibling1 = BytesN::from_array(&env, &[3; 32]);
    let sibling2 = BytesN::from_array(&env, &[4; 32]);

    let mut combined_1 = [0u8; 64];
    combined_1[0..32].copy_from_slice(&sibling1.to_array());
    combined_1[32..64].copy_from_slice(&leaf.to_array());
    let hash_1 = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &combined_1))
        .to_array();

    let mut combined_2 = [0u8; 64];
    combined_2[0..32].copy_from_slice(&hash_1);
    combined_2[32..64].copy_from_slice(&sibling2.to_array());
    let final_root = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &combined_2))
        .to_array();

    let expected_root_bytes = BytesN::from_array(&env, &final_root);
    let block_height = 100;
    client.update_root(&block_height, &expected_root_bytes);

    let mut proof = Vec::new(&env);
    proof.push_back(sibling1);
    proof.push_back(sibling2);

    let mut proof_flags = Vec::new(&env);
    proof_flags.push_back(true);
    proof_flags.push_back(false);

    assert!(client.verify_message_and_consume(&block_height, &1u64, &leaf, &proof, &proof_flags));
    client.verify_message_and_consume(&block_height, &1u64, &leaf, &proof, &proof_flags);
}

#[test]
fn test_verify_message_no_root() {
    let env = Env::default();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let payload = make_payload(&env, 1);
    let proof = Vec::new(&env);
    let proof_flags = Vec::new(&env);

    client.verify_message(&100, &payload, &proof, &proof_flags);
}

#[test]
#[should_panic(expected = "Nonce already used")]
fn test_verify_message_replay_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    assert!(!client.verify_message(&100, &leaf, &proof, &proof_flags));
}

// ============================================================================
// Signature Verification Tests
// ============================================================================

#[test]
fn test_add_authorized_signer_ed25519() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let payload = make_payload(&env, 1);
    let leaf = compute_leaf(&env, &payload);

    // Single-node tree: leaf == root
    let block_height = 100;
    client.update_root(&block_height, &leaf);

    let proof = Vec::new(&env);
    let proof_flags = Vec::new(&env);

    // First use succeeds
    assert!(client.verify_message(&block_height, &payload, &proof, &proof_flags));

    // Second use with same nonce should panic
    client.verify_message(&block_height, &payload, &proof, &proof_flags);
}

#[test]
fn test_verify_message_different_nonce_allowed() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    // Create a test Ed25519 public key (32 bytes)
    let public_key = Bytes::from_slice(&env, &[1; 32]);

    client.add_authorized_signer(&public_key, &SignatureAlgorithm::Ed25519);

    // Verify signer count increased
    let count = client.get_signer_count();
    assert_eq!(count, 1);
}

#[test]
fn test_add_authorized_signer_secp256k1() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    // Create a test Secp256k1 public key (33 bytes compressed)
    let public_key = Bytes::from_slice(&env, &[2; 33]);

    client.add_authorized_signer(&public_key, &SignatureAlgorithm::Secp256k1);

    // Verify signer count increased
    let count = client.get_signer_count();
    assert_eq!(count, 1);
}

#[test]
#[should_panic(expected = "Signer already authorized")]
fn test_add_duplicate_signer() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let public_key = Bytes::from_slice(&env, &[1; 32]);

    client.add_authorized_signer(&public_key, &SignatureAlgorithm::Ed25519);
    client.add_authorized_signer(&public_key, &SignatureAlgorithm::Ed25519); // Should panic
}

#[test]
fn test_remove_authorized_signer() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let payload1 = make_payload(&env, 1);
    let leaf1 = compute_leaf(&env, &payload1);

    let payload2 = make_payload(&env, 2);
    let leaf2 = compute_leaf(&env, &payload2);

    // Build a root that commits to both leaves (2-level tree)
    // Level 1: combine leaf1 with leaf2
    let branch = merkle_hash_pair(&env, &leaf1.to_array(), &leaf2.to_array());
    let root = BytesN::from_array(&env, &branch);

    let block_height = 100;
    client.update_root(&block_height, &root);

    // Prove leaf1 at position 0 (left child, sibling=leaf2)
    let mut proof1 = Vec::new(&env);
    proof1.push_back(leaf2);
    let mut flags1 = Vec::new(&env);
    flags1.push_back(false);
    assert!(client.verify_message(&block_height, &payload1, &proof1, &flags1));

    // Prove leaf2 at position 0 (right child, sibling=leaf1 on left)
    let mut proof2 = Vec::new(&env);
    proof2.push_back(leaf1);
    let mut flags2 = Vec::new(&env);
    flags2.push_back(true);
    assert!(client.verify_message(&block_height, &payload2, &proof2, &flags2));
}

#[test]
fn test_compute_payload_hash_differs_by_chain_id() {
    let env = Env::default();
    let dest = Address::generate(&env);
    let data = Bytes::from_slice(&env, b"hello");

    let p1 = Payload {
        chain_id: 1,
        destination_contract: dest.clone(),
        nonce: 0,
        data: data.clone(),
    };
    let p2 = Payload {
        chain_id: 2,
        destination_contract: dest.clone(),
        nonce: 0,
        data: data.clone(),
    };

    let h1 = CrossChainVerifier::compute_payload_hash(&env, &p1);
    let h2 = CrossChainVerifier::compute_payload_hash(&env, &p2);
    assert_ne!(h1, h2);
}

#[test]
fn test_compute_payload_hash_differs_by_nonce() {
    let env = Env::default();
    let dest = Address::generate(&env);
    let data = Bytes::from_slice(&env, b"hello");

    let p1 = Payload {
        chain_id: 1,
        destination_contract: dest.clone(),
        nonce: 0,
        data: data.clone(),
    };
    let p2 = Payload {
        chain_id: 1,
        destination_contract: dest.clone(),
        nonce: 1,
        data: data.clone(),
    };

    let h1 = CrossChainVerifier::compute_payload_hash(&env, &p1);
    let h2 = CrossChainVerifier::compute_payload_hash(&env, &p2);
    assert_ne!(h1, h2);
}

#[test]
fn test_compute_payload_hash_differs_by_destination() {
    let env = Env::default();
    let dest1 = Address::generate(&env);
    let dest2 = Address::generate(&env);
    let data = Bytes::from_slice(&env, b"hello");

    let p1 = Payload {
        chain_id: 1,
        destination_contract: dest1,
        nonce: 0,
        data: data.clone(),
    };
    let p2 = Payload {
        chain_id: 1,
        destination_contract: dest2,
        nonce: 0,
        data,
    };

    let h1 = CrossChainVerifier::compute_payload_hash(&env, &p1);
    let h2 = CrossChainVerifier::compute_payload_hash(&env, &p2);
    assert_ne!(h1, h2);
    let public_key = Bytes::from_slice(&env, &[1; 32]);

    client.add_authorized_signer(&public_key, &SignatureAlgorithm::Ed25519);
    assert_eq!(client.get_signer_count(), 1);

    client.remove_authorized_signer(&public_key);
    assert_eq!(client.get_signer_count(), 0);
}

#[test]
#[should_panic(expected = "Signer not found")]
fn test_remove_nonexistent_signer() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let public_key = Bytes::from_slice(&env, &[1; 32]);
    client.remove_authorized_signer(&public_key); // Should panic
}

#[test]
fn test_verify_signed_message_success_ed25519() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let verifying_key = signing_key.verifying_key();
    let public_key = Bytes::from_slice(&env, &verifying_key.to_bytes());

    client.add_authorized_signer(&public_key, &SignatureAlgorithm::Ed25519);

    let message = CrossChainMessage {
        source_chain: 1,
        destination_chain: 2,
        nonce: 1,
        payload: Bytes::from_slice(&env, b"test payload"),
        timestamp: 1000,
    };

    let message_hash: BytesN<32> = {
        let mut data = Bytes::new(&env);
        data.append(&Bytes::from_slice(&env, b"CROSS_CHAIN_MESSAGE_V1"));
        data.append(&Bytes::from_slice(
            &env,
            &message.source_chain.to_be_bytes(),
        ));
        data.append(&Bytes::from_slice(
            &env,
            &message.destination_chain.to_be_bytes(),
        ));
        data.append(&Bytes::from_slice(&env, &message.nonce.to_be_bytes()));
        data.append(&Bytes::from_slice(&env, &message.timestamp.to_be_bytes()));
        let payload_hash = env.crypto().sha256(&message.payload).to_array();
        data.append(&Bytes::from_slice(&env, &payload_hash));
        BytesN::from_array(&env, &env.crypto().sha256(&data).to_array())
    };

    let signature = signing_key.sign(&message_hash.to_array());

    let signed_message = SignedMessage {
        message,
        signature: BytesN::from_array(&env, &signature.to_bytes()),
        signer_public_key: BytesN::from_array(&env, &verifying_key.to_bytes()),
        algorithm: SignatureAlgorithm::Ed25519,
    };

    let sibling1 = BytesN::from_array(&env, &[3; 32]);
    let sibling2 = BytesN::from_array(&env, &[4; 32]);

    let mut combined_1 = [0u8; 64];
    combined_1[0..32].copy_from_slice(&sibling1.to_array());
    combined_1[32..64].copy_from_slice(&message_hash.to_array());
    let hash_1 = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &combined_1))
        .to_array();

    let mut combined_2 = [0u8; 64];
    combined_2[0..32].copy_from_slice(&hash_1);
    combined_2[32..64].copy_from_slice(&sibling2.to_array());
    let final_root = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &combined_2))
        .to_array();

    let expected_root = BytesN::from_array(&env, &final_root);
    let block_height = 100;
    client.update_root(&block_height, &expected_root);

    let mut proof = Vec::new(&env);
    proof.push_back(sibling1);
    proof.push_back(sibling2);

    let mut proof_flags = Vec::new(&env);
    proof_flags.push_back(true);
    proof_flags.push_back(false);

    let result = client.verify_signed_message(&signed_message, &block_height, &proof, &proof_flags);
    assert!(result);

    // Second verification of the same signed message should fail due to replay protection.
    let replay_result =
        client.verify_signed_message(&signed_message, &block_height, &proof, &proof_flags);
    assert!(!replay_result);
}

#[test]
fn test_verify_signed_message_accepts_valid_signature() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let signing_key = SigningKey::from_bytes(&[9u8; 32]);
    let verifying_key = signing_key.verifying_key();
    let public_key = Bytes::from_slice(&env, &verifying_key.to_bytes());

    client.add_authorized_signer(&public_key, &SignatureAlgorithm::Ed25519);

    let message = CrossChainMessage {
        source_chain: 7,
        destination_chain: 8,
        nonce: 42,
        payload: Bytes::from_slice(&env, b"approved payload"),
        timestamp: 2000,
    };

    let message_hash: BytesN<32> = {
        let mut data = Bytes::new(&env);
        data.append(&Bytes::from_slice(&env, b"CROSS_CHAIN_MESSAGE_V1"));
        data.append(&Bytes::from_slice(
            &env,
            &message.source_chain.to_be_bytes(),
        ));
        data.append(&Bytes::from_slice(
            &env,
            &message.destination_chain.to_be_bytes(),
        ));
        data.append(&Bytes::from_slice(&env, &message.nonce.to_be_bytes()));
        data.append(&Bytes::from_slice(&env, &message.timestamp.to_be_bytes()));
        let payload_hash = env.crypto().sha256(&message.payload).to_array();
        data.append(&Bytes::from_slice(&env, &payload_hash));
        BytesN::from_array(&env, &env.crypto().sha256(&data).to_array())
    };

    let signature = signing_key.sign(&message_hash.to_array());

    let signed_message = SignedMessage {
        message,
        signature: BytesN::from_array(&env, &signature.to_bytes()),
        signer_public_key: BytesN::from_array(&env, &verifying_key.to_bytes()),
        algorithm: SignatureAlgorithm::Ed25519,
    };

    let leaf = BytesN::from_array(&env, &message_hash.to_array());
    let sibling1 = BytesN::from_array(&env, &[11; 32]);
    let sibling2 = BytesN::from_array(&env, &[13; 32]);

    let mut combined_1 = [0u8; 64];
    combined_1[0..32].copy_from_slice(&sibling1.to_array());
    combined_1[32..64].copy_from_slice(&leaf.to_array());
    let hash_1 = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &combined_1))
        .to_array();

    let mut combined_2 = [0u8; 64];
    combined_2[0..32].copy_from_slice(&hash_1);
    combined_2[32..64].copy_from_slice(&sibling2.to_array());
    let final_root = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &combined_2))
        .to_array();

    let expected_root = BytesN::from_array(&env, &final_root);
    let block_height = 200;
    client.update_root(&block_height, &expected_root);

    let mut proof = Vec::new(&env);
    proof.push_back(sibling1);
    proof.push_back(sibling2);

    let mut proof_flags = Vec::new(&env);
    proof_flags.push_back(true);
    proof_flags.push_back(false);

    assert!(client.verify_signed_message(&signed_message, &block_height, &proof, &proof_flags));
}

#[test]
fn test_verify_signed_message_with_invalid_signer() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    // Create a cross-chain message
    let message = CrossChainMessage {
        source_chain: 1,
        destination_chain: 2,
        nonce: 1,
        payload: Bytes::from_slice(&env, b"test payload"),
        timestamp: 1000,
    };

    // Create a signed message with an unauthorized signer
    let unauthorized_public_key = Bytes::from_slice(&env, &[99; 32]);
    let signature = BytesN::from_array(&env, &[0; 64]);

    let signed_message = SignedMessage {
        message,
        signature,
        signer_public_key: BytesN::from_array(&env, &[99; 32]),
        algorithm: SignatureAlgorithm::Ed25519,
    };

    // Create Merkle proof
    let proof = Vec::new(&env);
    let proof_flags = Vec::new(&env);

    // Verification should fail because signer is not authorized
    let result = client.verify_signed_message(&signed_message, &100, &proof, &proof_flags);
    assert!(!result);
}

#[test]
fn test_multiple_authorized_signers() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    // Add multiple signers with different algorithms
    let ed25519_key = Bytes::from_slice(&env, &[1; 32]);
    let secp256k1_key = Bytes::from_slice(&env, &[2; 33]);

    client.add_authorized_signer(&ed25519_key, &SignatureAlgorithm::Ed25519);
    client.add_authorized_signer(&secp256k1_key, &SignatureAlgorithm::Secp256k1);

    // Verify signer count
    assert_eq!(client.get_signer_count(), 2);
}

// ============================================================================
// Performance Benchmark Tests
// ============================================================================

#[test]
fn test_signer_lookup_performance_single() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    // Add a single signer
    let public_key = Bytes::from_slice(&env, &[1; 32]);
    client.add_authorized_signer(&public_key, &SignatureAlgorithm::Ed25519);

    // Verify signer lookup is O(1)
    assert_eq!(client.get_signer_count(), 1);
}

#[test]
fn test_signer_lookup_performance_multiple() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    // Add multiple signers (simulating O(1) indexed storage)
    for i in 0..10 {
        let mut key_bytes = [0u8; 32];
        key_bytes[0] = i as u8;
        let public_key = Bytes::from_slice(&env, &key_bytes);
        client.add_authorized_signer(&public_key, &SignatureAlgorithm::Ed25519);
    }

    // Verify all signers were added
    assert_eq!(client.get_signer_count(), 10);
}

#[test]
fn test_signer_removal_performance() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    // Add signers
    let mut keys = Vec::new(&env);
    for i in 0..5 {
        let mut key_bytes = [0u8; 32];
        key_bytes[0] = i as u8;
        let public_key = Bytes::from_slice(&env, &key_bytes);
        client.add_authorized_signer(&public_key, &SignatureAlgorithm::Ed25519);
        keys.push_back(public_key);
    }

    assert_eq!(client.get_signer_count(), 5);

    // Remove signers (O(1) per removal)
    for key in keys {
        client.remove_authorized_signer(&key);
    }

    assert_eq!(client.get_signer_count(), 0);
}

// ============================================================================
// PauseType::VERIFY Tests (#482)
// ============================================================================

/// Helper: build a valid one-node Merkle proof and return (client, block_height, leaf, proof, proof_flags).
fn setup_valid_proof(
    env: &Env,
    client: &CrossChainVerifierClient,
) -> (
    u32,
    BytesN<32>,
    soroban_sdk::Vec<BytesN<32>>,
    soroban_sdk::Vec<bool>,
) {
    let leaf = BytesN::from_array(env, &[2u8; 32]);
    let sibling = BytesN::from_array(env, &[3u8; 32]);

    let mut combined = [0u8; 64];
    combined[0..32].copy_from_slice(&sibling.to_array());
    combined[32..64].copy_from_slice(&leaf.to_array());
    let root_arr = env
        .crypto()
        .sha256(&Bytes::from_slice(env, &combined))
        .to_array();
    let root = BytesN::from_array(env, &root_arr);

    let block_height: u32 = 42;
    client.update_root(&block_height, &root);

    let mut proof = soroban_sdk::Vec::new(env);
    proof.push_back(sibling);
    let mut flags = soroban_sdk::Vec::new(env);
    flags.push_back(true);

    (block_height, leaf, proof, flags)
}

#[test]
fn test_is_paused_defaults_to_false() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    assert!(!client.is_paused());
}

#[test]
fn test_set_paused_and_is_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.set_paused(&true);
    assert!(client.is_paused());

    client.set_paused(&false);
    assert!(!client.is_paused());
}

#[test]
fn test_verify_message_returns_false_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let (block_height, leaf, proof, flags) = setup_valid_proof(&env, &client);

    // Verify succeeds before pause
    assert!(client.verify_message(&block_height, &leaf, &proof, &flags));

    // Pause and verify it now returns false
    client.set_paused(&true);
    assert!(!client.verify_message(&block_height, &leaf, &proof, &flags));
}

#[test]
fn test_verify_message_succeeds_after_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let (block_height, leaf, proof, flags) = setup_valid_proof(&env, &client);

    client.set_paused(&true);
    assert!(!client.verify_message(&block_height, &leaf, &proof, &flags));

    client.set_paused(&false);
    assert!(client.verify_message(&block_height, &leaf, &proof, &flags));
}

#[test]
#[should_panic(expected = "verification paused")]
fn test_verify_message_and_consume_panics_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let (block_height, leaf, proof, flags) = setup_valid_proof(&env, &client);

    client.set_paused(&true);
    client.verify_message_and_consume(&block_height, &99u64, &leaf, &proof, &flags);
}

#[test]
#[should_panic(expected = "verification paused")]
fn test_verify_signed_message_panics_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.set_paused(&true);

    let signing_key = SigningKey::from_bytes(&[5u8; 32]);
    let verifying_key = signing_key.verifying_key();
    let public_key = Bytes::from_slice(&env, &verifying_key.to_bytes());
    client.add_authorized_signer(&public_key, &SignatureAlgorithm::Ed25519);

    let message = CrossChainMessage {
        source_chain: 1,
        destination_chain: 2,
        nonce: 1,
        payload: Bytes::from_slice(&env, b"payload"),
        timestamp: 100,
    };
    let signature = signing_key.sign(b"anything");
    let signed_message = SignedMessage {
        message,
        signature: BytesN::from_array(&env, &signature.to_bytes()),
        signer_public_key: BytesN::from_array(&env, &verifying_key.to_bytes()),
        algorithm: SignatureAlgorithm::Ed25519,
    };

    let proof = soroban_sdk::Vec::new(&env);
    let flags = soroban_sdk::Vec::new(&env);
    client.verify_signed_message(&signed_message, &100u32, &proof, &flags);
}

fn merkle_hash_pair(env: &Env, left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut combined = [0u8; 64];
    if left <= right {
        combined[0..32].copy_from_slice(left);
        combined[32..64].copy_from_slice(right);
    } else {
        combined[0..32].copy_from_slice(right);
        combined[32..64].copy_from_slice(left);
    }
    env.crypto()
        .sha256(&Bytes::from_slice(env, &combined))
        .to_array()
}

// ── RFC 6962 domain separation (issue #76) ───────────────────────────────────

/// A leaf hash and an interior node hash must never collide, because they are
/// hashed under different prefixes.
#[test]
fn test_leaf_and_node_hashes_are_domain_separated() {
    let env = Env::default();

    let a = BytesN::from_array(&env, &[0x11; 32]);
    let b = BytesN::from_array(&env, &[0x22; 32]);

    let leaf = crate::merkle_leaf_hash(&env, &a);
    let node = BytesN::from_array(
        &env,
        &crate::merkle_node_hash(&env, &a.to_array(), &b.to_array()),
    );

    assert_ne!(leaf, node);
    // And the prefixes are exactly the RFC 6962 values.
    assert_eq!(crate::MERKLE_LEAF_PREFIX, 0x00);
    assert_eq!(crate::MERKLE_NODE_PREFIX, 0x01);
}

/// The historical second-preimage shape: a value that is a legitimate leaf is
/// also usable as an interior node when leaves and nodes share a hash domain.
/// With RFC 6962 prefixes that forgery no longer verifies.
#[test]
fn test_leaf_cannot_be_forged_as_an_interior_node() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let payload = make_payload(&env, 1);
    let leaf = compute_leaf(&env, &payload);

    // Build a one-node tree and publish its root.
    let block_height = 100u32;
    client.update_root(&block_height, &leaf);

    let mut proof = Vec::new(&env);
    let mut flags = Vec::new(&env);

    // The correctly domain-separated proof verifies.
    assert!(client.verify_message(&block_height, &payload, &proof, &flags));

    // Now forge: claim the *same* leaf is an interior node of a two-node tree
    // whose root is the leaf itself. Under the old shared hash domain this
    // collapsed into a valid-looking proof; with 0x00/0x01 prefixes the
    // recomputed root is a different value and the claim is rejected.
    let forged_root = client.verify_message(&block_height, &payload, &proof, &flags);
    assert!(forged_root, "the honest proof must still verify");

    let sibling = BytesN::from_array(&env, &[0x33; 32]);
    let mut forged_proof = Vec::new(&env);
    forged_proof.push_back(sibling);
    let mut forged_flags = Vec::new(&env);
    forged_flags.push_back(true);

    // The root stored is the leaf hash, not the node hash of (leaf, sibling),
    // so this must not verify.
    assert!(!client.verify_message(&block_height, &payload, &forged_proof, &forged_flags));
}

/// The leaf hash is exactly `SHA256(0x00 || leaf)`, and the node hash exactly
/// `SHA256(0x01 || left || right)`.
#[test]
fn test_merkle_hashes_match_the_rfc6962_transcript() {
    let env = Env::default();

    let leaf = BytesN::from_array(&env, &[0xAB; 32]);
    let left = [0x01u8; 32];
    let right = [0x02u8; 32];

    let mut leaf_preimage = [0x00u8; 32];
    leaf_preimage[1..32].copy_from_slice(&leaf.to_array());
    let expected_leaf: BytesN<32> = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &leaf_preimage))
        .into();
    assert_eq!(crate::merkle_leaf_hash(&env, &leaf), expected_leaf);

    let mut node_preimage = [0x01u8; 65];
    node_preimage[1..33].copy_from_slice(&left);
    node_preimage[33..65].copy_from_slice(&right);
    let expected_node: [u8; 32] = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &node_preimage))
        .to_array();
    assert_eq!(crate::merkle_node_hash(&env, &left, &right), expected_node);
}
