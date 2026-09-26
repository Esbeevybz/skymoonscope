use soroban_sdk::{contracterror, contracttype, Address, Bytes, Map, String, Symbol, Vec};

/// Errors returned by the DID registry.
///
/// The access-control failures added for issue #79 are reported as contract
/// errors so a caller can tell "you may not do this" apart from a host-level
/// failure. The pre-existing validation panics are deliberately left as they
/// were, so existing behaviour and tests are unchanged.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// The caller is neither the registry owner nor the owner of this DID.
    Unauthorized = 1,
    /// No owner is recorded for this DID.
    DIDOwnerNotFound = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationMethod {
    pub id: String,
    pub type_: String, // e.g., "Ed25519VerificationKey2020"
    pub controller: Address,
    pub public_key_multibase: Bytes, // or whatever format
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Service {
    pub id: String,
    pub type_: String,
    pub service_endpoint: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DIDDocument {
    pub context: Vec<String>, // @context
    pub id: String,
    pub verification_method: Vec<VerificationMethod>,
    pub authentication: Vec<String>, // references to verification methods
    pub assertion_method: Vec<String>,
    pub key_agreement: Vec<String>,
    pub capability_invocation: Vec<String>,
    pub capability_delegation: Vec<String>,
    pub service: Vec<Service>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Claim {
    pub key: String,
    pub value: String,
    pub issuer: Address,
    pub subject: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attestation {
    pub claim_hash: Bytes, // hash of the claim
    pub attester: Address,
    pub signature: Bytes,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DIDMetadata {
    pub expiration_timestamp: Option<u64>,
    pub revocation_bitmap: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DIDUpdated {
    pub did: String,
    pub action: String,
    pub timestamp: u64,
}

// Storage keys
pub const DID_DOCUMENT: Symbol = Symbol::short("DID_DOC");
pub const DID_METADATA: Symbol = Symbol::short("DID_META");
pub const DID_INDEX: Symbol = Symbol::short("DID_IDX");
pub const CLAIMS: Symbol = Symbol::short("CLAIMS");
pub const ATTESTATIONS: Symbol = Symbol::short("ATTEST");
pub const OWNER: Symbol = Symbol::short("OWNER");
/// Maps a DID to the address allowed to mutate its document (issue #79).
pub const DID_OWNER: Symbol = Symbol::short("DID_OWNR");
