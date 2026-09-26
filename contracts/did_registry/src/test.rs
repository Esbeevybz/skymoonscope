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

// ── Per-DID ownership (issue #079) ────────────────────────────────────────────

fn empty_document(env: &Env, did: &String) -> DIDDocument {
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
fn test_registered_did_is_owned_by_registry_owner() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:owned");
    let document = empty_document(&env, &did);
    client.register_did(&did, &document, &None);

    assert_eq!(client.get_did_owner(&did), Some(owner));
}

#[test]
fn test_unregistered_did_has_no_owner() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:missing");
    assert_eq!(client.get_did_owner(&did), None);
}

#[test]
fn test_document_cannot_be_rewritten_without_authorization() {
    let env = Env::default();
    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let did = String::from_str(&env, "did:example:guarded");
    let document = empty_document(&env, &did);

    // Register while the owner authorizes, then withdraw all authorization so
    // any later call fails the `require_auth` on the DID's owner.
    env.mock_all_auths();
    client.initialize(&owner);
    client.register_did(&did, &document, &None);

    env.set_auths(&[]);

    let mut tampered = document.clone();
    tampered
        .context
        .push_back(String::from_str(&env, "https://evil.example/context"));
    assert!(client.try_update_did_document(&did, &tampered).is_err());

    let method = VerificationMethod {
        id: String::from_str(&env, "key-evil"),
        type_: String::from_str(&env, "Ed25519VerificationKey2020"),
        controller: Address::generate(&env),
        public_key_multibase: Bytes::from_array(&env, &[9, 9, 9]),
    };
    assert!(client.try_add_verification_method(&did, &method).is_err());
    assert!(client
        .try_add_service(
            &did,
            &Service {
                id: String::from_str(&env, "svc-evil"),
                type_: String::from_str(&env, "LinkedDomains"),
                service_endpoint: String::from_str(&env, "https://evil.example"),
            },
        )
        .is_err());
    assert!(client.try_revoke_did(&did).is_err());

    // The document on chain is untouched.
    let stored = client.get_did_document(&did);
    assert_eq!(stored.context.len(), 1);
    assert_eq!(stored.verification_method.len(), 0);
    assert_eq!(stored.service.len(), 0);
}

#[test]
fn test_transfer_did_ownership_moves_control() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:handoff");
    let document = empty_document(&env, &did);
    client.register_did(&did, &document, &None);

    client.transfer_did_ownership(&did, &new_owner);
    assert_eq!(client.get_did_owner(&did), Some(new_owner.clone()));

    // Still mutable with authorization present, now under the new owner.
    let mut updated = document.clone();
    updated
        .context
        .push_back(String::from_str(&env, "https://new.example/context"));
    client.update_did_document(&did, &updated);
    assert_eq!(client.get_did_document(&did).context.len(), 2);
}

#[test]
fn test_mutating_missing_did_reports_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DIDRegistry, ());
    let client = DIDRegistryClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

    let did = String::from_str(&env, "did:example:absent");
    let method = VerificationMethod {
        id: String::from_str(&env, "key-1"),
        type_: String::from_str(&env, "Ed25519VerificationKey2020"),
        controller: owner.clone(),
        public_key_multibase: Bytes::from_array(&env, &[1, 2, 3]),
    };

    // Previously this panicked on an unwrap of a missing document; it now fails
    // with an explicit "DID not found".
    assert!(client.try_add_verification_method(&did, &method).is_err());
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
