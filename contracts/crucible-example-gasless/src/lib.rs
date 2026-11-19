#![no_std]
#![allow(deprecated)]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol,
};

/// A meta-transaction (gasless) request signed by the user.
#[contracttype]
#[derive(Clone)]
pub struct MetaTx {
    /// The user whose tokens will be transferred.
    pub from: Address,
    /// Recipient of the transfer.
    pub to: Address,
    /// Token contract address.
    pub token: Address,
    /// Amount to transfer.
    pub amount: i128,
    /// Nonce to prevent replay attacks within the current epoch.
    ///
    /// The nonce is only valid when paired with the matching `epoch`. A nonce
    /// signed under epoch N cannot be replayed once the account's epoch is
    /// incremented to N+1 because the epoch field of the `MetaTx` will no
    /// longer match the stored epoch.
    pub nonce: u64,
    /// Epoch number. Must match the account's current stored epoch.
    ///
    /// Rotating the epoch (via `rotate_epoch`) increments this counter *and*
    /// resets the nonce to 0, which invalidates every outstanding signature
    /// from the previous epoch without requiring the relayer to enumerate
    /// individual nonces.
    pub epoch: u64,
    /// Deadline (unix timestamp) after which this meta-tx is invalid.
    pub deadline: u64,
}

#[contracttype]
enum DataKey {
    /// Admin / relayer address.
    Admin,
    /// Per-user nonce counter. Lives in *persistent* storage, one ledger entry
    /// per account — see `read_nonce` for why this must not be instance or
    /// temporary storage.
    Nonce(Address),
    /// Per-user epoch counter. Lives in persistent storage alongside the nonce.
    ///
    /// When a user rotates their epoch, both the epoch entry is incremented and
    /// the nonce entry is reset to 0. Any `MetaTx` carrying the old epoch value
    /// is then rejected by the `epoch` check in `execute`, making cross-epoch
    /// replay impossible even if the nonce range overlaps.
    Epoch(Address),
}

/// Extend a nonce/epoch entry's TTL to this many ledgers on every write
/// (~60 days at a 5s close time).
pub const NONCE_TTL_EXTEND_TO: u32 = 1_036_800;
/// Extend a nonce/epoch entry's TTL when it drops below this many ledgers
/// (~30 days at a 5s close time).
pub const NONCE_TTL_THRESHOLD: u32 = 518_400;
/// Largest number of nonces a single `invalidate_nonces` call may burn.
///
/// Invalidation is a single O(1) write regardless of how far it jumps, so this
/// cap is not about cost: it stops one mistaken call from advancing the counter
/// so far that the account can never issue a usable meta-tx again.
pub const MAX_NONCE_ADVANCE: u64 = 10_000;

/// A gasless transaction (meta-transaction) contract.
///
/// A user signs a `MetaTx` off-chain. A trusted relayer submits it on-chain,
/// paying the network fee. The contract verifies the epoch, nonce, and
/// deadline, then executes the token transfer on behalf of the user.
///
/// In Soroban, "signing" is handled by `require_auth` — the user's auth entry
/// is attached to the transaction by the relayer. This contract enforces:
/// - Epoch binding (cross-epoch replay protection).
/// - Nonce uniqueness within an epoch (same-epoch replay protection).
/// - Deadline enforcement (expiry protection).
/// - Relayer-only submission.
/// - User-driven batch invalidation of pending nonces.
/// - User-driven epoch rotation to mass-cancel all outstanding signatures.
#[contract]
#[derive(Default)]
pub struct Gasless;

// Internal helpers. Kept out of the `#[contractimpl]` block below so they do
// not become part of the contract's public interface.
impl Gasless {
    /// Read `user`'s next expected nonce.
    ///
    /// The counter lives in persistent storage, which matters for replay
    /// protection in two separate ways:
    ///
    /// - It must not be *temporary* storage. A temporary entry is deleted once
    ///   it expires, and a deleted entry reads back as `0` — which would reset
    ///   the counter and make every previously consumed nonce replayable. A
    ///   persistent entry is archived rather than deleted, and archived state
    ///   must be restored (with its value intact) before the contract can be
    ///   invoked against it, so the counter can never silently rewind.
    /// - It must not be *instance* storage. Instance storage is a single ledger
    ///   entry shared by the whole contract, so every account's nonce would be
    ///   packed into one value that is read and rewritten on every execution,
    ///   and the contract would stop working once enough accounts had been seen
    ///   to exceed the entry size limit.
    fn read_nonce(env: &Env, user: &Address) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::Nonce(user.clone()))
            .unwrap_or(0u64)
    }

    /// Write `user`'s next expected nonce and refresh the entry's TTL.
    ///
    /// The TTL bump happens on the same write that consumes a nonce, so an
    /// account stays alive for as long as it keeps transacting.
    fn write_nonce(env: &Env, user: &Address, value: u64) {
        let key = DataKey::Nonce(user.clone());
        env.storage().persistent().set(&key, &value);
        env.storage()
            .persistent()
            .extend_ttl(&key, NONCE_TTL_THRESHOLD, NONCE_TTL_EXTEND_TO);
    }

    /// Read `user`'s current epoch.
    ///
    /// A fresh account has epoch 0. Epochs only increase, via `rotate_epoch`.
    fn read_epoch(env: &Env, user: &Address) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::Epoch(user.clone()))
            .unwrap_or(0u64)
    }

    /// Write `user`'s epoch and refresh the entry's TTL.
    fn write_epoch(env: &Env, user: &Address, value: u64) {
        let key = DataKey::Epoch(user.clone());
        env.storage().persistent().set(&key, &value);
        env.storage()
            .persistent()
            .extend_ttl(&key, NONCE_TTL_THRESHOLD, NONCE_TTL_EXTEND_TO);
    }
}

#[contractimpl]
impl Gasless {
    /// Initialize the contract with a trusted relayer address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .extend_ttl(NONCE_TTL_THRESHOLD, NONCE_TTL_EXTEND_TO);
    }

    /// Execute a meta-transaction on behalf of `meta_tx.from`.
    ///
    /// Must be called by the registered relayer (admin).
    /// The user's authorization is verified via `meta_tx.from.require_auth()`.
    ///
    /// The `meta_tx.epoch` field must equal the user's current stored epoch.
    /// This binding ensures that a signature minted under epoch N is rejected
    /// after `rotate_epoch` advances the account to epoch N+1, preventing
    /// cross-epoch replay even when nonce ranges overlap between epochs.
    pub fn execute(env: Env, relayer: Address, meta_tx: MetaTx) {
        // Only the registered relayer may submit.
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if relayer != admin {
            panic!("unauthorized relayer");
        }
        relayer.require_auth();

        // Deadline check.
        let now = env.ledger().timestamp();
        if now > meta_tx.deadline {
            panic!("meta-tx expired");
        }

        // Epoch check — the meta-tx must carry the account's *current* epoch.
        // A signature produced under a previous epoch is rejected here, making
        // cross-epoch replay impossible.
        let stored_epoch = Self::read_epoch(&env, &meta_tx.from);
        if meta_tx.epoch != stored_epoch {
            panic!("invalid epoch");
        }

        // Nonce check — must match the stored next-nonce for this user. An
        // invalidated nonce fails here too, because invalidation moves the same
        // counter forward.
        let expected_nonce = Self::read_nonce(&env, &meta_tx.from);
        if meta_tx.nonce != expected_nonce {
            panic!("invalid nonce");
        }

        // Require the user's authorization (attached by the relayer).
        meta_tx.from.require_auth();

        // Consume the nonce *before* handing control to the token contract.
        // Soroban already forbids contract re-entry at the host level, so a
        // hostile token cannot call back into `execute` at all; ordering the
        // write first is defence in depth (checks-effects-interactions) that
        // keeps replay protection correct without relying on that guarantee.
        // `checked_add` keeps a saturated counter from wrapping back to 0.
        let next_nonce = expected_nonce.checked_add(1).expect("nonce overflow");
        Self::write_nonce(&env, &meta_tx.from, next_nonce);

        env.storage()
            .instance()
            .extend_ttl(NONCE_TTL_THRESHOLD, NONCE_TTL_EXTEND_TO);

        // Execute the transfer.
        token::Client::new(&env, &meta_tx.token).transfer(
            &meta_tx.from,
            &meta_tx.to,
            &meta_tx.amount,
        );

        env.events().publish(
            (symbol_short!("executed"),),
            (meta_tx.from, meta_tx.to, meta_tx.amount, meta_tx.nonce),
        );
    }

    /// Invalidate every nonce below `new_nonce` for `user`, in one write.
    ///
    /// This is how a user cancels meta-transactions they have already signed
    /// but that have not been submitted yet — for example after handing a
    /// batch to a relayer that never broadcast them. Advancing the counter to
    /// `new_nonce` retires the whole contiguous range `[current, new_nonce)` at
    /// once, so cancelling a hundred pending meta-txs costs the same single
    /// ledger write as cancelling one.
    ///
    /// Requires `user`'s authorization, so a relayer can submit this for the
    /// user exactly like an `execute` — invalidation is itself gasless. Returns
    /// the new next-expected nonce.
    ///
    /// Panics if `new_nonce` does not move the counter forward, or if it would
    /// advance it by more than [`MAX_NONCE_ADVANCE`] at once.
    pub fn invalidate_nonces(env: Env, user: Address, new_nonce: u64) -> u64 {
        user.require_auth();

        let current = Self::read_nonce(&env, &user);
        if new_nonce <= current {
            panic!("invalid nonce");
        }
        if new_nonce - current > MAX_NONCE_ADVANCE {
            panic!("advance too large");
        }

        Self::write_nonce(&env, &user, new_nonce);
        env.storage()
            .instance()
            .extend_ttl(NONCE_TTL_THRESHOLD, NONCE_TTL_EXTEND_TO);

        env.events().publish(
            (Symbol::new(&env, "invalidated"), user),
            (current, new_nonce),
        );

        new_nonce
    }

    /// Rotate the epoch for `user`, atomically resetting the nonce to 0.
    ///
    /// Every `MetaTx` the user has previously signed carries the old epoch
    /// number. After rotation, the epoch check in `execute` rejects all of
    /// those signatures, so the user can instantly revoke their entire pending
    /// queue — including nonces that `invalidate_nonces` cannot reach because
    /// the range cap of [`MAX_NONCE_ADVANCE`] would be exceeded.
    ///
    /// New meta-txs must be signed with the updated epoch returned by this
    /// function and must restart their nonce at 0.
    ///
    /// Requires `user`'s authorization, so a relayer can submit this gaslessly.
    /// Returns `(new_epoch, new_nonce)` where `new_nonce` is always 0.
    ///
    /// Panics on epoch overflow (u64 exhaustion, practically unreachable).
    pub fn rotate_epoch(env: Env, user: Address) -> (u64, u64) {
        user.require_auth();

        let current_epoch = Self::read_epoch(&env, &user);
        let new_epoch = current_epoch.checked_add(1).expect("epoch overflow");

        // Atomically advance the epoch and reset the nonce. Both writes happen
        // in the same transaction, so there is no window where a relayer could
        // submit an old-epoch meta-tx between the two writes.
        Self::write_epoch(&env, &user, new_epoch);
        Self::write_nonce(&env, &user, 0);

        env.storage()
            .instance()
            .extend_ttl(NONCE_TTL_THRESHOLD, NONCE_TTL_EXTEND_TO);

        env.events().publish(
            (Symbol::new(&env, "epoch_rotated"), user),
            (current_epoch, new_epoch),
        );

        (new_epoch, 0)
    }

    /// Return the current nonce for `user` (the next expected nonce).
    pub fn nonce(env: Env, user: Address) -> u64 {
        Self::read_nonce(&env, &user)
    }

    /// Return the current epoch for `user`.
    pub fn epoch(env: Env, user: Address) -> u64 {
        Self::read_epoch(&env, &user)
    }

    /// Return the relayer address.
    pub fn relayer(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized")
    }
}

/// Helper to build a `MetaTx` with an explicit epoch (used in tests).
pub fn make_meta_tx_with_epoch(
    _env: &Env,
    from: Address,
    to: Address,
    token: Address,
    amount: i128,
    nonce: u64,
    epoch: u64,
    deadline: u64,
) -> MetaTx {
    MetaTx {
        from,
        to,
        token,
        amount,
        nonce,
        epoch,
        deadline,
    }
}

/// Helper to build a `MetaTx` at epoch 0 (backwards-compatible, used in
/// existing tests where the epoch has never been rotated).
pub fn make_meta_tx(
    env: &Env,
    from: Address,
    to: Address,
    token: Address,
    amount: i128,
    nonce: u64,
    deadline: u64,
) -> MetaTx {
    make_meta_tx_with_epoch(env, from, to, token, amount, nonce, 0, deadline)
}

#[cfg(test)]
mod test;
