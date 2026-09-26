use crate::contract::{DIDRegistry, DIDRegistryClient};
use crate::storage_types::{Attestation, Claim, DIDDocument, Service, VerificationMethod};
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String, Vec};

#[test]
fn test_register_and_update_did() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:123");
    let mut document = DIDDocument {
        context: Vec::from_array(
            &env,
            [String::from_str(&env, "https://www.w3.org/ns/did/v1")],
        ),
        id: did.clone(),
        verification_method: Vec::new(&env),
        authentication: Vec::new(&env),
        assertion_method: Vec::new(&env),
        key_agreement: Vec::new(&env),
        capability_invocation: Vec::new(&env),
        capability_delegation: Vec::new(&env),
        service: Vec::new(&env),
    };

    client.register_did(&did, &document, &None);

    let retrieved = client.get_did_document(&did);
    assert_eq!(retrieved.id, did);

    // Update document
    document
        .context
        .push_back(String::from_str(&env, "https://example.com/context"));
    client.update_did_document(&did, &document);

    let updated = client.get_did_document(&did);
    assert_eq!(updated.context.len(), 2);
}

#[test]
fn test_add_verification_method() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:123");
    let document = DIDDocument {
        context: Vec::from_array(
            &env,
            [String::from_str(&env, "https://www.w3.org/ns/did/v1")],
        ),
        id: did.clone(),
        verification_method: Vec::new(&env),
        authentication: Vec::new(&env),
        assertion_method: Vec::new(&env),
        key_agreement: Vec::new(&env),
        capability_invocation: Vec::new(&env),
        capability_delegation: Vec::new(&env),
        service: Vec::new(&env),
    };

    client.register_did(&did, &document, &None);

    let method = VerificationMethod {
        id: String::from_str(&env, "key-1"),
        type_: String::from_str(&env, "Ed25519VerificationKey2020"),
        controller: owner.clone(),
        public_key_multibase: Bytes::from_array(&env, &[1, 2, 3]),
    };

    client.add_verification_method(&did, &method);

    let updated_doc = client.get_did_document(&did);
    assert_eq!(updated_doc.verification_method.len(), 1);
    assert_eq!(
        updated_doc.verification_method.get(0).unwrap().id,
        method.id
    );
}

#[test]
fn test_add_claim() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    let subject = Address::generate(&env);
    let claim = Claim {
        key: String::from_str(&env, "name"),
        value: String::from_str(&env, "Alice"),
        issuer: owner.clone(),
        subject: subject.clone(),
    };

    client.add_claim(&claim);

    let claims = client.get_claims(&subject);
    assert_eq!(claims.len(), 1);
    assert_eq!(claims.get(0).unwrap().value, claim.value);
}

#[test]
fn test_did_with_expiration() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:456");
    let document = DIDDocument {
        context: Vec::from_array(
            &env,
            [String::from_str(&env, "https://www.w3.org/ns/did/v1")],
        ),
        id: did.clone(),
        verification_method: Vec::new(&env),
        authentication: Vec::new(&env),
        assertion_method: Vec::new(&env),
        key_agreement: Vec::new(&env),
        capability_invocation: Vec::new(&env),
        capability_delegation: Vec::new(&env),
        service: Vec::new(&env),
    };

    let expiration = Some(1000u64);
    client.register_did(&did, &document, &expiration);

    let is_valid = client.is_did_valid(&did);
    assert!(is_valid);

    let retrieved = client.get_did_document(&did);
    assert_eq!(retrieved.id, did);
}

#[test]
fn test_did_expiration_passed() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:789");
    let document = DIDDocument {
        context: Vec::from_array(
            &env,
            [String::from_str(&env, "https://www.w3.org/ns/did/v1")],
        ),
        id: did.clone(),
        verification_method: Vec::new(&env),
        authentication: Vec::new(&env),
        assertion_method: Vec::new(&env),
        key_agreement: Vec::new(&env),
        capability_invocation: Vec::new(&env),
        capability_delegation: Vec::new(&env),
        service: Vec::new(&env),
    };

    let expiration = Some(100u64);
    client.register_did(&did, &document, &expiration);

    env.ledger().set_timestamp(200);

    let is_valid = client.is_did_valid(&did);
    assert!(!is_valid);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.get_did_document(&did);
    }));
    assert!(result.is_err());
}

#[test]
fn test_revoke_did() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:999");
    let document = DIDDocument {
        context: Vec::from_array(
            &env,
            [String::from_str(&env, "https://www.w3.org/ns/did/v1")],
        ),
        id: did.clone(),
        verification_method: Vec::new(&env),
        authentication: Vec::new(&env),
        assertion_method: Vec::new(&env),
        key_agreement: Vec::new(&env),
        capability_invocation: Vec::new(&env),
        capability_delegation: Vec::new(&env),
        service: Vec::new(&env),
    };

    client.register_did(&did, &document, &None);

    let is_valid = client.is_did_valid(&did);
    assert!(is_valid);

    client.revoke_did(&did);

    let is_valid_after = client.is_did_valid(&did);
    assert!(!is_valid_after);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.get_did_document(&did);
    }));
    assert!(result.is_err());
}

#[test]
fn test_set_expiration() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:111");
    let document = DIDDocument {
        context: Vec::from_array(
            &env,
            [String::from_str(&env, "https://www.w3.org/ns/did/v1")],
        ),
        id: did.clone(),
        verification_method: Vec::new(&env),
        authentication: Vec::new(&env),
        assertion_method: Vec::new(&env),
        key_agreement: Vec::new(&env),
        capability_invocation: Vec::new(&env),
        capability_delegation: Vec::new(&env),
        service: Vec::new(&env),
    };

    client.register_did(&did, &document, &None);

    let new_expiration = Some(5000u64);
    client.set_expiration(&did, &new_expiration);

    let is_valid = client.is_did_valid(&did);
    assert!(is_valid);

    env.ledger().set_timestamp(6000);

    let is_valid_after = client.is_did_valid(&did);
    assert!(!is_valid_after);
}

#[test]
fn test_did_without_expiration_remains_valid() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:222");
    let document = DIDDocument {
        context: Vec::from_array(
            &env,
            [String::from_str(&env, "https://www.w3.org/ns/did/v1")],
        ),
        id: did.clone(),
        verification_method: Vec::new(&env),
        authentication: Vec::new(&env),
        assertion_method: Vec::new(&env),
        key_agreement: Vec::new(&env),
        capability_invocation: Vec::new(&env),
        capability_delegation: Vec::new(&env),
        service: Vec::new(&env),
    };

    client.register_did(&did, &document, &None);

    env.ledger().set_timestamp(999999);

    let is_valid = client.is_did_valid(&did);
    assert!(is_valid);

    let retrieved = client.get_did_document(&did);
    assert_eq!(retrieved.id, did);
}

#[test]
fn test_rotate_and_remove_verification_method() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:soroban:test1234");
    let document = DIDDocument {
        context: Vec::from_array(
            &env,
            [String::from_str(&env, "https://www.w3.org/ns/did/v1")],
        ),
        id: did.clone(),
        verification_method: Vec::new(&env),
        authentication: Vec::new(&env),
        assertion_method: Vec::new(&env),
        key_agreement: Vec::new(&env),
        capability_invocation: Vec::new(&env),
        capability_delegation: Vec::new(&env),
        service: Vec::new(&env),
    };

    client.register_did(&did, &document, &None);

    let method_id = String::from_str(&env, "key-1");
    let method = VerificationMethod {
        id: method_id.clone(),
        type_: String::from_str(&env, "Ed25519VerificationKey2020"),
        controller: owner.clone(),
        public_key_multibase: Bytes::from_array(&env, &[1, 2, 3]),
    };

    client.add_verification_method(&did, &method);

    // Rotate key
    let new_key = Bytes::from_array(&env, &[9, 8, 7]);
    client.rotate_verification_method(&did, &method_id, &new_key);

    let doc_after_rotation = client.get_did_document(&did);
    assert_eq!(
        doc_after_rotation
            .verification_method
            .get(0)
            .unwrap()
            .public_key_multibase,
        new_key
    );

    // Remove key
    client.remove_verification_method(&did, &method_id);
    let doc_after_removal = client.get_did_document(&did);
    assert_eq!(doc_after_removal.verification_method.len(), 0);
}

#[test]
#[should_panic(expected = "invalid DID URI format")]
fn test_invalid_did_uri() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    let invalid_did = String::from_str(&env, "http://invalid-uri");
    let document = DIDDocument {
        context: Vec::new(&env),
        id: invalid_did.clone(),
        verification_method: Vec::new(&env),
        authentication: Vec::new(&env),
        assertion_method: Vec::new(&env),
        key_agreement: Vec::new(&env),
        capability_invocation: Vec::new(&env),
        capability_delegation: Vec::new(&env),
        service: Vec::new(&env),
    };

    client.register_did(&invalid_did, &document, &None);
}

// ── Per-DID ownership (issue #79) ────────────────────────────────────────────

fn sample_document(env: &Env, did: &String) -> DIDDocument {
    DIDDocument {
        context: Vec::from_array(env, [String::from_str(env, "https://www.w3.org/ns/did/v1")]),
        id: did.clone(),
        verification_method: Vec::new(env),
        authentication: Vec::new(env),
        assertion_method: Vec::new(env),
        key_agreement: Vec::new(env),
        capability_invocation: Vec::new(env),
        capability_delegation: Vec::new(env),
        service: Vec::new(env),
    }
}

#[test]
fn registering_a_did_records_the_registry_owner_as_its_owner() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:owner");
    client.register_did(&did, &sample_document(&env, &did), &None);

    assert_eq!(client.get_did_owner(&did), Some(owner));
}

#[test]
fn ownership_can_be_transferred_and_then_controls_updates() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:transfer");
    client.register_did(&did, &sample_document(&env, &did), &None);

    client.transfer_did_ownership(&did, &new_owner);
    assert_eq!(client.get_did_owner(&did), Some(new_owner.clone()));

    // The new owner can still mutate the document.
    let mut doc = sample_document(&env, &did);
    doc.context
        .push_back(String::from_str(&env, "https://example.com/ctx"));
    client.update_did_document(&did, &doc);
    assert_eq!(client.get_did_document(&did).context.len(), 2);
}

#[test]
fn a_caller_without_the_did_owner_authorization_cannot_update() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;

    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:guarded");
    client.register_did(&did, &sample_document(&env, &did), &None);
    client.transfer_did_ownership(&did, &new_owner);

    // An attacker authorizes the call, but neither the registry owner nor the
    // DID owner does, so the DID-owner gate must reject it.
    let attacker = Address::generate(&env);
    let mut doc = sample_document(&env, &did);
    doc.context
        .push_back(String::from_str(&env, "https://evil.example"));

    let res = client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "update_did_document",
                args: (did.clone(), doc.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_update_did_document(&did, &doc);

    assert!(res.is_err());
    // The document is untouched.
    assert_eq!(client.get_did_document(&did).context.len(), 1);
}

#[test]
fn revoking_requires_the_did_owner_too() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;

    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:revoke-guard");
    client.register_did(&did, &sample_document(&env, &did), &None);
    client.transfer_did_ownership(&did, &new_owner);

    let attacker = Address::generate(&env);
    let res = client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "revoke_did",
                args: (did.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_revoke_did(&did);

    assert!(res.is_err());
    // Still valid: the revoke was rejected.
    assert!(client.is_did_valid(&did));
}
