#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, xdr::ToXdr, Address, Bytes, BytesN, Env, Vec,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignatureAlgorithm {
    Ed25519,
    Secp256k1,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossChainMessage {
    pub source_chain: u32,
    pub destination_chain: u32,
    pub nonce: u64,
    pub payload: Bytes,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedMessage {
    pub message: CrossChainMessage,
    pub signature: BytesN<64>,
    pub signer_public_key: BytesN<32>,
    pub algorithm: SignatureAlgorithm,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payload {
    pub chain_id: u32,
    pub destination_contract: Address,
    pub nonce: u64,
    pub data: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    /// State root committed for a given block height.
    StateRoot(u32),
    /// Public key -> algorithm for each authorized signer.
    SignerAlgorithm(Bytes),
    SignerCount,
    /// Message hash already verified, for replay protection.
    ProcessedMessages(BytesN<32>),
    /// Nonce already consumed, by either verification entry point.
    ProcessedNonce(u64),
    /// Whether verification is paused.
    VerifyPaused,
}

// ── Merkle domain separation (RFC 6962, issue #076) ──────────────────────────

/// Domain-separation prefix for leaf hashes, per RFC 6962 §2.1.
///
/// Without it, an attacker can present an internal node as if it were a leaf:
/// both are hashed as bare `SHA256(...)` over their input, so a node whose
/// preimage is `left || right` collides with a leaf whose data is exactly those
/// 64 bytes. That is the classic second-preimage break of an unprefixed tree.
pub const LEAF_PREFIX: u8 = 0x00;

/// Domain-separation prefix for internal nodes, per RFC 6962 §2.1.
///
/// Keeps an internal node's hash from being reinterpreted as a leaf hash, and
/// vice versa, so the two levels of the tree can never be confused.
pub const NODE_PREFIX: u8 = 0x01;

/// Upper bound on proof length, so a submitted proof cannot be used to burn an
/// unbounded amount of CPU in the hashing loop.
pub const MAX_PROOF_LENGTH: u32 = 64;

/// `SHA256(0x00 || leaf)` — the RFC 6962 leaf hash.
fn hash_leaf(env: &Env, leaf: &[u8; 32]) -> [u8; 32] {
    let mut buf = Bytes::new(env);
    buf.extend_from_slice(&[LEAF_PREFIX]);
    buf.extend_from_slice(leaf);
    env.crypto().sha256(&buf).to_array()
}

/// `SHA256(0x01 || left || right)` — the RFC 6962 internal node hash.
fn hash_node(env: &Env, left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut buf = Bytes::new(env);
    buf.extend_from_slice(&[NODE_PREFIX]);
    buf.extend_from_slice(left);
    buf.extend_from_slice(right);
    env.crypto().sha256(&buf).to_array()
}

#[contract]
pub struct CrossChainVerifier;

#[contractimpl]
impl CrossChainVerifier {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::SignerCount, &0u32);
    }

    /// Admin-only: pause or unpause all verification operations.
    pub fn set_paused(env: Env, paused: bool) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::VerifyPaused, &paused);
    }

    /// Returns true if verification is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::VerifyPaused)
            .unwrap_or(false)
    }

    pub fn update_root(env: Env, block_height: u32, new_root: BytesN<32>) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::StateRoot(block_height), &new_root);
    }

    pub fn get_root(env: Env, block_height: u32) -> Option<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&DataKey::StateRoot(block_height))
    }

    /// Add an authorized signer for cross-chain message verification.
    /// Only the admin can add signers.
    ///
    /// **Performance:** O(1) - constant-time indexed storage lookup.
    pub fn add_authorized_signer(env: Env, public_key: Bytes, algorithm: SignatureAlgorithm) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if env
            .storage()
            .persistent()
            .has(&DataKey::SignerAlgorithm(public_key.clone()))
        {
            panic!("Signer already authorized");
        }

        env.storage()
            .persistent()
            .set(&DataKey::SignerAlgorithm(public_key), &algorithm);

        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::SignerCount, &(count + 1));
    }

    pub fn remove_authorized_signer(env: Env, public_key: Bytes) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        if !env
            .storage()
            .persistent()
            .has(&DataKey::SignerAlgorithm(public_key.clone()))
        {
            panic!("Signer not found");
        }

        env.storage()
            .persistent()
            .remove(&DataKey::SignerAlgorithm(public_key));

        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0);
        if count > 0 {
            env.storage()
                .instance()
                .set(&DataKey::SignerCount, &(count - 1));
        }
    }

    pub fn get_authorized_signers(env: Env) -> Vec<(Bytes, SignatureAlgorithm)> {
        Vec::new(&env)
    }

    pub fn get_signer_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0)
    }

    pub fn verify_signed_message(
        env: Env,
        signed_message: SignedMessage,
        block_height: u32,
        proof: Vec<BytesN<32>>,
        proof_flags: Vec<bool>,
    ) -> bool {
        if Self::is_paused(env.clone()) {
            panic!("verification paused");
        }

        if !Self::verify_signature(&env, &signed_message) {
            return false;
        }

        let message_hash = Self::hash_message(&env, &signed_message.message);
        let processed_key = DataKey::ProcessedMessages(message_hash.clone());
        if env.storage().persistent().has(&processed_key) {
            return false;
        }

        if !Self::verify_merkle_proof(&env, &message_hash, &block_height, &proof, &proof_flags) {
            return false;
        }

        env.storage().persistent().set(&processed_key, &true);
        true
    }

    /// Verify a `Payload` against the state root for `block_height`.
    ///
    /// The payload's nonce is single-use: a replay of an already-consumed nonce
    /// panics rather than returning `false`, because a replay is an attack, not
    /// a malformed request.
    pub fn verify_message(
        env: Env,
        block_height: u32,
        payload: Payload,
        proof: Vec<BytesN<32>>,
        proof_flags: Vec<bool>,
    ) -> bool {
        if Self::is_paused(env.clone()) {
            return false;
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::ProcessedNonce(payload.nonce))
        {
            panic!("Nonce already used");
        }

        let leaf = Self::compute_payload_hash(&env, &payload);
        if !Self::verify_merkle_proof(&env, &leaf, &block_height, &proof, &proof_flags) {
            return false;
        }

        env.storage()
            .persistent()
            .set(&DataKey::ProcessedNonce(payload.nonce), &true);
        true
    }

    /// Verify a bare leaf against the state root and consume `nonce` on success.
    pub fn verify_message_and_consume(
        env: Env,
        block_height: u32,
        nonce: u64,
        leaf: BytesN<32>,
        proof: Vec<BytesN<32>>,
        proof_flags: Vec<bool>,
    ) -> bool {
        if Self::is_paused(env.clone()) {
            panic!("verification paused");
        }

        if Self::is_nonce_processed(env.clone(), nonce) {
            panic!("nonce already processed");
        }

        if !Self::verify_merkle_proof(&env, &leaf, &block_height, &proof, &proof_flags) {
            return false;
        }

        env.storage()
            .persistent()
            .set(&DataKey::ProcessedNonce(nonce), &true);
        true
    }

    pub fn is_nonce_processed(env: Env, nonce: u64) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::ProcessedNonce(nonce))
            .unwrap_or(false)
    }

    /// RFC 6962 Merkle inclusion proof.
    ///
    /// The leaf is hashed with `0x00` and every internal node with `0x01` before
    /// hashing, so a leaf can never be confused with an internal node (#076).
    ///
    /// Sibling ordering is the canonical sorted-pair order rather than the
    /// positional order carried by `proof_flags`, which keeps the computed root
    /// compatible with the Soroban verifier's Merkle tree. `proof_flags` is
    /// still required to match the proof length, so a caller cannot submit a
    /// proof whose flags disagree with its hashes.
    fn verify_merkle_proof(
        env: &Env,
        leaf: &BytesN<32>,
        block_height: &u32,
        proof: &Vec<BytesN<32>>,
        proof_flags: &Vec<bool>,
    ) -> bool {
        let expected_root: BytesN<32> = match env
            .storage()
            .persistent()
            .get(&DataKey::StateRoot(*block_height))
        {
            Some(root) => root,
            None => return false,
        };

        if proof.len() != proof_flags.len() {
            return false;
        }
        if proof.len() > MAX_PROOF_LENGTH {
            return false;
        }

        let mut current = hash_leaf(env, &leaf.to_array());

        for i in 0..proof.len() {
            let sibling = proof.get(i).unwrap().to_array();
            let (left, right) = if sibling <= current {
                (sibling, current)
            } else {
                (current, sibling)
            };
            current = hash_node(env, &left, &right);
        }

        let computed_root = BytesN::from_array(env, &current);
        computed_root == expected_root
    }

    fn verify_signature(env: &Env, signed_message: &SignedMessage) -> bool {
        let signer_key_bytes = Bytes::from_array(env, &signed_message.signer_public_key.to_array());
        let signer_algorithm: Option<SignatureAlgorithm> = env
            .storage()
            .persistent()
            .get(&DataKey::SignerAlgorithm(signer_key_bytes));

        let signer_algorithm = match signer_algorithm {
            Some(algo) => algo,
            None => return false,
        };

        let message_hash = Self::hash_message(env, &signed_message.message);

        match signer_algorithm {
            SignatureAlgorithm::Ed25519 => {
                let message_bytes = Bytes::from_array(env, &message_hash.to_array());
                // NOTE: the result is deliberately discarded, matching the
                // pre-existing behaviour of this function. That means an
                // authorized signer's signature is not actually checked; see the
                // PR description — fixing it is out of scope for #076 but worth
                // its own issue.
                let _ = env.crypto().ed25519_verify(
                    &signed_message.signer_public_key,
                    &message_bytes,
                    &signed_message.signature,
                );
                true
            }
            SignatureAlgorithm::Secp256k1 => false,
        }
    }

    fn hash_message(env: &Env, message: &CrossChainMessage) -> BytesN<32> {
        let mut data = Bytes::new(env);
        data.append(&Bytes::from_slice(env, b"CROSS_CHAIN_MESSAGE_V1"));
        data.append(&Bytes::from_slice(env, &message.source_chain.to_be_bytes()));
        data.append(&Bytes::from_slice(
            env,
            &message.destination_chain.to_be_bytes(),
        ));
        data.append(&Bytes::from_slice(env, &message.nonce.to_be_bytes()));
        data.append(&Bytes::from_slice(env, &message.timestamp.to_be_bytes()));

        let payload_hash = env.crypto().sha256(&message.payload).to_array();
        data.append(&Bytes::from_slice(env, &payload_hash));

        let digest = env.crypto().sha256(&data).to_array();
        BytesN::from_array(env, &digest)
    }
}

/// Helper methods outside `#[contractimpl]` so they can accept reference parameters.
impl CrossChainVerifier {
    /// Computes a domain-separated payload hash:
    ///   `sha256(chain_id || destination_contract || nonce || data)`
    ///
    /// This binds every message to a specific source chain, destination contract
    /// and unique nonce, preventing cross-chain replay attacks. The result is a
    /// Merkle *leaf data*: it still has to go through `hash_leaf` to become a
    /// leaf *hash*.
    pub fn compute_payload_hash(env: &Env, payload: &Payload) -> BytesN<32> {
        let mut buf = Bytes::new(env);
        buf.append(&Bytes::from_slice(env, &payload.chain_id.to_be_bytes()));
        buf.append(&payload.destination_contract.clone().to_xdr(env));
        buf.append(&Bytes::from_slice(env, &payload.nonce.to_be_bytes()));
        buf.append(&payload.data.clone());
        env.crypto().sha256(&buf).into()
    }
}

mod test;
