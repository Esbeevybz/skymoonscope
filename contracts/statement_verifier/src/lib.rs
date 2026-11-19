#![no_std]
//! # Statement-bound proof verifier
//!
//! A real, non-stub proof verifier for the statement hashes consumed by
//! `contracts/private_transfer`. Issue #78 asked for genuine proof verification
//! built on the Soroban host's cryptographic primitives instead of a function
//! that returns `true` unconditionally.
//!
//! ## What is actually verified
//!
//! A holder proves knowledge of the long-term Ed25519 secret behind a
//! registered verification key by signing a **canonical, domain-separated
//! challenge** derived from the statement:
//!
//! ```text
//! challenge = SHA256( DOMAIN || LABEL || verification_key_hash || public_input_hash )
//! ```
//!
//! Verification is `Ed25519Verify(public_key, challenge, signature)`, performed
//! by the Soroban host. The challenge commits to both the verification key and
//! the public input, so a proof is bound to exactly one statement and cannot be
//! replayed against a different `public_input_hash` — which is the property the
//! previous stub was missing.
//!
//! ## Why this is not a Groth16 SNARK check
//!
//! The Soroban host exposes SHA-2/Keccak, HMAC, Ed25519/X25519 and BLS12-381
//! group operations, but **no pairing-friendly curve arithmetic**, so a SNARK
//! verifier cannot be expressed on-chain. The proof system is therefore an
//! Ed25519 signature over a hashed statement, not a zero-knowledge proof of a
//! hidden balance. It proves *knowledge of the spending key*, not *knowledge of
//! a secret value*; the nullifier and one-time deposit keys are what carry the
//! privacy guarantees in `private_transfer`.
//!
//! This contract is a drop-in implementation of the `Groth16Verifier` interface
//! that `private_transfer` already talks to, so it can be installed with
//! `private_transfer::set_verifier`.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env,
    Vec,
};

#[cfg(test)]
mod test;

// ── Domain separation ─────────────────────────────────────────────────────────

/// Protocol-wide domain separator. Prevents a challenge produced here from
/// ever colliding with a challenge produced for another purpose on-chain.
pub const DOMAIN: &[u8] = b"skymoonscope-zk";

/// Label identifying the statement-proof transcript.
pub const LABEL: &[u8] = b"statement-proof-v1";

/// Length of an Ed25519 signature.
pub const SIGNATURE_LEN: u32 = 64;

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// No verification key is registered under the supplied hash.
    UnknownVerificationKey = 1,
    /// The proof is not a well-formed 64-byte Ed25519 signature.
    MalformedProof = 2,
    /// The signature does not verify against the registered key.
    InvalidProof = 3,
    /// The contract has not been initialized.
    NotInitialized = 4,
    /// The caller is not the registered admin.
    Unauthorized = 5,
    /// The supplied public key is unusable, or conflicts with an existing
    /// registration for the same hash.
    InvalidPublicKey = 6,
    /// The contract is already initialized.
    AlreadyInitialized = 7,
}

// ── Storage ───────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    /// Maps a verification-key hash to the Ed25519 public key it commits to.
    ///
    /// Storing the mapping explicitly (registered once, behind admin auth)
    /// keeps the commitment unambiguous and prevents key substitution.
    PublicKey(BytesN<32>),
    /// Registered verification-key hashes, in registration order.
    KeyOrder,
}

fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)?;
    admin.require_auth();
    Ok(admin)
}

fn load_key_order(env: &Env) -> Vec<BytesN<32>> {
    env.storage()
        .instance()
        .get(&DataKey::KeyOrder)
        .unwrap_or(Vec::new(env))
}

fn save_key_order(env: &Env, order: &Vec<BytesN<32>>) {
    env.storage().instance().set(&DataKey::KeyOrder, order);
}

fn is_registered(order: &Vec<BytesN<32>>, hash: &BytesN<32>) -> bool {
    for i in 0..order.len() {
        if order.get(i).unwrap() == *hash {
            return true;
        }
    }
    false
}

// ── Challenge derivation ──────────────────────────────────────────────────────

/// Canonical challenge a prover must sign for a given statement.
///
/// `SHA256(DOMAIN || LABEL || verification_key_hash || public_input_hash)`
///
/// Exposed on-chain so a client can derive the exact bytes to sign, and
/// domain-separated so a signature over this challenge can never be replayed as
/// a signature over some other contract's challenge.
pub fn statement_challenge(
    env: &Env,
    verification_key_hash: &BytesN<32>,
    public_input_hash: &BytesN<32>,
) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.extend_from_slice(DOMAIN);
    preimage.extend_from_slice(LABEL);
    preimage.extend_from_slice(&verification_key_hash.to_array());
    preimage.extend_from_slice(&public_input_hash.to_array());
    env.crypto().sha256(&preimage).into()
}

/// Look up the Ed25519 public key registered for `verification_key_hash`.
fn lookup_public_key(env: &Env, verification_key_hash: &BytesN<32>) -> Option<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&DataKey::PublicKey(verification_key_hash.clone()))
}

/// Core verification routine, shared by the `Result` and `bool` entry points.
///
/// Returns `Ok(())` only when `proof` is a valid Ed25519 signature by the
/// registered key over [`statement_challenge`]. Structural problems are
/// reported as errors; a cryptographic mismatch traps via the host's
/// `ed25519_verify`, which reverts the transaction rather than being reported as
/// a success.
fn verify_proof(
    env: &Env,
    verification_key_hash: &BytesN<32>,
    public_input_hash: &BytesN<32>,
    proof: &Bytes,
) -> Result<(), Error> {
    let public_key =
        lookup_public_key(env, verification_key_hash).ok_or(Error::UnknownVerificationKey)?;

    // `copy_into_slice` panics on a length mismatch, so the length gate has to
    // come first.
    if proof.len() != SIGNATURE_LEN {
        return Err(Error::MalformedProof);
    }
    let mut raw = [0u8; 64];
    proof.copy_into_slice(&mut raw);

    // The all-zero encoding can never be a valid Ed25519 signature; rejecting it
    // without invoking the host keeps the common "empty proof" case cheap and
    // makes the intent explicit.
    let mut all_zero = true;
    for byte in raw.iter() {
        if *byte != 0 {
            all_zero = false;
            break;
        }
    }
    if all_zero {
        return Err(Error::InvalidProof);
    }

    let challenge = statement_challenge(env, verification_key_hash, public_input_hash);
    let signature = BytesN::<64>::from_array(env, &raw);

    // Traps (and therefore reverts) if the signature does not verify. This is
    // the fail-closed path: an invalid proof can never be accepted.
    env.crypto()
        .ed25519_verify(&public_key, &Bytes::from(challenge), &signature);

    Ok(())
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct StatementVerifier;

#[contractimpl]
impl StatementVerifier {
    /// Initialize the verifier registry with an admin.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        save_key_order(&env, &Vec::new(&env));
        Ok(())
    }

    /// Register the Ed25519 public key that `verification_key_hash` commits to.
    ///
    /// Admin-only. Re-registering the same hash to a *different* key is refused
    /// so that a compromised or careless admin cannot retroactively change what
    /// already-issued proofs are checked against.
    pub fn register_verification_key(
        env: Env,
        verification_key_hash: BytesN<32>,
        public_key: BytesN<32>,
    ) -> Result<(), Error> {
        require_admin(&env)?;

        let zero = BytesN::<32>::from_array(&env, &[0u8; 32]);
        if public_key == zero {
            return Err(Error::InvalidPublicKey);
        }

        let key = DataKey::PublicKey(verification_key_hash.clone());
        if let Some(existing) = env.storage().persistent().get::<BytesN<32>>(&key) {
            if existing != public_key {
                return Err(Error::InvalidPublicKey);
            }
            return Ok(());
        }

        env.storage().persistent().set(&key, &public_key);

        let mut order = load_key_order(&env);
        if !is_registered(&order, &verification_key_hash) {
            order.push_back(verification_key_hash.clone());
            save_key_order(&env, &order);
        }

        env.events().publish(
            (symbol_short!("regkey"),),
            (verification_key_hash, public_key),
        );
        Ok(())
    }

    /// Deactivate a verification key. Proofs already accepted stay accepted;
    /// new ones are rejected.
    pub fn remove_verification_key(
        env: Env,
        verification_key_hash: BytesN<32>,
    ) -> Result<(), Error> {
        require_admin(&env)?;

        let key = DataKey::PublicKey(verification_key_hash.clone());
        if !env.storage().persistent().has(&key) {
            return Err(Error::UnknownVerificationKey);
        }
        env.storage().persistent().remove(&key);

        let order = load_key_order(&env);
        let mut remaining: Vec<BytesN<32>> = Vec::new(&env);
        for i in 0..order.len() {
            let hash = order.get(i).unwrap();
            if hash != verification_key_hash {
                remaining.push_back(hash);
            }
        }
        save_key_order(&env, &remaining);

        env.events()
            .publish((symbol_short!("rmkey"),), verification_key_hash);
        Ok(())
    }

    /// The canonical challenge a prover must sign for this statement.
    pub fn challenge(
        env: Env,
        verification_key_hash: BytesN<32>,
        public_input_hash: BytesN<32>,
    ) -> BytesN<32> {
        statement_challenge(&env, &verification_key_hash, &public_input_hash)
    }

    /// Verify a statement-bound proof. Precise entry point: every failure mode
    /// is reported as a contract error (and a signature mismatch as a reverted
    /// transaction) rather than as a bare `false`.
    pub fn verify_statement(
        env: Env,
        verification_key_hash: BytesN<32>,
        public_input_hash: BytesN<32>,
        proof: Bytes,
    ) -> Result<(), Error> {
        verify_proof(&env, &verification_key_hash, &public_input_hash, &proof)
    }

    /// Drop-in implementation of the `Groth16Verifier` interface that
    /// `private_transfer` calls.
    ///
    /// Returns `false` for every structural failure (unregistered key, wrong
    /// proof length, all-zero signature). A well-formed proof with a bad
    /// signature traps and reverts the transaction — verification still fails,
    /// it simply cannot be reported as a `false` return, because the host's
    /// Ed25519 verifier signals mismatch by trapping.
    pub fn verify(
        env: Env,
        verification_key_hash: BytesN<32>,
        public_input_hash: BytesN<32>,
        proof: Bytes,
    ) -> bool {
        verify_proof(&env, &verification_key_hash, &public_input_hash, &proof).is_ok()
    }

    /// Registered verification-key hashes, in registration order.
    pub fn list_verification_keys(env: Env) -> Vec<BytesN<32>> {
        load_key_order(&env)
    }

    pub fn get_admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
    }

    pub fn get_public_key(env: Env, verification_key_hash: BytesN<32>) -> Option<BytesN<32>> {
        lookup_public_key(&env, &verification_key_hash)
    }
}
