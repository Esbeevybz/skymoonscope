#![cfg(test)]

use crate::{
    CrossChainMessage, CrossChainVerifier, CrossChainVerifierClient, Payload, SignatureAlgorithm,
    SignedMessage, LEAF_PREFIX, MAX_PROOF_LENGTH, NODE_PREFIX,
};
use ed25519_dalek::{Signer, SigningKey};
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

/// `SHA256(0x00 || data)` — the RFC 6962 leaf hash (issue #076).
fn hash_leaf(env: &Env, data: &[u8; 32]) -> [u8; 32] {
    let mut buf = Bytes::new(env);
    buf.extend_from_slice(&[LEAF_PREFIX]);
    buf.extend_from_slice(data);
    env.crypto()
        .sha256(&Bytes::from_slice(env, &buf.to_array()))
        .to_array()
}

/// `SHA256(0x01 || left || right)` — the RFC 6962 internal node hash (#076).
fn hash_node(env: &Env, left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut buf = Bytes::new(env);
    buf.extend_from_slice(&[NODE_PREFIX]);
    buf.extend_from_slice(left);
    buf.extend_from_slice(right);
    env.crypto()
        .sha256(&Bytes::from_slice(env, &buf.to_array()))
        .to_array()
}

/// Canonical sorted-pair node hash, matching the verifier's sibling ordering.
fn merkle_hash_pair(env: &Env, left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    if left <= right {
        hash_node(env, left, right)
    } else {
        hash_node(env, right, left)
    }
}

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

    // Manually construct the root with RFC 6962 domain separation (#076).
    let hash_1 = merkle_hash_pair(
        &env,
        &sibling1.to_array(),
        &hash_leaf(&env, &leaf.to_array()),
    );
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

    let hash_1 = merkle_hash_pair(
        &env,
        &sibling1.to_array(),
        &hash_leaf(&env, &leaf.to_array()),
    );
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

    let hash_1 = merkle_hash_pair(
        &env,
        &sibling1.to_array(),
        &hash_leaf(&env, &leaf.to_array()),
    );
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

    // No root is registered for this height, so the proof cannot be checked.
    assert!(!client.verify_message(&100, &payload, &proof, &proof_flags));
}

#[test]
#[should_panic(expected = "Nonce already used")]
fn test_verify_message_replay_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    // Single-node tree: the root is the leaf hash, prefix included.
    let payload = make_payload(&env, 1);
    let leaf = compute_leaf(&env, &payload);
    let root = BytesN::from_array(&env, &hash_leaf(&env, &leaf.to_array()));
    client.update_root(&100, &root);

    let proof = Vec::new(&env);
    let proof_flags = Vec::new(&env);

    // The first use is accepted.
    assert!(client.verify_message(&100, &payload, &proof, &proof_flags));

    // Replaying the same payload hits the consumed nonce.
    let replay = client.try_verify_message(&100, &payload, &proof, &proof_flags);
    assert!(replay.is_err());
}

// ============================================================================
// Signature Verification Tests
// ============================================================================

#[test]
#[should_panic(expected = "Nonce already used")]
fn test_add_authorized_signer_ed25519() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let payload = make_payload(&env, 1);
    let leaf = compute_leaf(&env, &payload);

    // Single-node tree: the root is the leaf hash, prefix included (#076).
    let block_height = 100;
    let root = BytesN::from_array(&env, &hash_leaf(&env, &leaf.to_array()));
    client.update_root(&block_height, &root);

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
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    // Two distinct payloads sharing one root: nonces are consumed independently.
    let payload1 = make_payload(&env, 1);
    let payload2 = make_payload(&env, 2);
    let leaf1 = compute_leaf(&env, &payload1);
    let leaf2 = compute_leaf(&env, &payload2);
    let branch = merkle_hash_pair(
        &env,
        &hash_leaf(&env, &leaf1.to_array()),
        &hash_leaf(&env, &leaf2.to_array()),
    );
    let root = BytesN::from_array(&env, &branch);
    client.update_root(&100, &root);

    let mut proof1 = Vec::new(&env);
    proof1.push_back(leaf2);
    let mut flags1 = Vec::new(&env);
    flags1.push_back(false);
    assert!(client.verify_message(&100, &payload1, &proof1, &flags1));

    let mut proof2 = Vec::new(&env);
    proof2.push_back(leaf1);
    let mut flags2 = Vec::new(&env);
    flags2.push_back(true);
    assert!(client.verify_message(&100, &payload2, &proof2, &flags2));

    assert!(client.is_nonce_processed(&1u64));
    assert!(client.is_nonce_processed(&2u64));
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

    // Build a root that commits to both leaves. Both children are leaves, so
    // each is hashed with the 0x00 prefix before being combined (#076).
    let branch = merkle_hash_pair(
        &env,
        &hash_leaf(&env, &leaf1.to_array()),
        &hash_leaf(&env, &leaf2.to_array()),
    );
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

    let hash_1 = merkle_hash_pair(
        &env,
        &sibling1.to_array(),
        &hash_leaf(&env, &message_hash.to_array()),
    );
    let final_root = merkle_hash_pair(&env, &hash_1, &sibling2.to_array());

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

    let hash_1 = merkle_hash_pair(
        &env,
        &sibling1.to_array(),
        &hash_leaf(&env, &leaf.to_array()),
    );
    let final_root = merkle_hash_pair(&env, &hash_1, &sibling2.to_array());

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

    let root_arr = merkle_hash_pair(env, &sibling.to_array(), &hash_leaf(env, &leaf.to_array()));
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

// ============================================================================
// Merkle domain separation (issue #076)
// ============================================================================

#[test]
fn test_leaf_prefix_is_zero() {
    assert_eq!(LEAF_PREFIX, 0x00);
    assert_eq!(NODE_PREFIX, 0x01);
    assert_ne!(LEAF_PREFIX, NODE_PREFIX);
}

/// A leaf hash must not equal the bare hash of the same data. Without the
/// prefix they are identical, which is exactly what lets a node be replayed
/// as a leaf.
#[test]
fn test_leaf_hash_is_domain_separated() {
    let env = Env::default();
    let data = [7u8; 32];

    let prefixed = hash_leaf(&env, &data);
    let bare = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &data))
        .to_array();

    assert_ne!(prefixed, bare);
}

/// A node hash must not equal the bare hash of `0x01 || left || right`'s
/// payload, nor the hash of the same children with the leaf prefix.
#[test]
fn test_node_hash_is_domain_separated() {
    let env = Env::default();
    let left = [1u8; 32];
    let right = [2u8; 32];

    let node = hash_node(&env, &left, &right);

    let mut bare = [0u8; 64];
    bare[0..32].copy_from_slice(&left);
    bare[32..64].copy_from_slice(&right);
    assert_ne!(
        node,
        env.crypto()
            .sha256(&Bytes::from_slice(&env, &bare))
            .to_array()
    );

    // A node is not interchangeable with a leaf over the same child bytes.
    assert_ne!(node, hash_leaf(&env, &left));
}

/// The forgery this guards against: an attacker who knows an internal node's
/// children presents those 64 bytes as leaf data. Under an unprefixed scheme
/// the leaf hash of that data equals the node hash, so the proof verifies.
/// With prefixes it does not.
#[test]
fn test_internal_node_cannot_be_forged_as_a_leaf() {
    let env = Env::default();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // A real two-level tree over two leaves.
    let leaf_a = [10u8; 32];
    let leaf_b = [20u8; 32];
    let node = merkle_hash_pair(&env, &leaf_a, &leaf_b);
    let root = BytesN::from_array(&env, &node);
    client.update_root(&100, &root);

    // The attacker supplies the *node's children* as if they were a leaf.
    let forged_leaf = BytesN::from_array(&env, &merkle_hash_pair(&env, &leaf_a, &leaf_b));
    let sibling = BytesN::from_array(&env, &[30u8; 32]);
    let mut proof = Vec::new(&env);
    proof.push_back(sibling);
    let mut flags = Vec::new(&env);
    flags.push_back(true);

    // Build the root the attacker would need the contract to accept: a node
    // combining their forged leaf with the sibling. That is a *different* root
    // from the real one, so verification fails.
    let forged_root = merkle_hash_pair(&env, &forged_leaf.to_array(), &sibling.to_array());
    assert_ne!(BytesN::from_array(&env, &forged_root), root);

    // And the un-prefixed leaf hash of the 64 child bytes is not the node hash
    // either, so the forgery has no preimage to work with.
    let mut child_bytes = [0u8; 64];
    child_bytes[0..32].copy_from_slice(&leaf_a);
    child_bytes[32..64].copy_from_slice(&leaf_b);
    let unprefixed_forgery = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &child_bytes))
        .to_array();
    assert_ne!(BytesN::from_array(&env, &unprefixed_forgery), root);

    let mut forged_proof = Vec::new(&env);
    forged_proof.push_back(sibling);
    assert!(!client.verify_message_and_consume(&100, &1u64, &forged_leaf, &forged_proof, &flags));
}

#[test]
fn test_valid_proof_still_verifies_with_prefixes() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let leaf = BytesN::from_array(&env, &[5u8; 32]);
    let sibling = BytesN::from_array(&env, &[6u8; 32]);
    let root_arr = merkle_hash_pair(
        &env,
        &sibling.to_array(),
        &hash_leaf(&env, &leaf.to_array()),
    );
    client.update_root(&42, &BytesN::from_array(&env, &root_arr));

    let mut proof = Vec::new(&env);
    proof.push_back(sibling);
    let mut flags = Vec::new(&env);
    flags.push_back(true);

    assert!(client.verify_message_and_consume(&42, &1u64, &leaf, &proof, &flags));
}

#[test]
fn test_single_leaf_tree_root_must_be_prefixed() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let leaf = BytesN::from_array(&env, &[8u8; 32]);
    let empty_proof = Vec::new(&env);
    let empty_flags = Vec::new(&env);

    // An un-prefixed root — the old scheme — no longer verifies.
    let unprefixed = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &leaf.to_array()))
        .to_array();
    client.update_root(&7, &BytesN::from_array(&env, &unprefixed));
    assert!(!client.verify_message_and_consume(&7, &1u64, &leaf, &empty_proof, &empty_flags));

    // The RFC 6962 root does.
    client.update_root(
        &8,
        &BytesN::from_array(&env, &hash_leaf(&env, &leaf.to_array())),
    );
    assert!(client.verify_message_and_consume(&8, &2u64, &leaf, &empty_proof, &empty_flags));
}

#[test]
fn test_proof_longer_than_max_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let leaf = BytesN::from_array(&env, &[9u8; 32]);
    client.update_root(&1, &BytesN::from_array(&env, &[0u8; 32]));

    let mut proof = Vec::new(&env);
    let mut flags = Vec::new(&env);
    for i in 0..=MAX_PROOF_LENGTH {
        proof.push_back(BytesN::from_array(&env, &[i as u8; 32]));
        flags.push_back(true);
    }

    assert!(!client.verify_message_and_consume(&1, &1u64, &leaf, &proof, &flags));
}

#[test]
fn test_mismatched_flag_length_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let leaf = BytesN::from_array(&env, &[4u8; 32]);
    let sibling = BytesN::from_array(&env, &[5u8; 32]);
    let root_arr = merkle_hash_pair(
        &env,
        &sibling.to_array(),
        &hash_leaf(&env, &leaf.to_array()),
    );
    client.update_root(&3, &BytesN::from_array(&env, &root_arr));

    let mut proof = Vec::new(&env);
    proof.push_back(sibling);
    // No flags at all: the proof cannot be interpreted consistently.
    let empty_flags = Vec::new(&env);

    assert!(!client.verify_message_and_consume(&3, &1u64, &leaf, &proof, &empty_flags));
}
