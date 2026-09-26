use crate::storage_types::{
    Attestation, Claim, DIDDocument, DIDMetadata, DIDUpdated, Error, Service, VerificationMethod,
    ATTESTATIONS, CLAIMS, DID_DOCUMENT, DID_INDEX, DID_METADATA, DID_OWNER, OWNER,
};
use soroban_sdk::{contract, contractimpl, Address, Bytes, Env, String, Symbol, Vec};

pub trait DIDRegistryTrait {
    fn initialize(e: Env, owner: Address);

    fn register_did(
        e: Env,
        did: String,
        document: DIDDocument,
        expiration_timestamp: Option<u64>,
    ) -> Result<(), Error>;

    fn revoke_did(e: Env, did: String) -> Result<(), Error>;

    fn set_expiration(e: Env, did: String, expiration_timestamp: Option<u64>) -> Result<(), Error>;

    fn is_did_valid(e: Env, did: String) -> bool;

    fn update_did_document(e: Env, did: String, document: DIDDocument) -> Result<(), Error>;

    fn transfer_did_ownership(e: Env, did: String, new_owner: Address) -> Result<(), Error>;

    fn get_did_owner(e: Env, did: String) -> Option<Address>;

    fn add_verification_method(
        e: Env,
        did: String,
        method: VerificationMethod,
    ) -> Result<(), Error>;

    fn remove_verification_method(e: Env, did: String, method_id: String) -> Result<(), Error>;

    fn rotate_verification_method(
        e: Env,
        did: String,
        method_id: String,
        new_public_key_multibase: Bytes,
    ) -> Result<(), Error>;

    fn add_service(e: Env, did: String, service: Service) -> Result<(), Error>;

    fn remove_service(e: Env, did: String, service_id: String) -> Result<(), Error>;

    fn add_claim(e: Env, claim: Claim) -> Result<(), Error>;

    fn attest_claim(e: Env, attestation: Attestation) -> Result<(), Error>;

    fn get_did_document(e: Env, did: String) -> DIDDocument;

    fn get_claims(e: Env, subject: Address) -> Vec<Claim>;

    fn get_attestations(e: Env, claim_hash: Bytes) -> Vec<Attestation>;

    fn verify_attestation(e: Env, attestation: Attestation) -> bool;
}

#[contract]
pub struct DIDRegistry;

impl DIDRegistry {
    fn owner(e: &Env) -> Address {
        e.storage().instance().get(&OWNER).unwrap()
    }

    fn require_owner_auth(e: &Env) {
        let owner = Self::owner(e);
        owner.require_auth();
    }

    /// The address allowed to mutate `did`'s document.
    ///
    /// Recorded at registration time and changeable only through
    /// `transfer_did_ownership`, which is itself registry-owner gated.
    fn did_owner(e: &Env, did: &String) -> Result<Address, Error> {
        e.storage()
            .persistent()
            .get(&(DID_OWNER, did.clone()))
            .ok_or(Error::DIDOwnerNotFound)
    }

    /// Gate every state-mutating operation on a DID.
    ///
    /// Issue #79: these operations previously required only the global registry
    /// owner, so a DID document had no owner of its own. Both the registry owner
    /// and the DID owner must now authorize, which lets ownership be delegated
    /// per DID without exposing the document to arbitrary callers.
    ///
    /// The workspace pins `soroban-sdk` 22, which does not expose
    /// `Env::invoker()`, so the caller check is expressed as `require_auth()`:
    /// the transaction must carry that address's authorization for this
    /// contract, this function and these arguments.
    fn require_did_owner_auth(e: &Env, did: &String) -> Result<(), Error> {
        Self::require_owner_auth(e);
        let owner = Self::did_owner(e, did)?;
        owner.require_auth();
        Ok(())
    }

    fn validate_did_uri(e: &Env, did: &String) {
        if did.len() < 4 {
            panic!("invalid DID URI format");
        }
        let mut buf = [0u8; 4];
        did.copy_into_slice(&mut buf[..4]);
        if &buf != b"did:" {
            panic!("invalid DID URI format");
        }
    }

    fn emit_did_updated(e: &Env, did: &String, action: &str) {
        e.events().publish(
            (Symbol::new(e, "did_updated"), did.clone()),
            DIDUpdated {
                did: did.clone(),
                action: String::from_str(e, action),
                timestamp: e.ledger().timestamp(),
            },
        );
    }

    fn append_did_index(e: &Env, did: &String) {
        let mut dids: Vec<String> = e
            .storage()
            .persistent()
            .get(&DID_INDEX)
            .unwrap_or(Vec::new(&e));
        let mut i = 0;
        while i < dids.len() {
            if dids.get(i).unwrap() == *did {
                return;
            }
            i += 1;
        }
        dids.push_back(did.clone());
        e.storage().persistent().set(&DID_INDEX, &dids);
    }

    fn attester_is_authorized(e: &Env, attester: &Address) -> bool {
        let owner = Self::owner(e);
        if attester == &owner {
            return true;
        }

        let dids: Vec<String> = e
            .storage()
            .persistent()
            .get(&DID_INDEX)
            .unwrap_or(Vec::new(&e));
        let mut i = 0;
        while i < dids.len() {
            let did = dids.get(i).unwrap();
            let key = (DID_DOCUMENT, did.clone());
            let document: DIDDocument = e.storage().persistent().get(&key).unwrap();
            let mut j = 0;
            while j < document.verification_method.len() {
                if document.verification_method.get(j).unwrap().controller == *attester {
                    return true;
                }
                j += 1;
            }
            i += 1;
        }
        false
    }

    fn _is_did_valid(e: &Env, did: String) -> bool {
        let key = (DID_DOCUMENT, did.clone());
        if !e.storage().persistent().has(&key) {
            return false;
        }

        let metadata_key = (DID_METADATA, did.clone());
        let metadata: DIDMetadata = e.storage().persistent().get(&metadata_key).unwrap();

        if metadata.revocation_bitmap != 0 {
            return false;
        }

        if let Some(expiration) = metadata.expiration_timestamp {
            let current_ledger_time = e.ledger().timestamp();
            if current_ledger_time >= expiration {
                return false;
            }
        }

        true
    }
}

#[contractimpl]
impl DIDRegistryTrait for DIDRegistry {
    fn initialize(e: Env, owner: Address) {
        if e.storage().instance().has(&OWNER) {
            panic!("already initialized");
        }
        e.storage().instance().set(&OWNER, &owner);
        e.storage().persistent().set(&DID_INDEX, &Vec::new(&e));
    }

    fn register_did(
        e: Env,
        did: String,
        document: DIDDocument,
        expiration_timestamp: Option<u64>,
    ) -> Result<(), Error> {
        Self::require_owner_auth(&e);
        Self::validate_did_uri(&e, &did);

        let key = (DID_DOCUMENT, did.clone());
        if e.storage().persistent().has(&key) {
            panic!("DID already registered");
        }

        // The registry owner is the initial owner of every DID it registers.
        let registry_owner = Self::owner(&e);
        e.storage()
            .persistent()
            .set(&(DID_OWNER, did.clone()), &registry_owner);

        let metadata = DIDMetadata {
            expiration_timestamp,
            revocation_bitmap: 0,
        };
        let metadata_key = (DID_METADATA, did.clone());

        e.storage().persistent().set(&key, &document);
        e.storage().persistent().set(&metadata_key, &metadata);
        Self::append_did_index(&e, &did);
        Self::emit_did_updated(&e, &did, "register");
        Ok(())
    }

    fn revoke_did(e: Env, did: String) -> Result<(), Error> {
        Self::require_did_owner_auth(&e, &did)?;

        let key = (DID_DOCUMENT, did.clone());
        if !e.storage().persistent().has(&key) {
            panic!("DID not found");
        }

        let metadata_key = (DID_METADATA, did.clone());
        let mut metadata: DIDMetadata = e.storage().persistent().get(&metadata_key).unwrap();
        metadata.revocation_bitmap = 1;
        e.storage().persistent().set(&metadata_key, &metadata);
        Self::emit_did_updated(&e, &did, "revoke");
        Ok(())
    }

    fn set_expiration(e: Env, did: String, expiration_timestamp: Option<u64>) -> Result<(), Error> {
        Self::require_did_owner_auth(&e, &did)?;

        let key = (DID_DOCUMENT, did.clone());
        if !e.storage().persistent().has(&key) {
            panic!("DID not found");
        }

        let metadata_key = (DID_METADATA, did.clone());
        let mut metadata: DIDMetadata = e.storage().persistent().get(&metadata_key).unwrap();
        metadata.expiration_timestamp = expiration_timestamp;
        e.storage().persistent().set(&metadata_key, &metadata);
        Self::emit_did_updated(&e, &did, "set_expiration");
        Ok(())
    }

    fn is_did_valid(e: Env, did: String) -> bool {
        Self::_is_did_valid(&e, did)
    }

    fn update_did_document(e: Env, did: String, document: DIDDocument) -> Result<(), Error> {
        Self::require_did_owner_auth(&e, &did)?;

        let key = (DID_DOCUMENT, did.clone());
        if !e.storage().persistent().has(&key) {
            panic!("DID not found");
        }

        e.storage().persistent().set(&key, &document);
        Self::emit_did_updated(&e, &did, "update");
        Ok(())
    }

    /// Hand a DID to a new owner. Registry-owner gated.
    fn transfer_did_ownership(e: Env, did: String, new_owner: Address) -> Result<(), Error> {
        Self::require_owner_auth(&e);

        let key = (DID_DOCUMENT, did.clone());
        if !e.storage().persistent().has(&key) {
            panic!("DID not found");
        }

        e.storage()
            .persistent()
            .set(&(DID_OWNER, did.clone()), &new_owner);
        Self::emit_did_updated(&e, &did, "transfer_ownership");
        Ok(())
    }

    /// The address currently allowed to mutate `did`'s document.
    fn get_did_owner(e: Env, did: String) -> Option<Address> {
        e.storage().persistent().get(&(DID_OWNER, did))
    }

    fn add_verification_method(
        e: Env,
        did: String,
        method: VerificationMethod,
    ) -> Result<(), Error> {
        Self::require_did_owner_auth(&e, &did)?;

        let key = (DID_DOCUMENT, did.clone());
        let mut document: DIDDocument = e.storage().persistent().get(&key).unwrap();
        document.verification_method.push_back(method);
        e.storage().persistent().set(&key, &document);
        Self::emit_did_updated(&e, &did, "add_verification_method");
        Ok(())
    }

    fn remove_verification_method(e: Env, did: String, method_id: String) -> Result<(), Error> {
        Self::require_did_owner_auth(&e, &did)?;

        let key = (DID_DOCUMENT, did.clone());
        let mut document: DIDDocument = e.storage().persistent().get(&key).unwrap();
        let mut removed = false;
        let mut i = 0;
        while i < document.verification_method.len() {
            if document.verification_method.get(i).unwrap().id == method_id {
                document.verification_method.remove(i);
                removed = true;
                break;
            }
            i += 1;
        }

        if !removed {
            panic!("verification method not found");
        }

        e.storage().persistent().set(&key, &document);
        Self::emit_did_updated(&e, &did, "remove_verification_method");
        Ok(())
    }

    fn rotate_verification_method(
        e: Env,
        did: String,
        method_id: String,
        new_public_key_multibase: Bytes,
    ) -> Result<(), Error> {
        Self::require_did_owner_auth(&e, &did)?;

        let key = (DID_DOCUMENT, did.clone());
        let mut document: DIDDocument = e.storage().persistent().get(&key).unwrap();
        let mut rotated = false;
        let mut i = 0;
        while i < document.verification_method.len() {
            let mut method = document.verification_method.get(i).unwrap().clone();
            if method.id == method_id {
                method.public_key_multibase = new_public_key_multibase.clone();
                document.verification_method.remove(i);
                document.verification_method.insert(i, method);
                rotated = true;
                break;
            }
            i += 1;
        }

        if !rotated {
            panic!("verification method not found");
        }

        e.storage().persistent().set(&key, &document);
        Self::emit_did_updated(&e, &did, "rotate_verification_method");
        Ok(())
    }

    fn add_service(e: Env, did: String, service: Service) -> Result<(), Error> {
        Self::require_did_owner_auth(&e, &did)?;

        let key = (DID_DOCUMENT, did.clone());
        let mut document: DIDDocument = e.storage().persistent().get(&key).unwrap();
        let mut i = 0;
        while i < document.service.len() {
            if document.service.get(i).unwrap().id == service.id {
                panic!("service already exists");
            }
            i += 1;
        }
        document.service.push_back(service);
        e.storage().persistent().set(&key, &document);
        Ok(())
    }

    fn remove_service(e: Env, did: String, service_id: String) -> Result<(), Error> {
        Self::require_did_owner_auth(&e, &did)?;

        let key = (DID_DOCUMENT, did.clone());
        let mut document: DIDDocument = e.storage().persistent().get(&key).unwrap();
        let mut removed = false;
        let mut i = 0;
        while i < document.service.len() {
            if document.service.get(i).unwrap().id == service_id {
                document.service.remove(i);
                removed = true;
                break;
            }
            i += 1;
        }

        if !removed {
            panic!("service not found");
        }

        e.storage().persistent().set(&key, &document);
        Ok(())
    }

    fn add_claim(e: Env, claim: Claim) -> Result<(), Error> {
        Self::require_owner_auth(&e);

        let key = (CLAIMS, claim.subject.clone());
        let mut claims: Vec<Claim> = e.storage().persistent().get(&key).unwrap_or(Vec::new(&e));
        claims.push_back(claim);
        e.storage().persistent().set(&key, &claims);
        Ok(())
    }

    fn attest_claim(e: Env, attestation: Attestation) -> Result<(), Error> {
        Self::require_owner_auth(&e);

        let key = (ATTESTATIONS, attestation.claim_hash.clone());
        let mut attestations: Vec<Attestation> =
            e.storage().persistent().get(&key).unwrap_or(Vec::new(&e));
        attestations.push_back(attestation);
        e.storage().persistent().set(&key, &attestations);
        Ok(())
    }

    fn get_did_document(e: Env, did: String) -> DIDDocument {
        if !Self::_is_did_valid(&e, did.clone()) {
            panic!("DID is invalid (expired or revoked)");
        }

        let key = (DID_DOCUMENT, did.clone());
        e.storage().persistent().get(&key).unwrap()
    }

    fn get_claims(e: Env, subject: Address) -> Vec<Claim> {
        let key = (CLAIMS, subject.clone());
        e.storage().persistent().get(&key).unwrap_or(Vec::new(&e))
    }

    fn get_attestations(e: Env, claim_hash: Bytes) -> Vec<Attestation> {
        let key = (ATTESTATIONS, claim_hash.clone());
        e.storage().persistent().get(&key).unwrap_or(Vec::new(&e))
    }

    fn verify_attestation(e: Env, attestation: Attestation) -> bool {
        if attestation.timestamp == 0 || attestation.signature.len() == 0 {
            return false;
        }

        let key = (ATTESTATIONS, attestation.claim_hash.clone());
        let attestations: Vec<Attestation> =
            e.storage().persistent().get(&key).unwrap_or(Vec::new(&e));
        let mut found = false;
        let mut i = 0;
        while i < attestations.len() {
            if attestations.get(i).unwrap() == attestation {
                found = true;
                break;
            }
            i += 1;
        }

        if !found {
            return false;
        }

        Self::attester_is_authorized(&e, &attestation.attester)
    }
}
