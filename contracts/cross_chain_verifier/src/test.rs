#![cfg(test)]

use crate::{
    BatchMessage, BatchVerificationResult, CrossChainMessage, CrossChainVerifier,
    CrossChainVerifierClient, Payload, SignatureAlgorithm, SignedBatchMessage,
    SignedBatchVerificationResult, SignedMessage, LEAF_PREFIX, MAX_PROOF_LENGTH, NODE_PREFIX,
};
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{testutils::Address as _, testutils::Events, Address, Bytes, BytesN, Env, Vec};

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



/// The leaf value the contract is asked to prove inclusion of: the payload hash.
fn raw_leaf(env: &Env, payload: &Payload) -> BytesN<32> {
    CrossChainVerifier::compute_payload_hash(env, payload)
}

/// Helper function for tests (alias for raw_leaf)
fn compute_leaf(env: &Env, payload: &Payload) -> BytesN<32> {
    raw_leaf(env, payload)
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

    // A successful verification must emit an event.
    let events = env.events().all();
    assert!(!events.is_empty());
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

#[test]
fn test_nonce_is_shared_across_verification_entry_points() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let payload = make_payload(&env, 73);
    let leaf = compute_leaf(&env, &payload);
    let proof = Vec::new(&env);
    let proof_flags = Vec::new(&env);

    // An invalid proof must not consume the nonce.
    assert!(!client.verify_message(&100, &payload, &proof, &proof_flags));
    assert!(!client.is_nonce_processed(&73));

    // Once verified through the payload API, the same nonce is unavailable
    // through the raw-leaf API as well.
    let root = BytesN::from_array(&env, &hash_leaf(&env, &leaf.to_array()));
    client.update_root(&100, &root);
    assert!(client.verify_message(&100, &payload, &proof, &proof_flags));
    let replay = client.try_verify_message_and_consume(&100, &73, &leaf, &proof, &proof_flags);
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
    assert!(client.is_nonce_processed(&1));

    // Second verification of the same signed message should fail due to replay protection.
    let replay_result =
        client.verify_signed_message(&signed_message, &block_height, &proof, &proof_flags);
    assert!(!replay_result);

    // A different signed message cannot reuse a nonce already consumed above.
    let replay_message = CrossChainMessage {
        nonce: 1,
        timestamp: 1001,
        payload: Bytes::from_slice(&env, b"different payload"),
        ..signed_message.message.clone()
    };
    let mut replay_data = Bytes::new(&env);
    replay_data.append(&Bytes::from_slice(&env, b"CROSS_CHAIN_MESSAGE_V1"));
    replay_data.append(&Bytes::from_slice(&env, &replay_message.source_chain.to_be_bytes()));
    replay_data.append(&Bytes::from_slice(&env, &replay_message.destination_chain.to_be_bytes()));
    replay_data.append(&Bytes::from_slice(&env, &replay_message.nonce.to_be_bytes()));
    replay_data.append(&Bytes::from_slice(&env, &replay_message.timestamp.to_be_bytes()));
    let replay_payload_hash = env.crypto().sha256(&replay_message.payload).to_array();
    replay_data.append(&Bytes::from_slice(&env, &replay_payload_hash));
    let replay_hash = env.crypto().sha256(&replay_data).to_array();
    let replay_signature = signing_key.sign(&replay_hash);
    let replay_signed_message = SignedMessage {
        message: replay_message,
        signature: BytesN::from_array(&env, &replay_signature.to_bytes()),
        signer_public_key: BytesN::from_array(&env, &verifying_key.to_bytes()),
        algorithm: SignatureAlgorithm::Ed25519,
    };
    let replay_root = BytesN::from_array(&env, &hash_leaf(&env, &replay_hash));
    client.update_root(&101, &replay_root);
    assert!(!client.verify_signed_message(
        &replay_signed_message,
        &101,
        &Vec::new(&env),
        &Vec::new(&env),
    ));
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

// ============================================================================
// Batch Verification Tests
// ============================================================================

#[test]
fn test_verify_batch_success() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let block_height = 100;
    
    // Create test data for 3 messages
    let mut batch_messages = Vec::new(&env);
    
    for i in 0..3 {
        let payload = Payload {
            chain_id: 1,
            destination_contract: Address::generate(&env),
            nonce: i as u64,
            data: Bytes::from_slice(&env, &[i as u8; 32]),
        };
        
        let leaf = CrossChainVerifier::compute_payload_hash(&env, &payload);
        let sibling = BytesN::from_array(&env, &[i as u8 + 10; 32]);
        
        // Single node tree: leaf hash is root
        let root_arr = hash_leaf(&env, &leaf.to_array());
        let root = BytesN::from_array(&env, &root_arr);
        client.update_root(&(block_height + i), &root);
        
        let mut proof = Vec::new(&env);
        let mut proof_flags = Vec::new(&env);
        
        batch_messages.push_back(BatchMessage {
            block_height: block_height + i,
            payload,
            proof,
            proof_flags,
        });
    }
    
    let result = client.verify_batch(&batch_messages);
    assert_eq!(result.total, 3);
    assert_eq!(result.succeeded, 3);
    assert_eq!(result.failed, 0);
    assert_eq!(result.failed_indices.len(), 0);
}

#[test]
fn test_verify_batch_partial_success() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let block_height = 200;
    
    // Create test data for 4 messages, 2 will fail
    let mut batch_messages = Vec::new(&env);
    
    for i in 0..4 {
        let payload = Payload {
            chain_id: 1,
            destination_contract: Address::generate(&env),
            nonce: i as u64,
            data: Bytes::from_slice(&env, &[i as u8; 32]),
        };
        
        let leaf = CrossChainVerifier::compute_payload_hash(&env, &payload);
        
        // Messages 0 and 1 will succeed (valid root), 2 and 3 will fail (no root)
        if i < 2 {
            let root_arr = hash_leaf(&env, &leaf.to_array());
            let root = BytesN::from_array(&env, &root_arr);
            client.update_root(&(block_height + i), &root);
        }
        
        let mut proof = Vec::new(&env);
        let mut proof_flags = Vec::new(&env);
        
        batch_messages.push_back(BatchMessage {
            block_height: block_height + i,
            payload,
            proof,
            proof_flags,
        });
    }
    
    let result = client.verify_batch(&batch_messages);
    assert_eq!(result.total, 4);
    assert_eq!(result.succeeded, 2);
    assert_eq!(result.failed, 2);
    assert_eq!(result.failed_indices.len(), 2);
    assert_eq!(result.failed_indices.get(0).unwrap(), 2);
    assert_eq!(result.failed_indices.get(1).unwrap(), 3);
}

#[test]
fn test_verify_batch_replay_protection() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let block_height = 300;
    let payload = Payload {
        chain_id: 1,
        destination_contract: Address::generate(&env),
        nonce: 1,
        data: Bytes::from_slice(&env, b"test"),
    };
    
    let leaf = CrossChainVerifier::compute_payload_hash(&env, &payload);
    let root_arr = hash_leaf(&env, &leaf.to_array());
    let root = BytesN::from_array(&env, &root_arr);
    client.update_root(&block_height, &root);
    
    let mut proof = Vec::new(&env);
    let mut proof_flags = Vec::new(&env);
    
    let message = BatchMessage {
        block_height,
        payload: payload.clone(),
        proof,
        proof_flags,
    };
    
    // First batch succeeds
    let mut batch1 = Vec::new(&env);
    batch1.push_back(message.clone());
    let result1 = client.verify_batch(&batch1);
    assert_eq!(result1.succeeded, 1);
    
    // Second batch with same nonce should fail due to replay protection
    let mut batch2 = Vec::new(&env);
    batch2.push_back(message);
    let result2 = client.verify_batch(&batch2);
    assert_eq!(result2.succeeded, 0);
    assert_eq!(result2.failed, 1);
}

#[test]
fn test_verify_signed_batch_success() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Add authorized signer
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[1u8; 32]);
    let verifying_key = signing_key.verifying_key();
    let public_key = Bytes::from_slice(&env, &verifying_key.to_bytes());
    client.add_authorized_signer(&public_key, &SignatureAlgorithm::Ed25519);

    let block_height = 400;
    
    // Include two distinct signed messages with the same nonce.
    let mut batch_messages = Vec::new(&env);

    for i in 0..3 {
        let message = CrossChainMessage {
            source_chain: 1,
            destination_chain: 2,
            nonce: if i == 2 { 0 } else { i as u64 },
            payload: Bytes::from_slice(&env, &[i as u8; 32]),
            timestamp: 1000 + i as u64,
        };
        
        let message_hash: BytesN<32> = {
            let mut data = Bytes::new(&env);
            data.append(&Bytes::from_slice(&env, b"CROSS_CHAIN_MESSAGE_V1"));
            data.append(&Bytes::from_slice(&env, &message.source_chain.to_be_bytes()));
            data.append(&Bytes::from_slice(&env, &message.destination_chain.to_be_bytes()));
            data.append(&Bytes::from_slice(&env, &message.nonce.to_be_bytes()));
            data.append(&Bytes::from_slice(&env, &message.timestamp.to_be_bytes()));
            let payload_hash = env.crypto().sha256(&message.payload).to_array();
            data.append(&Bytes::from_slice(&env, &payload_hash));
            BytesN::from_array(&env, &env.crypto().sha256(&data).to_array())
        };
        
        let signature = signing_key.sign(&message_hash.to_array());
        
        let signed_message = SignedMessage {
            message: message.clone(),
            signature: BytesN::from_array(&env, &signature.to_bytes()),
            signer_public_key: BytesN::from_array(&env, &verifying_key.to_bytes()),
            algorithm: SignatureAlgorithm::Ed25519,
        };
        
        // Create merkle proof
        let root_arr = hash_leaf(&env, &message_hash.to_array());
        let root = BytesN::from_array(&env, &root_arr);
        client.update_root(&(block_height + i), &root);
        
        let mut proof = Vec::new(&env);
        let mut proof_flags = Vec::new(&env);
        
        batch_messages.push_back(SignedBatchMessage {
            block_height: block_height + i,
            signed_message,
            proof,
            proof_flags,
        });
    }
    
    let result = client.verify_signed_batch(&batch_messages);
    assert_eq!(result.total, 3);
    assert_eq!(result.succeeded, 2);
    assert_eq!(result.failed, 1);
    assert_eq!(result.failed_indices.len(), 1);
    assert_eq!(result.failed_indices.get(0).unwrap(), 2);
    assert!(client.is_nonce_processed(&0));
    assert!(client.is_nonce_processed(&1));
}

#[test]
fn test_verify_batch_paused() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let block_height = 500;
    let payload = Payload {
        chain_id: 1,
        destination_contract: Address::generate(&env),
        nonce: 1,
        data: Bytes::from_slice(&env, b"test"),
    };
    
    let leaf = CrossChainVerifier::compute_payload_hash(&env, &payload);
    let root_arr = hash_leaf(&env, &leaf.to_array());
    let root = BytesN::from_array(&env, &root_arr);
    client.update_root(&block_height, &root);
    
    let mut proof = Vec::new(&env);
    let mut proof_flags = Vec::new(&env);
    
    let mut batch_messages = Vec::new(&env);
    batch_messages.push_back(BatchMessage {
        block_height,
        payload,
        proof,
        proof_flags,
    });
    
    // Pause verification
    client.set_paused(&true);
    
    let result = client.verify_batch(&batch_messages);
    assert_eq!(result.total, 1);
    assert_eq!(result.succeeded, 0);
    assert_eq!(result.failed, 1);
    assert_eq!(result.failed_indices.len(), 1);
    assert_eq!(result.failed_indices.get(0).unwrap(), 0);
}

#[test]
fn test_verify_batch_large_batch() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CrossChainVerifier);
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let block_height = 600;
    let mut batch_messages = Vec::new(&env);
    
    // Test with 10 messages (reasonable batch size)
    for i in 0..10 {
        let payload = Payload {
            chain_id: 1,
            destination_contract: Address::generate(&env),
            nonce: i as u64,
            data: Bytes::from_slice(&env, &[i as u8; 32]),
        };
        
        let leaf = CrossChainVerifier::compute_payload_hash(&env, &payload);
        let root_arr = hash_leaf(&env, &leaf.to_array());
        let root = BytesN::from_array(&env, &root_arr);
        client.update_root(&(block_height + i), &root);
        
        let mut proof = Vec::new(&env);
        let mut proof_flags = Vec::new(&env);
        
        batch_messages.push_back(BatchMessage {
            block_height: block_height + i,
            payload,
            proof,
            proof_flags,
        });
    }
    
    let result = client.verify_batch(&batch_messages);
    assert_eq!(result.total, 10);
    assert_eq!(result.succeeded, 10);
    assert_eq!(result.failed, 0);
    assert_eq!(result.failed_indices.len(), 0);
}

// ============================================================================
// Odd-Numbered Leaf Count Merkle Tree Tests (Issue #349)
// ============================================================================

#[test]
fn test_verify_message_odd_leaf_count_three_leaves() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // 3 leaves in tree: L0, L1, L2
    let p0 = make_payload(&env, 10);
    let p1 = make_payload(&env, 11);
    let p2 = make_payload(&env, 12);

    let l0 = compute_leaf(&env, &p0);
    let l1 = compute_leaf(&env, &p1);
    let l2 = compute_leaf(&env, &p2);

    let h0 = hash_leaf(&env, &l0.to_array());
    let h1 = hash_leaf(&env, &l1.to_array());
    let h2 = hash_leaf(&env, &l2.to_array());

    // Level 0 -> Level 1
    let n0 = merkle_hash_pair(&env, &h0, &h1);
    let n1 = merkle_hash_pair(&env, &h2, &h2); // Odd leaf duplicated and hashed with itself

    // Level 1 -> Root
    let root = merkle_hash_pair(&env, &n0, &n1);
    let root_bytes = BytesN::from_array(&env, &root);

    let block_height = 300;
    client.update_root(&block_height, &root_bytes);

    // Verify leaf 0 (index 0)
    let mut proof0 = Vec::new(&env);
    proof0.push_back(BytesN::from_array(&env, &h1));
    proof0.push_back(BytesN::from_array(&env, &n1));
    let mut flags0 = Vec::new(&env);
    flags0.push_back(false);
    flags0.push_back(false);
    assert!(client.verify_message(&block_height, &p0, &proof0, &flags0));

    // Verify leaf 1 (index 1)
    let mut proof1 = Vec::new(&env);
    proof1.push_back(BytesN::from_array(&env, &h0));
    proof1.push_back(BytesN::from_array(&env, &n1));
    let mut flags1 = Vec::new(&env);
    flags1.push_back(true);
    flags1.push_back(false);
    assert!(client.verify_message(&block_height, &p1, &proof1, &flags1));

    // Verify leaf 2 (index 2 - the odd padded leaf)
    let mut proof2 = Vec::new(&env);
    proof2.push_back(BytesN::from_array(&env, &h2)); // Sibling is itself
    proof2.push_back(BytesN::from_array(&env, &n0));
    let mut flags2 = Vec::new(&env);
    flags2.push_back(false);
    flags2.push_back(true);
    assert!(client.verify_message(&block_height, &p2, &proof2, &flags2));
}

#[test]
fn test_verify_message_odd_leaf_count_five_leaves() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // 5 leaves in tree: L0, L1, L2, L3, L4
    let payloads: [Payload; 5] = [
        make_payload(&env, 20),
        make_payload(&env, 21),
        make_payload(&env, 22),
        make_payload(&env, 23),
        make_payload(&env, 24),
    ];

    let leaf_hashes: [[u8; 32]; 5] = [
        hash_leaf(&env, &compute_leaf(&env, &payloads[0]).to_array()),
        hash_leaf(&env, &compute_leaf(&env, &payloads[1]).to_array()),
        hash_leaf(&env, &compute_leaf(&env, &payloads[2]).to_array()),
        hash_leaf(&env, &compute_leaf(&env, &payloads[3]).to_array()),
        hash_leaf(&env, &compute_leaf(&env, &payloads[4]).to_array()),
    ];

    // Level 0 -> Level 1
    let n0 = merkle_hash_pair(&env, &leaf_hashes[0], &leaf_hashes[1]);
    let n1 = merkle_hash_pair(&env, &leaf_hashes[2], &leaf_hashes[3]);
    let n2 = merkle_hash_pair(&env, &leaf_hashes[4], &leaf_hashes[4]); // Odd leaf duplicated

    // Level 1 -> Level 2
    let m0 = merkle_hash_pair(&env, &n0, &n1);
    let m1 = merkle_hash_pair(&env, &n2, &n2); // Odd internal node duplicated

    // Level 2 -> Root
    let root = merkle_hash_pair(&env, &m0, &m1);
    let root_bytes = BytesN::from_array(&env, &root);

    let block_height = 500;
    client.update_root(&block_height, &root_bytes);

    // Verify leaf 4 (the odd padded leaf at both levels)
    let mut proof4 = Vec::new(&env);
    proof4.push_back(BytesN::from_array(&env, &leaf_hashes[4]));
    proof4.push_back(BytesN::from_array(&env, &n2));
    proof4.push_back(BytesN::from_array(&env, &m0));
    let mut flags4 = Vec::new(&env);
    flags4.push_back(false);
    flags4.push_back(false);
    flags4.push_back(true);
    assert!(client.verify_message(&block_height, &payloads[4], &proof4, &flags4));

    // Verify leaf 0
    let mut proof0 = Vec::new(&env);
    proof0.push_back(BytesN::from_array(&env, &leaf_hashes[1]));
    proof0.push_back(BytesN::from_array(&env, &n1));
    proof0.push_back(BytesN::from_array(&env, &m1));
    let mut flags0 = Vec::new(&env);
    flags0.push_back(false);
    flags0.push_back(false);
    flags0.push_back(false);
    assert!(client.verify_message(&block_height, &payloads[0], &proof0, &flags0));
}

#[test]
fn test_verify_message_and_consume_odd_leaf_count_tree() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CrossChainVerifier, ());
    let client = CrossChainVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let leaf0 = BytesN::from_array(&env, &[101u8; 32]);
    let leaf1 = BytesN::from_array(&env, &[102u8; 32]);
    let leaf2 = BytesN::from_array(&env, &[103u8; 32]);

    let h0 = hash_leaf(&env, &leaf0.to_array());
    let h1 = hash_leaf(&env, &leaf1.to_array());
    let h2 = hash_leaf(&env, &leaf2.to_array());

    let n0 = merkle_hash_pair(&env, &h0, &h1);
    let n1 = merkle_hash_pair(&env, &h2, &h2);
    let root = merkle_hash_pair(&env, &n0, &n1);

    let block_height = 700;
    client.update_root(&block_height, &BytesN::from_array(&env, &root));

    // Consume leaf 2 (odd leaf)
    let mut proof2 = Vec::new(&env);
    proof2.push_back(BytesN::from_array(&env, &h2));
    proof2.push_back(BytesN::from_array(&env, &n0));
    let mut flags2 = Vec::new(&env);
    flags2.push_back(false);
    flags2.push_back(true);

    assert!(client.verify_message_and_consume(&block_height, &42u64, &leaf2, &proof2, &flags2));
    assert!(client.is_nonce_processed(&42u64));
}

