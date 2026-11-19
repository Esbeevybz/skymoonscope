#![no_std]
//! # TTL extension with a protocol-maximum clamp
//!
//! Issue #89: this contract bumped TTL by a caller-supplied amount without
//! checking whether the result exceeded the protocol maximum, which makes the
//! host trap.
//!
//! Every bump is therefore clamped to the remaining headroom before the
//! `extend_ttl` host call is issued:
//!
//! ```text
//! requested = min(requested, MAX_TTL - current_ttl)
//! ```
//!
//! so the contract can never ask the host for a TTL beyond the maximum, and a
//! caller cannot make the contract trap by passing an enormous bump.
//!
//! The original file was also syntactically invalid (`#a[no_std]`, `#contract`,
//! `#contractimpl` and a `Symbol::new!` macro that does not exist) and declared
//! its dependency as `sorban-sdk`; all of that is fixed here.

use soroban_sdk::{contract, contractimpl, symbol_short, Bytes, Env, Symbol};

#[cfg(test)]
mod test;

/// Storage key for the single persistent entry this contract manages.
pub const KEY: Symbol = symbol_short!("KEY");

/// Protocol maximum TTL, in ledgers, for a persistent or instance storage entry.
///
/// This is the Soroban network maximum entry TTL. Requesting anything above it
/// is a host-level error, so it must never reach `extend_ttl`.
pub const MAX_TTL: u32 = 535_677;

/// Default lower bound: an entry is only worth extending once it is within this
/// many ledgers of expiry, which keeps the common "nothing to do" case cheap.
pub const DEFAULT_THRESHOLD: u32 = 100;

#[contract]
pub struct TtlExtension;

#[contractimpl]
impl TtlExtension {
    /// Extend the TTL of the contract's instance storage and persistent entry,
    /// but only if they are within `threshold` ledgers of expiry.
    ///
    /// `extend_to` is an absolute target TTL and is clamped to [`MAX_TTL`].
    pub fn extend_ttl(env: Env, threshold: u32, extend_to: u32) {
        let target = clamp_target(extend_to);

        let instance_ttl = env.storage().instance().get_ttl();
        if instance_ttl < threshold {
            env.storage().instance().extend_ttl(threshold, target);
        }

        let persistent_ttl = env.storage().persistent().get_ttl(&KEY);
        if persistent_ttl < threshold {
            env.storage()
                .persistent()
                .extend_ttl(&KEY, threshold, target);
        }
    }

    /// Bump both entries by `requested_bump` ledgers, clamped to the protocol
    /// maximum.
    ///
    /// The clamp is the point of this entry point: without it, a caller could
    /// pass a bump that pushes the resulting TTL past [`MAX_TTL`] and trap the
    /// host. Returns the clamped bump that was actually applied.
    pub fn bump_ttl(env: Env, requested_bump: u32) -> u32 {
        let instance_current = env.storage().instance().get_ttl();
        let instance_headroom = headroom(instance_current);
        let applied = core::cmp::min(requested_bump, instance_headroom);

        if applied > 0 {
            let target = clamp_target(instance_current.saturating_add(applied));
            env.storage().instance().extend_ttl(applied, target);
            env.storage().persistent().extend_ttl(&KEY, applied, target);
        }

        applied
    }

    /// Bump only the persistent entry, clamped to the protocol maximum.
    pub fn bump_persistent_ttl(env: Env, requested_bump: u32) -> u32 {
        let current = env.storage().persistent().get_ttl(&KEY);
        let applied = core::cmp::min(requested_bump, headroom(current));
        if applied > 0 {
            let target = clamp_target(current.saturating_add(applied));
            env.storage().persistent().extend_ttl(&KEY, applied, target);
        }
        applied
    }

    /// Remaining headroom before the protocol maximum, in ledgers.
    pub fn max_ttl_headroom(env: Env) -> u32 {
        headroom(env.storage().instance().get_ttl())
    }

    pub fn write(env: Env, value: u32) {
        env.storage().persistent().set(&KEY, &value);
    }

    pub fn read(env: Env) -> Bytes {
        env.storage()
            .persistent()
            .get(&KEY)
            .map(|v: u32| Bytes::from_slice(&env, &v.to_be_bytes()))
            .unwrap_or_else(|| Bytes::new(&env))
    }

    pub fn persistent_ttl(env: Env) -> u32 {
        env.storage().persistent().get_ttl(&KEY)
    }

    pub fn instance_ttl(env: Env) -> u32 {
        env.storage().instance().get_ttl()
    }
}

/// Ledgers still available before [`MAX_TTL`] is reached.
fn headroom(current_ttl: u32) -> u32 {
    MAX_TTL.saturating_sub(current_ttl)
}

/// Clamp an absolute target TTL to the protocol maximum.
///
/// A target at or below the current TTL is returned unchanged: `extend_ttl`
/// treats a non-increasing target as a no-op, and clamping must never *raise* a
/// caller's value.
fn clamp_target(target: u32) -> u32 {
    core::cmp::min(target, MAX_TTL)
}
