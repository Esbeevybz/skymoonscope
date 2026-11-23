use soroban_sdk::{contracttype, BytesN, String};
use crate::chain_info::ValidatorSet;

/// Status of a cross-chain payload verification
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerificationStatus {
    /// Payload not yet verified
    Pending,
    /// Payload has been verified successfully
    Verified,
    /// Payload verification failed
    Failed,
    /// Payload expired before verification could complete
    Expired,
    /// Payload verification was cancelled
    Cancelled,
    /// Payload verification failed because the signature scheme is unrecognised
    Invalid,
    /// Payload verification encountered an unrecognised signature scheme (lenient mode only)
    Unknown,
}

/// Detailed verification result
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationResult {
    /// Overall verification status
    pub status: VerificationStatus,
    /// Signatures verified count
    pub signatures_verified: u32,
    /// Total signatures required
    pub signatures_required: u32,
    /// Error message if verification failed
    pub error_message: String,
    /// Block height at which verification occurred
    pub verified_at_height: u64,
    /// Whether payload was rejected by any validator
    pub has_rejections: bool,
    /// Number of validators who rejected the payload
    pub rejection_count: u32,
}

/// Context needed for payload verification
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationContext {
    /// Payload ID being verified
    pub payload_id: BytesN<32>,
    /// Current block height
    pub current_height: u64,
    /// Current block timestamp
    pub current_timestamp: u64,
    /// Validator set to use for verification
    pub validator_set: ValidatorSet,
    /// Hash of the payload to verify
    pub payload_hash: BytesN<32>,
    /// Minimum signatures required for consensus
    pub min_signatures_required: u32,
    /// Whether to enforce strict ordering
    pub enforce_ordering: bool,
    /// Whether replay protection is enabled
    pub replay_protection_enabled: bool,
    /// Whether to accept unrecognised signature schemes leniently.
    /// When `false` (default), unknown schemes fail closed with
    /// `VerificationStatus::Invalid`. When `true`, they return
    /// `VerificationStatus::Unknown`.
    pub lenient: bool,
}

/// Signature scheme used to sign a cross-chain payload.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignatureScheme {
    /// Ed25519 signatures
    Ed25519,
    /// secp256k1 signatures
    Secp256k1,
    /// Placeholder for future/unknown schemes
    Unknown,
}

/// Outcome of resolving a signature scheme for verification.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemeResolution {
    /// Scheme is recognised and supported
    Supported,
    /// Scheme is unrecognised
    Unrecognised,
}

/// Resolve whether a signature scheme is recognised.
pub fn resolve_signature_scheme(scheme: &SignatureScheme) -> SchemeResolution {
    match scheme {
        SignatureScheme::Ed25519 | SignatureScheme::Secp256k1 => SchemeResolution::Supported,
        SignatureScheme::Unknown => SchemeResolution::Unrecognised,
    }
}

/// Verify a payload signed with the given signature scheme.
///
/// Fails closed: unrecognised schemes return `VerificationStatus::Invalid`
/// unless the caller explicitly opts into lenient mode via
/// `VerificationContext::lenient`, in which case `VerificationStatus::Unknown`
/// is returned.
pub fn verify_signature_scheme(
    scheme: &SignatureScheme,
    context: &VerificationContext,
) -> VerificationStatus {
    match resolve_signature_scheme(scheme) {
        SchemeResolution::Supported => VerificationStatus::Verified,
        SchemeResolution::Unrecognised => {
            if context.lenient {
                VerificationStatus::Unknown
            } else {
                VerificationStatus::Invalid
            }
        }
    }
}

/// Lenient verification entrypoint: preserves the legacy behaviour of
/// returning `VerificationStatus::Unknown` for unrecognised schemes.
/// Callers must opt in explicitly.
pub fn verify_signature_scheme_lenient(
    scheme: &SignatureScheme,
    context: &VerificationContext,
) -> VerificationStatus {
    match resolve_signature_scheme(scheme) {
        SchemeResolution::Supported => VerificationStatus::Verified,
        SchemeResolution::Unrecognised => VerificationStatus::Unknown,
    }
}

/// Record of a single validation attempt
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationRecord {
    /// Payload ID that was validated
    pub payload_id: BytesN<32>,
    /// Validator public key that performed validation
    pub validator_public_key: BytesN<32>,
    /// Result of the validation
    pub validation_result: VerificationStatus,
    /// Block height when validation occurred
    pub validation_height: u64,
    /// Timestamp of the validation
    pub validation_timestamp: u64,
    /// Any notes or comments about the validation
    pub notes: String,
}

/// Cross-chain consensus state
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsensusState {
    /// Payload ID for this consensus
    pub payload_id: BytesN<32>,
    /// Total validators that have voted
    pub votes_received: u32,
    /// Validators who voted to accept
    pub votes_for: u32,
    /// Validators who voted to reject
    pub votes_against: u32,
    /// Validators who abstained/didn't vote
    pub votes_abstain: u32,
    /// Whether consensus has been reached
    pub consensus_reached: bool,
    /// Final consensus result (if reached)
    pub final_result: VerificationStatus,
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    fn make_context(env: &Env, lenient: bool) -> VerificationContext {
        VerificationContext {
            payload_id: BytesN::from_array(env, &[0u8; 32]),
            current_height: 1,
            current_timestamp: 1,
            validator_set: ValidatorSet {
                validators: soroban_sdk::Vec::new(env),
                threshold: 1,
                nonce: 0,
            },
            payload_hash: BytesN::from_array(env, &[1u8; 32]),
            min_signatures_required: 1,
            enforce_ordering: false,
            replay_protection_enabled: true,
            lenient,
        }
    }

    #[test]
    fn unknown_scheme_fails_closed_by_default() {
        let env = Env::default();
        let ctx = make_context(&env, false);
        assert_eq!(
            verify_signature_scheme(&SignatureScheme::Unknown, &ctx),
            VerificationStatus::Invalid
        );
    }

    #[test]
    fn unknown_scheme_returns_unknown_in_lenient_mode() {
        let env = Env::default();
        let ctx = make_context(&env, true);
        assert_eq!(
            verify_signature_scheme(&SignatureScheme::Unknown, &ctx),
            VerificationStatus::Unknown
        );
        assert_eq!(
            verify_signature_scheme_lenient(&SignatureScheme::Unknown, &ctx),
            VerificationStatus::Unknown
        );
    }

    #[test]
    fn known_schemes_verify() {
        let env = Env::default();
        let ctx = make_context(&env, false);
        assert_eq!(
            verify_signature_scheme(&SignatureScheme::Ed25519, &ctx),
            VerificationStatus::Verified
        );
        assert_eq!(
            verify_signature_scheme(&SignatureScheme::Secp256k1, &ctx),
            VerificationStatus::Verified
        );
    }
}
