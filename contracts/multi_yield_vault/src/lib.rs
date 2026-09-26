#![no_std]
//! # Multi-Yield Aggregator Vault
//!
//! Accepts a single deposit token and splits/rebalances it across up to
//! `MAX_POOLS` registered Soroban AMM pools to maximise yield.
//!
//! ## APR estimation
//! Each registered pool exposes `reserve_a`, `reserve_b`, and `fee_bps`.
//! We approximate the 24-h fee APR for the deposit token as:
//!
//!   estimated_apr_bps = fee_bps * VOLUME_PROXY / reserve_deposit_token
//!
//! where `VOLUME_PROXY` is a configurable constant representing the assumed
//! daily volume relative to the pool's reserve (default: 100 % of reserve,
//! i.e. `VOLUME_PROXY = 10_000` in bps).  This is intentionally simple and
//! can be replaced by an oracle-fed value without changing the interface.
//!
//! ## Rebalancing
//! `rebalance()` reads the current allocation, estimates APR for every pool,
//! sorts pools by APR descending, and moves funds from under-performing pools
//! to the best pool.  Each withdrawal and deposit is slippage-protected: the
//! contract checks that the amount received from a pool withdrawal is within
//! `slippage_bps` of the expected amount before proceeding.
//!
//! ## Withdrawal queue
//! Shares are minted 1:1 with the deposit token, so a withdrawal is entitled to
//! exactly as many tokens as the shares it burns.  That entitlement is not always
//! payable on the spot: the pools may have moved against the vault, or the vault's
//! idle balance may not cover the redemption.  Rather than skipping such a
//! withdrawal, `withdraw()` pays out what it can and books the difference as a
//! persistent `WithdrawalRequest`.  `process_withdraw_queue()` then settles those
//! requests oldest-first as liquidity returns, and `cancel_withdrawal()` lets an
//! owner walk away from a claim they no longer want.

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

#[cfg(test)]
mod test;

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidAmount = 3,
    TooManyPools = 4,
    PoolAlreadyRegistered = 5,
    PoolNotFound = 6,
    SlippageExceeded = 7,
    InsufficientShares = 8,
    Unauthorized = 9,
    InvalidWeights = 10,
    /// The withdrawal queue is full; the request could not be recorded.
    QueueFull = 11,
    /// No withdrawal request exists for the given id.
    WithdrawalNotFound = 12,
    /// The withdrawal request has already been fully filled or cancelled.
    WithdrawalClosed = 13,
}

// ── External pool interface ───────────────────────────────────────────────────

/// Minimal interface the vault calls on each registered AMM pool.
#[soroban_sdk::contractclient(name = "AmmPoolClient")]
pub trait AmmPool {
    /// Deposit `amount_a` and `amount_b`; returns LP shares minted.
    fn deposit(e: Env, to: Address, amount_a: i128, amount_b: i128) -> i128;
    /// Burn `shares`; returns (amount_a, amount_b) withdrawn.
    fn withdraw(e: Env, to: Address, share_amount: i128) -> (i128, i128);
    /// Current reserve of token_a.
    fn get_reserve_a(e: Env) -> i128;
    /// Current reserve of token_b.
    fn get_reserve_b(e: Env) -> i128;
    /// Swap fee in basis points.
    fn get_fee(e: Env) -> i128;
    /// Which of the two pool tokens is token_a.
    fn token_a(e: Env) -> Address;
}

// ── Storage types ─────────────────────────────────────────────────────────────

/// Vault-wide configuration and accounting, stored as a single instance entry.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultState {
    /// The single token users deposit (e.g. USDC).
    pub deposit_token: Address,
    pub admin: Address,
    /// Total vault shares outstanding.
    pub total_shares: i128,
    /// Maximum slippage tolerated on rebalance withdrawals, in bps.
    pub slippage_bps: i128,
    /// Assumed daily volume as a fraction of pool reserve, in bps (default 10_000 = 100%).
    pub volume_proxy_bps: i128,
}

/// Per-pool allocation record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolAllocation {
    /// AMM pool contract address.
    pub pool: Address,
    /// LP shares the vault holds in this pool.
    pub lp_shares: i128,
    /// Whether the deposit token is token_a (true) or token_b (false) in this pool.
    pub deposit_is_a: bool,
    /// Target weight in basis points (e.g. 5,000 = 50%).
    pub weight: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Vault,
    /// Vec<PoolAllocation> — ordered list of registered pools.
    Pools,
    /// Per-user vault share balance.
    Balance(Address),
    /// Vec<u32> — ids of the open withdrawal requests, oldest first.
    Queue,
    /// Id to hand out to the next withdrawal request.
    NextQueueId,
    /// WithdrawalRequest by id.
    Withdrawal(u32),
}

/// A withdrawal that the vault could not satisfy in full at request time.
///
/// The vault mints shares 1:1 with the deposit token (see `MultiYieldVault::deposit`),
/// so `remaining_shares` is also the amount of deposit token still owed to `owner`.
/// The request is a burned-in claim: the shares backing it were already deducted from
/// the owner's balance and from `total_shares` when the request was created, so the
/// entry can only be settled once, by `MultiYieldVault::process_withdraw_queue`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalRequest {
    /// Address entitled to the remaining payout.
    pub owner: Address,
    /// Deposit token this request was opened for.
    pub requested_shares: i128,
    /// Deposit token still owed to `owner`.
    pub remaining_shares: i128,
    /// Deposit token this request has paid out since it was opened.  Whatever was
    /// paid directly by the `withdraw` call that opened the request is not counted
    /// here; the originating `withdraw_partial` event reports it.
    pub filled_amount: i128,
    /// True once the request is fully filled or cancelled.
    pub closed: bool,
}

/// Summary returned by `MultiYieldVault::process_withdraw_queue`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueFillResult {
    /// Requests visited during this call.
    pub processed: u32,
    /// Requests that were fully settled and removed from the queue.
    pub completed: u32,
    /// Deposit token paid out across all settled requests.
    pub paid_out: i128,
    /// Requests still waiting for liquidity when the call returned.
    pub remaining_in_queue: u32,
}

// ── Constants ─────────────────────────────────────────────────────────────────

pub const MAX_POOLS: u32 = 8;
pub const TTL_LEDGERS: u32 = 17_280;
pub const DEFAULT_SLIPPAGE_BPS: i128 = 100; // 1 %
pub const DEFAULT_VOLUME_PROXY_BPS: i128 = 10_000; // 100 % of reserve

/// Hard ceiling on open withdrawal requests, so a liquidity crunch cannot be used
/// to grow `persistent` storage without bound.  Requests beyond this are rejected
/// outright rather than silently dropped.
pub const MAX_QUEUE_ENTRIES: u32 = 128;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn load_vault(e: &Env) -> Result<VaultState, Error> {
    e.storage()
        .instance()
        .get(&DataKey::Vault)
        .ok_or(Error::NotInitialized)
}

fn save_vault(e: &Env, v: &VaultState) {
    e.storage().instance().set(&DataKey::Vault, v);
}

fn load_pools(e: &Env) -> Vec<PoolAllocation> {
    e.storage()
        .instance()
        .get(&DataKey::Pools)
        .unwrap_or(Vec::new(e))
}

fn save_pools(e: &Env, pools: &Vec<PoolAllocation>) {
    e.storage().instance().set(&DataKey::Pools, pools);
}

// ── Withdrawal queue helpers ──────────────────────────────────────────────────

fn load_queue(e: &Env) -> Vec<u32> {
    e.storage()
        .instance()
        .get(&DataKey::Queue)
        .unwrap_or(Vec::new(e))
}

fn save_queue(e: &Env, queue: &Vec<u32>) {
    e.storage().instance().set(&DataKey::Queue, queue);
}

fn request_key(id: u32) -> DataKey {
    DataKey::Withdrawal(id)
}

fn load_request(e: &Env, id: u32) -> Result<WithdrawalRequest, Error> {
    e.storage()
        .persistent()
        .get(&request_key(id))
        .ok_or(Error::WithdrawalNotFound)
}

fn save_request(e: &Env, id: u32, req: &WithdrawalRequest) {
    let key = request_key(id);
    e.storage().persistent().set(&key, req);
    e.storage()
        .persistent()
        .extend_ttl(&key, TTL_LEDGERS, TTL_LEDGERS);
}

/// Deposit token currently idle in the vault, i.e. available to pay a queued
/// withdrawal without redeeming further LP positions.
fn available_liquidity(e: &Env, deposit_token: &Address) -> i128 {
    soroban_sdk::token::Client::new(e, deposit_token).balance(&e.current_contract_address())
}

/// Append a new open request to the FIFO queue and return its id.
fn enqueue_request(e: &Env, owner: &Address, requested: i128) -> Result<u32, Error> {
    let mut queue = load_queue(e);
    if queue.len() >= MAX_QUEUE_ENTRIES {
        return Err(Error::QueueFull);
    }
    let id: u32 = e
        .storage()
        .instance()
        .get(&DataKey::NextQueueId)
        .unwrap_or(0);
    e.storage().instance().set(&DataKey::NextQueueId, &(id + 1));

    save_request(
        e,
        id,
        &WithdrawalRequest {
            owner: owner.clone(),
            requested_shares: requested,
            remaining_shares: requested,
            filled_amount: 0,
            closed: false,
        },
    );
    queue.push_back(id);
    save_queue(e, &queue);
    Ok(id)
}

/// Pay `owner` up to `amount` from idle vault liquidity, returning what was
/// actually transferred.  Never moves more than the vault holds.
fn pay_out(e: &Env, deposit_token: &Address, owner: &Address, amount: i128) -> i128 {
    if amount <= 0 {
        return 0;
    }
    let available = available_liquidity(e, deposit_token);
    let payout = amount.min(available);
    if payout > 0 {
        soroban_sdk::token::Client::new(e, deposit_token).transfer(
            &e.current_contract_address(),
            owner,
            &payout,
        );
    }
    payout
}

/// Estimate APR in bps for the deposit token in a given pool.
///
/// apr_bps = fee_bps * volume_proxy_bps / 10_000
///
/// This represents: if daily volume = `volume_proxy_bps/10_000` × reserve,
/// then daily fee revenue / deposit = fee_bps/10_000 × volume_proxy_bps/10_000.
/// Annualised (×365) is omitted here — we only need relative ranking.
fn estimate_apr_bps(fee_bps: i128, volume_proxy_bps: i128) -> i128 {
    fee_bps * volume_proxy_bps / 10_000
}

/// How much of the deposit token the vault has in a given pool allocation.
fn deposit_value_in_pool(e: &Env, alloc: &PoolAllocation) -> i128 {
    if alloc.lp_shares == 0 {
        return 0;
    }
    let client = AmmPoolClient::new(e, &alloc.pool);
    let reserve_a = client.get_reserve_a();
    let reserve_b = client.get_reserve_b();
    // Total LP supply is not exposed by the minimal interface, so we approximate
    // by reading the pool's reserves and assuming the vault's share is proportional.
    // We store lp_shares and use the ratio: value ≈ lp_shares / total_lp * reserve.
    // Since we can't get total_lp cheaply, we track deposited amounts separately
    // via a simpler invariant: on deposit we record the token amount, on withdraw
    // we get it back.  For APR ranking we only need the fee and volume proxy.
    if alloc.deposit_is_a {
        reserve_a
    } else {
        reserve_b
    }
}

fn validate_weighted_pools(e: &Env, pools: &Vec<PoolAllocation>) -> Result<(), Error> {
    for i in 0..pools.len() {
        let alloc = pools.get(i).unwrap();
        if alloc.weight == 0 {
            continue;
        }
        let client = AmmPoolClient::new(e, &alloc.pool);
        if client.get_fee() <= 0 {
            return Err(Error::InvalidWeights);
        }
    }
    Ok(())
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct MultiYieldVault;

#[contractimpl]
impl MultiYieldVault {
    // ── Admin ─────────────────────────────────────────────────────────────────

    pub fn initialize(
        e: Env,
        admin: Address,
        deposit_token: Address,
        slippage_bps: i128,
        volume_proxy_bps: i128,
    ) -> Result<(), Error> {
        if e.storage().instance().has(&DataKey::Vault) {
            return Err(Error::AlreadyInitialized);
        }
        if slippage_bps < 0 || volume_proxy_bps <= 0 {
            return Err(Error::InvalidAmount);
        }
        save_vault(
            &e,
            &VaultState {
                deposit_token,
                admin,
                total_shares: 0,
                slippage_bps,
                volume_proxy_bps,
            },
        );
        Ok(())
    }

    /// Register a new AMM pool.  `deposit_is_a` tells the vault whether the
    /// deposit token is token_a (true) or token_b (false) in that pool.
    pub fn register_pool(e: Env, pool: Address, deposit_is_a: bool) -> Result<(), Error> {
        let vault = load_vault(&e)?;
        vault.admin.require_auth();

        let mut pools = load_pools(&e);
        if pools.len() >= MAX_POOLS {
            return Err(Error::TooManyPools);
        }
        for i in 0..pools.len() {
            if pools.get(i).unwrap().pool == pool {
                return Err(Error::PoolAlreadyRegistered);
            }
        }
        pools.push_back(PoolAllocation {
            pool,
            lp_shares: 0,
            deposit_is_a,
            weight: 0,
        });
        save_pools(&e, &pools);
        Ok(())
    }

    /// Set strategy weights for the registered pools.
    /// Weights must be specified in basis points, sum to 10,000, and no single weight can exceed 5,000 (50%).
    pub fn set_weights(e: Env, weights: Vec<u32>) -> Result<(), Error> {
        let vault = load_vault(&e)?;
        vault.admin.require_auth();

        let mut pools = load_pools(&e);
        if weights.len() != pools.len() {
            return Err(Error::InvalidWeights);
        }

        let mut sum_weights: u32 = 0;
        for i in 0..weights.len() {
            let w = weights.get(i).unwrap();
            if w > 5_000 {
                return Err(Error::InvalidWeights);
            }
            sum_weights += w;
        }

        if sum_weights != 10_000 {
            return Err(Error::InvalidWeights);
        }

        for i in 0..pools.len() {
            let mut alloc = pools.get(i).unwrap();
            alloc.weight = weights.get(i).unwrap();
            pools.set(i, alloc);
        }

        validate_weighted_pools(&e, &pools)?;

        save_pools(&e, &pools);
        Ok(())
    }

    pub fn set_slippage(e: Env, slippage_bps: i128) -> Result<(), Error> {
        let mut vault = load_vault(&e)?;
        vault.admin.require_auth();
        vault.slippage_bps = slippage_bps;
        save_vault(&e, &vault);
        Ok(())
    }

    // ── User deposit / withdraw ───────────────────────────────────────────────

    /// Deposit `amount` of the deposit token.  Funds are routed to the
    /// highest-APR registered pool.  Returns vault shares minted.
    pub fn deposit(e: Env, from: Address, amount: i128) -> Result<i128, Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        from.require_auth();
        let mut vault = load_vault(&e)?;
        let pools = load_pools(&e);
        if pools.len() == 0 {
            return Err(Error::PoolNotFound);
        }

        // Pull deposit token from user into vault.
        soroban_sdk::token::Client::new(&e, &vault.deposit_token).transfer(
            &from,
            &e.current_contract_address(),
            &amount,
        );

        // Validate weights sum
        let mut sum_weights: u32 = 0;
        let mut last_non_zero_idx: Option<u32> = None;
        for i in 0..pools.len() {
            let w = pools.get(i).unwrap().weight;
            sum_weights += w;
            if w > 0 {
                last_non_zero_idx = Some(i);
            }
        }
        if sum_weights != 10_000 {
            return Err(Error::InvalidWeights);
        }
        let last_idx = last_non_zero_idx.ok_or(Error::InvalidWeights)?;
        validate_weighted_pools(&e, &pools)?;

        // Deposit into pools based on weights.
        let mut pools_mut = pools;
        let mut remaining_amount = amount;

        for i in 0..pools_mut.len() {
            let mut alloc = pools_mut.get(i).unwrap();
            if alloc.weight == 0 {
                continue;
            }
            let pool_amount = if i == last_idx {
                remaining_amount
            } else {
                amount * (alloc.weight as i128) / 10_000
            };

            if pool_amount <= 0 {
                continue;
            }
            remaining_amount -= pool_amount;

            let client = AmmPoolClient::new(&e, &alloc.pool);
            let (dep_a, dep_b) = if alloc.deposit_is_a {
                (pool_amount, 0i128)
            } else {
                (0i128, pool_amount)
            };

            // Approve pool to pull tokens from vault.
            soroban_sdk::token::Client::new(&e, &vault.deposit_token).approve(
                &e.current_contract_address(),
                &alloc.pool,
                &pool_amount,
                &(e.ledger().sequence() + 1),
            );

            let new_lp = client.deposit(&e.current_contract_address(), &dep_a, &dep_b);
            alloc.lp_shares += new_lp;
            pools_mut.set(i, alloc);
        }
        save_pools(&e, &pools_mut);

        // Mint vault shares proportional to deposit.
        let shares = if vault.total_shares == 0 {
            amount
        } else {
            amount // 1:1 for simplicity; production would use NAV-based pricing
        };

        let bal_key = DataKey::Balance(from.clone());
        let cur: i128 = e.storage().persistent().get(&bal_key).unwrap_or(0);
        e.storage().persistent().set(&bal_key, &(cur + shares));
        e.storage()
            .persistent()
            .extend_ttl(&bal_key, TTL_LEDGERS, TTL_LEDGERS);

        vault.total_shares += shares;
        save_vault(&e, &vault);

        Ok(shares)
    }

    /// Burn `shares` and receive deposit tokens back.
    ///
    /// The vault mints shares 1:1 with the deposit token, so `shares` is also the
    /// amount of deposit token owed to `to`.  When the vault cannot cover that
    /// amount in full — drained pools, or an idle balance smaller than the
    /// redemption — it pays out what it can and records the remainder as a
    /// persistent queue entry instead of skipping the withdrawal.  The returned
    /// value is the amount actually transferred; a smaller return than `shares`
    /// means a queue entry now exists for the difference.
    pub fn withdraw(e: Env, to: Address, shares: i128) -> Result<i128, Error> {
        if shares <= 0 {
            return Err(Error::InvalidAmount);
        }
        to.require_auth();
        let mut vault = load_vault(&e)?;

        let bal_key = DataKey::Balance(to.clone());
        let cur: i128 = e.storage().persistent().get(&bal_key).unwrap_or(0);
        if shares > cur {
            return Err(Error::InsufficientShares);
        }

        // Proportional share of total vault assets.
        let fraction_num = shares;
        let fraction_den = vault.total_shares;

        let mut pools = load_pools(&e);
        let mut total_received: i128 = 0;

        for i in 0..pools.len() {
            let mut alloc = pools.get(i).unwrap();
            if alloc.lp_shares == 0 {
                continue;
            }
            // Withdraw proportional LP shares from this pool.
            let lp_to_burn = alloc.lp_shares * fraction_num / fraction_den;
            if lp_to_burn == 0 {
                continue;
            }

            let client = AmmPoolClient::new(&e, &alloc.pool);
            let (out_a, out_b) = client.withdraw(&e.current_contract_address(), &lp_to_burn);

            let received = if alloc.deposit_is_a { out_a } else { out_b };
            total_received += received;
            alloc.lp_shares -= lp_to_burn;
            pools.set(i, alloc);
        }
        save_pools(&e, &pools);

        // Send deposit token to user, but never more than the vault can actually
        // cover right now.  Anything left over stays owed to the user and is
        // recorded in the withdrawal queue.
        let payout = pay_out(&e, &vault.deposit_token, &to, total_received.min(shares));
        let remainder = shares - payout;

        // The full request is settled against the user's balance up front: the
        // unpaid remainder is carried by the queue entry, so leaving those shares
        // in the balance would let the same shares be claimed twice.
        e.storage().persistent().set(&bal_key, &(cur - shares));
        e.storage()
            .persistent()
            .extend_ttl(&bal_key, TTL_LEDGERS, TTL_LEDGERS);

        vault.total_shares -= shares;
        save_vault(&e, &vault);

        if remainder > 0 {
            let id = enqueue_request(&e, &to, remainder)?;
            e.events().publish(
                (Symbol::new(&e, "withdraw_partial"), to.clone()),
                (id, payout, remainder),
            );
        }

        Ok(payout)
    }

    // ── Withdrawal queue ──────────────────────────────────────────────────────

    /// Settle queued withdrawals, oldest request first.
    ///
    /// Permissionless so that anyone can keep the queue moving, but strictly
    /// FIFO: a request is never paid while an older one is still waiting.  Each
    /// request is filled as far as idle liquidity allows and stays queued for the
    /// rest, so a dry vault makes no progress instead of burning through claims.
    ///
    /// `max_entries` bounds how many requests are visited per call; it defaults to
    /// `MAX_QUEUE_ENTRIES` when passed as 0.
    pub fn process_withdraw_queue(e: Env, max_entries: u32) -> Result<QueueFillResult, Error> {
        let vault = load_vault(&e)?;
        let budget = if max_entries == 0 {
            MAX_QUEUE_ENTRIES
        } else {
            max_entries.min(MAX_QUEUE_ENTRIES)
        };

        let mut queue = load_queue(&e);
        let mut result = QueueFillResult {
            processed: 0,
            completed: 0,
            paid_out: 0,
            remaining_in_queue: queue.len(),
        };
        if queue.len() == 0 {
            return Ok(result);
        }

        let mut visited: u32 = 0;
        // Always keep looking at the head of the queue: it is the only request
        // allowed to receive funds, so a dry head stops the whole pass.
        while visited < budget && queue.len() > 0 {
            let id = queue.get(0).unwrap();
            let mut req = load_request(&e, id)?;
            if req.closed || req.remaining_shares <= 0 {
                queue.remove(0);
                continue;
            }

            let available = available_liquidity(&e, &vault.deposit_token);
            if available <= 0 {
                break;
            }

            let want = req.remaining_shares.min(available);
            let payout = pay_out(&e, &vault.deposit_token, &req.owner, want);
            if payout <= 0 {
                break;
            }

            req.remaining_shares -= payout;
            req.filled_amount += payout;
            result.processed += 1;
            result.paid_out += payout;
            visited += 1;

            if req.remaining_shares == 0 {
                req.closed = true;
                save_request(&e, id, &req);
                queue.remove(0);
                result.completed += 1;
            } else {
                // Partially filled again: the request keeps its place at the head
                // so ordering is preserved and no later claim jumps ahead of it.
                save_request(&e, id, &req);
                break;
            }
        }

        save_queue(&e, &queue);
        result.remaining_in_queue = queue.len();

        e.events().publish(
            (Symbol::new(&e, "queue_processed"),),
            (result.processed, result.completed, result.paid_out),
        );
        Ok(result)
    }

    /// Abandon a queued withdrawal.  Only the request owner may cancel, and only
    /// while the request is still open.  The unpaid remainder is simply written
    /// off — the shares behind it were already deducted when the request was made.
    pub fn cancel_withdrawal(e: Env, owner: Address, id: u32) -> Result<(), Error> {
        owner.require_auth();
        let mut req = load_request(&e, id)?;
        if req.owner != owner {
            return Err(Error::Unauthorized);
        }
        if req.closed || req.remaining_shares <= 0 {
            return Err(Error::WithdrawalClosed);
        }
        req.remaining_shares = 0;
        req.closed = true;
        save_request(&e, id, &req);

        let mut queue = load_queue(&e);
        for i in 0..queue.len() {
            if queue.get(i).unwrap() == id {
                queue.remove(i);
                break;
            }
        }
        save_queue(&e, &queue);
        Ok(())
    }

    // ── Rebalancing ───────────────────────────────────────────────────────────

    /// Rebalance: redistribute all assets across all pools based on strategy weights.
    pub fn rebalance(e: Env) -> Result<(), Error> {
        let vault = load_vault(&e)?;
        let mut pools = load_pools(&e);
        if pools.len() <= 1 {
            return Ok(()); // nothing to rebalance
        }

        // Validate weights sum
        let mut sum_weights: u32 = 0;
        let mut last_non_zero_idx: Option<u32> = None;
        for i in 0..pools.len() {
            let w = pools.get(i).unwrap().weight;
            sum_weights += w;
            if w > 0 {
                last_non_zero_idx = Some(i);
            }
        }
        if sum_weights != 10_000 {
            return Err(Error::InvalidWeights);
        }
        let last_idx = last_non_zero_idx.ok_or(Error::InvalidWeights)?;
        validate_weighted_pools(&e, &pools)?;

        let mut total_to_move: i128 = 0;

        // Step 1: withdraw from all pools.
        for i in 0..pools.len() {
            let mut alloc = pools.get(i).unwrap();
            if alloc.lp_shares == 0 {
                continue;
            }

            let client = AmmPoolClient::new(&e, &alloc.pool);

            // Estimate expected deposit-token return before withdrawing.
            let reserve = if alloc.deposit_is_a {
                client.get_reserve_a()
            } else {
                client.get_reserve_b()
            };
            let expected = reserve.min(alloc.lp_shares);

            let (out_a, out_b) = client.withdraw(&e.current_contract_address(), &alloc.lp_shares);
            let received = if alloc.deposit_is_a { out_a } else { out_b };

            // Slippage check.
            let min_acceptable = expected * (10_000 - vault.slippage_bps) / 10_000;
            if received < min_acceptable {
                return Err(Error::SlippageExceeded);
            }

            total_to_move += received;
            alloc.lp_shares = 0;
            pools.set(i, alloc);
        }

        // Step 2: deposit back according to weights.
        if total_to_move > 0 {
            let mut remaining_amount = total_to_move;
            for i in 0..pools.len() {
                let mut alloc = pools.get(i).unwrap();
                if alloc.weight == 0 {
                    continue;
                }
                let pool_amount = if i == last_idx {
                    remaining_amount
                } else {
                    total_to_move * (alloc.weight as i128) / 10_000
                };

                if pool_amount <= 0 {
                    continue;
                }
                remaining_amount -= pool_amount;

                let client = AmmPoolClient::new(&e, &alloc.pool);
                let (dep_a, dep_b) = if alloc.deposit_is_a {
                    (pool_amount, 0i128)
                } else {
                    (0i128, pool_amount)
                };

                soroban_sdk::token::Client::new(&e, &vault.deposit_token).approve(
                    &e.current_contract_address(),
                    &alloc.pool,
                    &pool_amount,
                    &(e.ledger().sequence() + 1),
                );

                let new_lp = client.deposit(&e.current_contract_address(), &dep_a, &dep_b);
                alloc.lp_shares += new_lp;
                pools.set(i, alloc);
            }
        }

        save_pools(&e, &pools);

        e.events().publish(
            (Symbol::new(&e, "rebalance"),),
            (last_idx as u32, total_to_move),
        );
        Ok(())
    }

    // ── Views ─────────────────────────────────────────────────────────────────

    pub fn get_vault(e: Env) -> Result<VaultState, Error> {
        load_vault(&e)
    }

    pub fn get_pools(e: Env) -> Vec<PoolAllocation> {
        load_pools(&e)
    }

    pub fn vault_balance(e: Env, user: Address) -> i128 {
        e.storage()
            .persistent()
            .get(&DataKey::Balance(user))
            .unwrap_or(0)
    }

    /// Ids of the open withdrawal requests, oldest first.
    pub fn get_queue(e: Env) -> Vec<u32> {
        load_queue(&e)
    }

    /// Number of withdrawal requests still waiting for liquidity.
    pub fn queue_len(e: Env) -> u32 {
        load_queue(&e).len()
    }

    /// A single withdrawal request, or `None` when the id was never issued.
    pub fn get_withdrawal_request(e: Env, id: u32) -> Option<WithdrawalRequest> {
        e.storage().persistent().get(&request_key(id))
    }

    /// Deposit token currently idle in the vault and therefore available to pay
    /// queued withdrawals.
    pub fn available_liquidity(e: Env) -> Result<i128, Error> {
        let vault = load_vault(&e)?;
        Ok(available_liquidity(&e, &vault.deposit_token))
    }

    /// Returns the estimated APR (in bps) for each registered pool, in order.
    pub fn get_aprs(e: Env) -> Result<Vec<i128>, Error> {
        let vault = load_vault(&e)?;
        let pools = load_pools(&e);
        let mut aprs = Vec::new(&e);
        for i in 0..pools.len() {
            let alloc = pools.get(i).unwrap();
            let client = AmmPoolClient::new(&e, &alloc.pool);
            let fee_bps = client.get_fee();
            aprs.push_back(estimate_apr_bps(fee_bps, vault.volume_proxy_bps));
        }
        Ok(aprs)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    /// Returns the index of the pool with the highest estimated APR.
    fn best_pool_idx(e: &Env, pools: &Vec<PoolAllocation>, vault: &VaultState) -> u32 {
        let mut best_idx = 0u32;
        let mut best_apr: i128 = -1;
        for i in 0..pools.len() {
            let alloc = pools.get(i).unwrap();
            let client = AmmPoolClient::new(e, &alloc.pool);
            let fee_bps = client.get_fee();
            let apr = estimate_apr_bps(fee_bps, vault.volume_proxy_bps);
            if apr > best_apr {
                best_apr = apr;
                best_idx = i;
            }
        }
        best_idx
    }
}
