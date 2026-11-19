//! Tests for the statement verifier.
//!
//! Signatures are produced with `ed25519-dalek` over the same raw challenge
//! bytes the host verifies, so these are real cryptographic checks rather than
//! mocks — the point of issue #78 was to stop shipping a verifier that passes
//! everything.

#![cfg(test)]

extern crate std;

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::testutils::Address as _;

const SEED: [u8; 32] = [
    0x5a, 0xcc, 0x72, 0x53, 0x29, 0x5d, 0xfc, 0x35, 0x6c, 0x04, 0x62, 0x97, 0x92, 0x5a, 0x36, 0x9f,
    0x3d, 0x27, 0x62, 0xd0, 0x0a, 0xfd, 0xaf, 0x25, 0x83, 0xec, 0xbe, 0x92, 0x18, 0x0b, 0x07, 0xc3,
    0x7d,
];

fn bytes32(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn signer() -> SigningKey {
    SigningKey::from_bytes(&SEED)
}

/// Sign the challenge the contract will derive for this statement.
fn sign_challenge(
    env: &Env,
    key: &SigningKey,
    vk_hash: &BytesN<32>,
    input_hash: &BytesN<32>,
) -> Bytes {
    let challenge = statement_challenge(env, vk_hash, input_hash);
    let sig = key.sign(&challenge.to_array());
    Bytes::from_slice(env, &sig.to_bytes())
}

/// (env, contract id, admin, signing key, public key, vk hash, input hash)
#[allow(clippy::type_complexity)]
fn setup() -> (
    Env,
    Address,
    Address,
    SigningKey,
    BytesN<32>,
    BytesN<32>,
    BytesN<32>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(StatementVerifier, ());
    let client = StatementVerifierClient::new(&env, &contract_id);
    client.initialize(&admin);

    let key = signer();
    let public_key = BytesN::from_array(&env, &key.verifying_key().to_bytes());
    let vk_hash = bytes32(&env, 0xAB);
    let input_hash = bytes32(&env, 0xCD);

    client.register_verification_key(&vk_hash, &public_key);

    (
        env,
        contract_id,
        admin,
        key,
        public_key,
        vk_hash,
        input_hash,
    )
}

#[test]
fn accepts_a_valid_statement_bound_proof() {
    let (env, contract_id, _, key, _public_key, vk_hash, input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    let proof = sign_challenge(&env, &key, &vk_hash, &input_hash);

    assert_eq!(
        client.verify_statement(&vk_hash, &input_hash, &proof),
        Ok(())
    );
    // The drop-in bool interface agrees.
    assert!(client.verify(&vk_hash, &input_hash, &proof));
}

#[test]
fn challenge_is_domain_separated_and_statement_bound() {
    let (env, contract_id, _, _key, _public_key, vk_hash, input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    let challenge = client.challenge(&vk_hash, &input_hash);

    // Matches the documented transcript exactly.
    let mut preimage = std::vec::Vec::new();
    preimage.extend_from_slice(b"skymoonscope-zk");
    preimage.extend_from_slice(b"statement-proof-v1");
    preimage.extend_from_slice(&vk_hash.to_array());
    preimage.extend_from_slice(&input_hash.to_array());
    let expected: BytesN<32> = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &preimage))
        .into();
    assert_eq!(challenge, expected);

    // A different public input yields a different challenge, so a proof for one
    // statement cannot be replayed against another.
    let other = client.challenge(&vk_hash, &bytes32(&env, 0xEE));
    assert_ne!(challenge, other);
}

#[test]
fn rejects_unregistered_verification_key() {
    let (env, contract_id, _, key, _public_key, vk_hash, input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    let unknown = bytes32(&env, 0x99);
    let proof = sign_challenge(&env, &key, &vk_hash, &input_hash);

    assert_eq!(
        client.try_verify_statement(&unknown, &input_hash, &proof),
        Err(Ok(Error::UnknownVerificationKey))
    );
    // The bool interface fails closed rather than returning true.
    assert!(!client.verify(&unknown, &input_hash, &proof));
}

#[test]
fn rejects_malformed_proof_length() {
    let (env, contract_id, _, _key, _public_key, vk_hash, input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    let short = Bytes::from_slice(&env, &[1u8, 2, 3]);
    assert_eq!(
        client.try_verify_statement(&vk_hash, &input_hash, &short),
        Err(Ok(Error::MalformedProof))
    );
    assert!(!client.verify(&vk_hash, &input_hash, &short));

    let mut long_bytes = [7u8; 65];
    long_bytes[0] = 1;
    let long = Bytes::from_slice(&env, &long_bytes);
    assert_eq!(
        client.try_verify_statement(&vk_hash, &input_hash, &long),
        Err(Ok(Error::MalformedProof))
    );
    assert!(!client.verify(&vk_hash, &input_hash, &long));

    let empty = Bytes::new(&env);
    assert_eq!(
        client.try_verify_statement(&vk_hash, &input_hash, &empty),
        Err(Ok(Error::MalformedProof))
    );
}

#[test]
fn rejects_all_zero_signature_without_invoking_the_host() {
    let (env, contract_id, _, _key, _public_key, vk_hash, input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    let zero_sig = Bytes::from_slice(&env, &[0u8; 64]);

    assert_eq!(
        client.try_verify_statement(&vk_hash, &input_hash, &zero_sig),
        Err(Ok(Error::InvalidProof))
    );
    assert!(!client.verify(&vk_hash, &input_hash, &zero_sig));
}

#[test]
fn a_proof_for_another_statement_does_not_verify() {
    let (env, contract_id, _, key, _public_key, vk_hash, input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    // Sign the challenge for a *different* public input.
    let other_input = bytes32(&env, 0xEE);
    let proof = sign_challenge(&env, &key, &vk_hash, &other_input);

    // The host rejects it, which reverts the call: verification fails.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = client.verify_statement(&vk_hash, &input_hash, &proof);
    }));
    assert!(result.is_err(), "a cross-statement proof must not verify");
}

#[test]
fn a_proof_from_a_different_key_does_not_verify() {
    let (env, contract_id, _, _key, _public_key, vk_hash, input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    let other_key = SigningKey::from_bytes(&[9u8; 32]);
    let proof = sign_challenge(&env, &other_key, &vk_hash, &input_hash);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = client.verify_statement(&vk_hash, &input_hash, &proof);
    }));
    assert!(result.is_err(), "a foreign-key proof must not verify");
}

#[test]
fn removed_key_stops_verifying() {
    let (env, contract_id, _, key, _public_key, vk_hash, input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    let proof = sign_challenge(&env, &key, &vk_hash, &input_hash);
    assert!(client.verify(&vk_hash, &input_hash, &proof));

    client.remove_verification_key(&vk_hash);

    assert_eq!(
        client.try_verify_statement(&vk_hash, &input_hash, &proof),
        Err(Ok(Error::UnknownVerificationKey))
    );
    assert!(!client.verify(&vk_hash, &input_hash, &proof));
}

#[test]
fn tracks_registered_keys_in_order() {
    let (env, contract_id, _admin, _key, public_key, vk_hash, _input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    let second = bytes32(&env, 0x11);
    let second_key = SigningKey::from_bytes(&[3u8; 32]);
    let second_pub = BytesN::from_array(&env, &second_key.verifying_key().to_bytes());
    client.register_verification_key(&second, &second_pub);

    let keys = client.list_verification_keys();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys.get(0).unwrap(), vk_hash);
    assert_eq!(keys.get(1).unwrap(), second);
    assert_eq!(client.get_public_key(&vk_hash), Some(public_key));
}

#[test]
fn refuses_to_rebind_a_hash_to_a_different_key() {
    let (env, contract_id, _admin, _key, public_key, vk_hash, _input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    let other_pub = BytesN::from_array(&env, &[5u8; 32]);

    assert_eq!(
        client.try_register_verification_key(&vk_hash, &other_pub),
        Err(Ok(Error::InvalidPublicKey))
    );
    // The original binding is untouched.
    assert_eq!(client.get_public_key(&vk_hash), Some(public_key));
}

#[test]
fn refuses_a_zero_public_key() {
    let (env, contract_id, _admin, _key, _public_key, _vk_hash, _input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    let zero_pub = BytesN::<32>::from_array(&env, &[0u8; 32]);
    let fresh = bytes32(&env, 0x22);

    assert_eq!(
        client.try_register_verification_key(&fresh, &zero_pub),
        Err(Ok(Error::InvalidPublicKey))
    );
}

#[test]
fn cannot_be_initialized_twice() {
    let (env, contract_id, _admin, _key, _public_key, _vk_hash, _input_hash) = setup();
    let client = StatementVerifierClient::new(&env, &contract_id);

    let other_admin = Address::generate(&env);
    assert_eq!(
        client.try_initialize(&other_admin),
        Err(Ok(Error::AlreadyInitialized))
    );
}

#[test]
fn key_registration_requires_admin_authorization() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;

    let env = Env::default();

    let admin = Address::generate(&env);
    let contract_id = env.register(StatementVerifier, ());
    let client = StatementVerifierClient::new(&env, &contract_id);
    client.initialize(&admin);

    let vk_hash = bytes32(&env, 0x42);
    let public_key = BytesN::from_array(&env, &[4u8; 32]);

    // Only a non-admin address authorizes the call, so the admin's
    // `require_auth()` finds no matching authorization.
    let attacker = Address::generate(&env);
    let res = client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "register_verification_key",
                args: (vk_hash.clone(), public_key.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_register_verification_key(&vk_hash, &public_key);

    assert!(res.is_err());
    // Nothing was registered.
    assert_eq!(client.get_public_key(&vk_hash), None);
}
