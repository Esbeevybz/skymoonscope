use crate::storage_types::{DataKey, Error};
use soroban_sdk::{Address, Env, Vec};

pub const REQUIRED_GUARDIANS: u32 = 5;

/// Strict-majority quorum for a guardian set of `guardian_count` members:
/// `floor(n / 2) + 1` — e.g. 3 of 5. This is the default applied by
/// `initialize` when no explicit `required_approvals` is supplied (issue #76).
/// A single compromised guardian can never meet this threshold.
pub fn default_required_approvals(guardian_count: u32) -> u32 {
    guardian_count / 2 + 1
}

// ── Guardian List ─────────────────────────────────────────────

pub fn read_guardians(e: &Env) -> Vec<Address> {
    e.storage()
        .instance()
        .get(&DataKey::Guardians)
        .unwrap_or(Vec::new(e))
}

pub fn write_guardians(e: &Env, guardians: &Vec<Address>) {
    e.storage().instance().set(&DataKey::Guardians, guardians);
}

/// Number of guardian approvals required to reach quorum.
///
/// Stored at initialization (issue #76) so the threshold is no longer a
/// compile-time constant. Returns `0` only when unset, which callers treat
/// as "not initialized".
pub fn read_required_approvals(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::RequiredApprovals)
        .unwrap_or(0u32)
}

pub fn write_required_approvals(e: &Env, required: u32) {
    e.storage()
        .instance()
        .set(&DataKey::RequiredApprovals, &required);
}

/// Returns the index (0..4) of `addr` in the guardian list.
pub fn guardian_index(guardians: &Vec<Address>, addr: &Address) -> Result<u32, Error> {
    for i in 0..guardians.len() {
        if guardians.get(i).unwrap() == *addr {
            return Ok(i);
        }
    }
    Err(Error::NotGuardian)
}

/// Validates exactly 5 unique addresses. O(n^2) with n=5 is trivial.
pub fn validate_guardians(guardians: &Vec<Address>) -> Result<(), Error> {
    if guardians.len() != REQUIRED_GUARDIANS {
        return Err(Error::InvalidGuardianCount);
    }
    for i in 0..guardians.len() {
        for j in (i + 1)..guardians.len() {
            if guardians.get(i).unwrap() == guardians.get(j).unwrap() {
                return Err(Error::DuplicateGuardian);
            }
        }
    }
    Ok(())
}

// ── Approval Bitmap ───────────────────────────────────────────

pub fn read_approvals(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::Approvals)
        .unwrap_or(0u32)
}

pub fn write_approvals(e: &Env, bitmap: u32) {
    e.storage().instance().set(&DataKey::Approvals, &bitmap);
}

pub fn read_epoch(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::ApprovalEpoch)
        .unwrap_or(0u32)
}

pub fn write_epoch(e: &Env, epoch: u32) {
    e.storage().instance().set(&DataKey::ApprovalEpoch, &epoch);
}

/// Sets bit `index` in bitmap. Returns error if already set.
pub fn set_approval(bitmap: u32, index: u32) -> Result<u32, Error> {
    let mask = 1u32 << index;
    if bitmap & mask != 0 {
        return Err(Error::AlreadyApproved);
    }
    Ok(bitmap | mask)
}

/// Counts set bits. O(1) for a 5-bit range.
pub fn approval_count(bitmap: u32) -> u32 {
    let mut n = bitmap;
    let mut count = 0u32;
    while n != 0 {
        count += n & 1;
        n >>= 1;
    }
    count
}

pub fn has_quorum(bitmap: u32, required: u32) -> bool {
    approval_count(bitmap) >= required
}

// ── Guardian Rotation Timelock Delay ──────────────────────────

pub const DEFAULT_ROTATION_DELAY_LEDGERS: u32 = 100;

pub fn read_rotation_delay(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::RotationDelayLedgers)
        .unwrap_or(DEFAULT_ROTATION_DELAY_LEDGERS)
}

pub fn write_rotation_delay(e: &Env, delay: u32) {
    e.storage()
        .instance()
        .set(&DataKey::RotationDelayLedgers, &delay);
}

pub fn read_pending_rotation(e: &Env) -> Option<crate::storage_types::PendingGuardianRotation> {
    e.storage().instance().get(&DataKey::PendingRotation)
}

pub fn write_pending_rotation(
    e: &Env,
    proposal: &crate::storage_types::PendingGuardianRotation,
) {
    e.storage().instance().set(&DataKey::PendingRotation, proposal);
}

pub fn clear_pending_rotation(e: &Env) {
    e.storage().instance().remove(&DataKey::PendingRotation);
}
