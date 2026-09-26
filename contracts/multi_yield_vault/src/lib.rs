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
//!
//! ## Withdrawal schedules
//! A depositor who wants a periodic income stream can stand up a
//! `WithdrawalSchedule` instead of watching for each payout.  One-time and
//! recurring schedules are both supported, and each execution pays a fixed
//! amount of shares to a chosen recipient.
//!
//! The shares covering every remaining execution are **reserved at creation**:
//! they stay in the owner's balance and in `total_shares`, so the LP backing
//! them is still accounted for, but `withdraw` refuses to touch them.
//! `cancel_withdrawal_schedule` releases whatever has not fired.  Because the
//! reservation is exact, a recurring schedule declares how long it runs rather
//! than running open-ended.
//!
//! Soroban has no scheduler, so "automatic" means permissionless:
//! `execute_scheduled_withdrawals()` is callable by anyone and settles every
//! schedule that has come due, in the same keeper-driven spirit as
//! `process_withdraw_queue()`.

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
    /// The schedule parameters are unusable (non-positive amount, a first
    /// execution that is not in the future, a zero interval, or an end ledger
    /// before the first execution).
    InvalidSchedule = 14,
    /// No schedule exists for the given id.
    ScheduleNotFound = 15,
    /// The schedule has been cancelled or has already run to completion.
    ScheduleClosed = 16,
    /// The schedule list is full; no new schedule could be recorded.
    TooManySchedules = 17,
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
    /// Deposit-token amount currently owed across all outstanding withdrawal
    /// queue entries.  Kept in lockstep with the `WithdrawalRequest` entries so
    /// the vault can report its total unfunded withdrawal liability.
    pub queued_amount: i128,
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
    /// Vec<u32> — ids of the open withdrawal schedules, in creation order.
    Schedules,
    /// Id to hand out to the next withdrawal schedule.
    NextScheduleId,
    /// WithdrawalSchedule by id.
    Schedule(u32),
    /// Deposit-token amount still owed to `owner` across all their schedules.
    ///
    /// These shares are *reserved*, not removed: they stay in the owner's
    /// balance and in `total_shares` so the LP backing them is still accounted
    /// for, but `withdraw` refuses to touch them until a schedule fires or is
    /// cancelled.  Deducting them from `total_shares` up front instead would
    /// leave the LP unbacked and make each execution over-redeem from the pools.
    Escrowed(Address),
}

/// How often a [`WithdrawalSchedule`] fires.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScheduleFrequency {
    /// Fires exactly once, on `next_execution_ledger`.
    OneTime,
    /// Fires every `interval_ledgers` until `end_ledger`.
    Recurring,
}

/// A standing instruction to withdraw a fixed amount from the vault on a
/// timetable, so a depositor can turn a balance into a periodic income stream
/// without having to be online when each withdrawal falls due.
///
/// ## Reservation
/// The shares backing **every** remaining execution are *reserved* when the
/// schedule is created: the owner's withdrawable balance drops by the full
/// escrow, and cancelling restores whatever has not fired yet. Two consequences
/// matter:
///
/// - execution cannot fail for want of funds, because the shares are already
///   committed — a schedule that was affordable when it was created stays
///   payable even if the owner later withdraws everything else;
/// - those shares are not spendable, so they cannot be double-spent.
///
/// The shares stay in the owner's balance and in `total_shares` — they are
/// reserved, not burned — so the LP backing them remains accounted for and each
/// execution redeems exactly what an ordinary `withdraw` of the same size
/// would. Only `withdraw` is held off them.
///
/// Because the reservation is exact, a recurring schedule must state how long it
/// runs (`end_ledger`); there is no open-ended variant, which would make the
/// amount to reserve unknowable at creation time.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalSchedule {
    /// Schedule id, unique and monotonic.
    pub id: u32,
    /// Who created the schedule and may cancel it.
    pub owner: Address,
    /// Where the withdrawn deposit token is sent each time the schedule fires.
    pub recipient: Address,
    /// Shares burned per execution. Shares are minted 1:1 with the deposit
    /// token, so this is also the token amount per execution.
    pub amount_per_execution: i128,
    pub frequency: ScheduleFrequency,
    /// Ledgers between executions. Ignored for `OneTime`.
    pub interval_ledgers: u32,
    /// Earliest ledger on which this schedule may fire.
    pub next_execution_ledger: u32,
    /// Last ledger on which this schedule may fire.
    pub end_ledger: u32,
    /// Total executions this schedule will ever perform. Fixed at creation,
    /// because it determines the escrow.
    pub total_executions: u32,
    /// Executions performed so far.
    pub executions: u32,
    /// True once cancelled or once `total_executions` have been performed.
    pub closed: bool,
}

/// Summary returned by `execute_scheduled_withdrawals`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleExecutionResult {
    /// Schedules inspected during this call.
    pub considered: u32,
    /// Schedules that fired.
    pub executed: u32,
    /// Deposit token paid out across all executions.
    pub paid_out: i128,
    /// Schedules still open when the call returned.
    pub remaining: u32,
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

/// Hard ceiling on open withdrawal schedules.  Bounds both `persistent` storage
/// growth and the work `execute_scheduled_withdrawals` can be asked to do in a
/// single call.
pub const MAX_SCHEDULES: u32 = 64;

/// Upper bound on how many times one schedule may fire.  Together with the
/// owner's balance this caps the escrowed amount, so a schedule can never lock up
/// an unbounded share of the vault.
pub const MAX_SCHEDULE_EXECUTIONS: u32 = 1_000;

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
///
/// Adds `requested` to the vault's `queued_amount` liability so the counter
/// stays in lockstep with the queue entries it is supposed to summarise.
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

    if let Ok(mut vault) = load_vault(e) {
        vault.queued_amount += requested;
        save_vault(e, &vault);
    }
    Ok(id)
}

/// Reduce the vault's `queued_amount` liability by `amount`, never below zero.
fn reduce_queued(e: &Env, amount: i128) {
    if amount <= 0 {
        return;
    }
    if let Ok(mut vault) = load_vault(e) {
        vault.queued_amount = (vault.queued_amount - amount).max(0);
        save_vault(e, &vault);
    }
}

// ── Withdrawal schedule helpers ───────────────────────────────────────────────

fn load_schedules(e: &Env) -> Vec<u32> {
    e.storage()
        .instance()
        .get(&DataKey::Schedules)
        .unwrap_or(Vec::new(e))
}

fn save_schedules(e: &Env, schedules: &Vec<u32>) {
    e.storage().instance().set(&DataKey::Schedules, schedules);
}

fn schedule_key(id: u32) -> DataKey {
    DataKey::Schedule(id)
}

fn load_schedule(e: &Env, id: u32) -> Result<WithdrawalSchedule, Error> {
    e.storage()
        .persistent()
        .get(&schedule_key(id))
        .ok_or(Error::ScheduleNotFound)
}

fn save_schedule(e: &Env, id: u32, schedule: &WithdrawalSchedule) {
    let key = schedule_key(id);
    e.storage().persistent().set(&key, schedule);
    e.storage()
        .persistent()
        .extend_ttl(&key, TTL_LEDGERS, TTL_LEDGERS);
}

/// Drop a schedule id from the open-schedule list.
fn remove_from_schedules(e: &Env, id: u32) {
    let mut schedules = load_schedules(e);
    for i in 0..schedules.len() {
        if schedules.get(i).unwrap() == id {
            schedules.remove(i);
            break;
        }
    }
    save_schedules(e, &schedules);
}

/// Shares `owner` has committed to their open withdrawal schedules.  These stay
/// in the owner's balance but are not withdrawable.
fn escrowed_of(e: &Env, owner: &Address) -> i128 {
    e.storage()
        .persistent()
        .get(&DataKey::Escrowed(owner.clone()))
        .unwrap_or(0)
}

/// Change `owner`'s reserved total by `delta` (negative to release).
fn adjust_escrowed(e: &Env, owner: &Address, delta: i128) {
    let key = DataKey::Escrowed(owner.clone());
    let next = (escrowed_of(e, owner) + delta).max(0);
    e.storage().persistent().set(&key, &next);
    e.storage()
        .persistent()
        .extend_ttl(&key, TTL_LEDGERS, TTL_LEDGERS);
}

/// Number of executions implied by the timing parameters, and the escrow they
/// require.  Returns `Err` for unusable combinations.
fn plan_executions(
    amount_per_execution: i128,
    frequency: &ScheduleFrequency,
    first_execution_ledger: u32,
    interval_ledgers: u32,
    end_ledger: u32,
) -> Result<(u32, i128), Error> {
    if amount_per_execution <= 0 {
        return Err(Error::InvalidAmount);
    }

    let total_executions = match frequency {
        ScheduleFrequency::OneTime => 1u32,
        ScheduleFrequency::Recurring => {
            if interval_ledgers == 0 {
                return Err(Error::InvalidSchedule);
            }
            if end_ledger < first_execution_ledger {
                return Err(Error::InvalidSchedule);
            }
            // Firing on `first`, `first + interval`, ... up to and including `end`.
            ((end_ledger - first_execution_ledger) / interval_ledgers) + 1
        }
    };

    if total_executions == 0 || total_executions > MAX_SCHEDULE_EXECUTIONS {
        return Err(Error::InvalidSchedule);
    }

    let escrow = amount_per_execution
        .checked_mul(total_executions as i128)
        .ok_or(Error::InvalidAmount)?;

    Ok((total_executions, escrow))
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
                queued_amount: 0,
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
        let token = soroban_sdk::token::Client::new(&e, &vault.deposit_token);

        let bal_key = DataKey::Balance(to.clone());
        let cur: i128 = e.storage().persistent().get(&bal_key).unwrap_or(0);
        // Shares reserved for an open withdrawal schedule are not spendable, or
        // the owner could withdraw them and leave the schedule unpayable.
        let spendable = cur - escrowed_of(&e, &to);
        if shares > spendable {
            return Err(Error::InsufficientShares);
        }

        // Deposit token already idle in the vault counts towards what this
        // withdrawal can be settled with right now.
        let idle_before = token.balance(&e.current_contract_address());

        let mut pools = load_pools(&e);
        let mut total_received: i128 = 0;

        for i in 0..pools.len() {
            let mut alloc = pools.get(i).unwrap();
            if alloc.lp_shares == 0 {
                continue;
            }
            // Withdraw proportional LP shares from this pool.
            let lp_to_burn = alloc.lp_shares * shares / vault.total_shares;
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
            reduce_queued(&e, payout);
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
        // The unpaid remainder is written off, so it leaves the liability too.
        let written_off = req.remaining_shares;
        req.remaining_shares = 0;
        req.closed = true;
        reduce_queued(&e, written_off);
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

    // ── Withdrawal schedules ──────────────────────────────────────────────────

    /// Create a standing instruction to withdraw `amount_per_execution` shares
    /// on a timetable, paying each payout to `recipient`.
    ///
    /// The shares backing **every** execution are escrowed now, so the schedule
    /// cannot fail later for want of funds and the escrowed shares cannot be
    /// spent twice. `vault_balance` drops by the full escrow immediately, and
    /// cancelling returns whatever has not fired yet.
    ///
    /// - `frequency`: `OneTime` fires a single time on `first_execution_ledger`;
    ///   `Recurring` then fires every `interval_ledgers` up to and including
    ///   `end_ledger`.
    /// - `interval_ledgers` is ignored for `OneTime`, and must be non-zero for
    ///   `Recurring`.
    /// - `end_ledger` is ignored for `OneTime`, and must be at or after
    ///   `first_execution_ledger` for `Recurring`.
    ///
    /// Returns the new schedule id.
    pub fn schedule_withdrawals(
        e: Env,
        owner: Address,
        recipient: Address,
        amount_per_execution: i128,
        frequency: ScheduleFrequency,
        first_execution_ledger: u32,
        interval_ledgers: u32,
        end_ledger: u32,
    ) -> Result<u32, Error> {
        owner.require_auth();
        load_vault(&e)?;

        // The first execution has to be in the future, otherwise the schedule
        // would be due before it could ever be observed.
        if first_execution_ledger <= e.ledger().sequence() {
            return Err(Error::InvalidSchedule);
        }

        let (total_executions, escrow) = plan_executions(
            amount_per_execution,
            &frequency,
            first_execution_ledger,
            interval_ledgers,
            end_ledger,
        )?;

        // The escrow must fit in what the owner holds *and* has not already
        // promised to another schedule.
        let cur: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::Balance(owner.clone()))
            .unwrap_or(0);
        let spendable = cur - escrowed_of(&e, &owner);
        if escrow > spendable {
            return Err(Error::InsufficientShares);
        }

        let mut schedules = load_schedules(&e);
        if schedules.len() >= MAX_SCHEDULES {
            return Err(Error::TooManySchedules);
        }

        // A one-time schedule has exactly one due ledger, so record it as the
        // end too.  Otherwise `end_ledger` would stay 0 and the due-check
        // ("not past the end") would treat the schedule as already expired.
        let effective_end_ledger = match frequency {
            ScheduleFrequency::OneTime => first_execution_ledger,
            ScheduleFrequency::Recurring => end_ledger,
        };

        let id: u32 = e
            .storage()
            .instance()
            .get(&DataKey::NextScheduleId)
            .unwrap_or(0);
        e.storage()
            .instance()
            .set(&DataKey::NextScheduleId, &(id + 1));

        let schedule = WithdrawalSchedule {
            id,
            owner: owner.clone(),
            recipient: recipient.clone(),
            amount_per_execution,
            frequency: frequency.clone(),
            interval_ledgers,
            next_execution_ledger: first_execution_ledger,
            end_ledger: effective_end_ledger,
            total_executions,
            executions: 0,
            closed: false,
        };
        save_schedule(&e, id, &schedule);
        schedules.push_back(id);
        save_schedules(&e, &schedules);

        // Reserve the shares. They stay in the owner's balance and in
        // `total_shares`; only `withdraw` is held off them.
        adjust_escrowed(&e, &owner, escrow);
        e.events().publish(
            (Symbol::new(&e, "withdrawal_scheduled"), owner, id),
            (
                recipient,
                amount_per_execution,
                first_execution_ledger,
                interval_ledgers,
                total_executions,
                frequency,
            ),
        );

        Ok(id)
    }

    /// Fire every schedule that is due, paying each one to its recipient.
    ///
    /// Soroban has no scheduler, so "automatic" means permissionless: anyone may
    /// call this, and it settles every schedule whose `next_execution_ledger`
    /// has arrived. Each schedule fires at most once per call, so a keeper that
    /// calls once per ledger keeps a schedule on time.
    ///
    /// A due schedule cannot fail for want of funds — its shares were escrowed
    /// at creation — so the worst case is a partial token payout, which falls
    /// back to the ordinary withdrawal queue exactly as `withdraw` does.
    ///
    /// `max_schedules` bounds how many are inspected per call; 0 means
    /// `MAX_SCHEDULES`.
    pub fn execute_scheduled_withdrawals(
        e: Env,
        max_schedules: u32,
    ) -> Result<ScheduleExecutionResult, Error> {
        let deposit_token = load_vault(&e)?.deposit_token;
        let budget = if max_schedules == 0 {
            MAX_SCHEDULES
        } else {
            max_schedules.min(MAX_SCHEDULES)
        };

        let mut schedules = load_schedules(&e);
        let mut result = ScheduleExecutionResult {
            considered: 0,
            executed: 0,
            paid_out: 0,
            remaining: schedules.len(),
        };
        if schedules.len() == 0 {
            return Ok(result);
        }

        let now = e.ledger().sequence();
        let mut i = 0u32;
        // Walk in creation order and compact the list as schedules close, so a
        // finished schedule does not linger and cost gas forever.
        while i < schedules.len() && result.considered < budget {
            let id = schedules.get(i).unwrap();
            let mut schedule = load_schedule(&e, id)?;
            result.considered += 1;

            if schedule.closed || schedule.executions >= schedule.total_executions {
                schedule.closed = true;
                save_schedule(&e, id, &schedule);
                schedules.remove(i);
                continue;
            }

            if now < schedule.next_execution_ledger || now > schedule.end_ledger {
                i += 1;
                continue;
            }

            // Reloaded each round: the queue entry written below for a partial
            // execution updates the vault, so a cached copy would go stale.
            let mut vault = load_vault(&e)?;
            let amount = schedule.amount_per_execution;

            // Defensive.  The reservation is validated against the balance when
            // the schedule is created and `withdraw` is held off the reserved
            // shares, so the owner must still hold this tranche.  Checked before
            // any LP is redeemed: skipping here leaves the schedule intact for a
            // later call instead of stranding half-redeemed positions.
            let owner_key = DataKey::Balance(schedule.owner.clone());
            let owner_balance: i128 = e.storage().persistent().get(&owner_key).unwrap_or(0);
            if owner_balance < amount {
                i += 1;
                continue;
            }

            // Redeem this execution's reserved shares across the pools. Because
            // they were only reserved — never removed from `total_shares` — this
            // is exactly the redemption an ordinary `withdraw` of the same size
            // would perform.
            let mut pools = load_pools(&e);
            let mut total_received: i128 = 0;
            for p in 0..pools.len() {
                let mut alloc = pools.get(p).unwrap();
                if alloc.lp_shares == 0 || vault.total_shares == 0 {
                    continue;
                }
                let lp_to_burn = alloc.lp_shares * amount / vault.total_shares;
                if lp_to_burn == 0 {
                    continue;
                }
                let client = AmmPoolClient::new(&e, &alloc.pool);
                let (out_a, out_b) = client.withdraw(&e.current_contract_address(), &lp_to_burn);
                total_received += if alloc.deposit_is_a { out_a } else { out_b };
                alloc.lp_shares -= lp_to_burn;
                pools.set(p, alloc);
            }
            save_pools(&e, &pools);

            // Spend the reservation and burn the shares, exactly as `withdraw`
            // would have for the owner.
            adjust_escrowed(&e, &schedule.owner, -amount);
            e.storage()
                .persistent()
                .set(&owner_key, &(owner_balance - amount));
            e.storage()
                .persistent()
                .extend_ttl(&owner_key, TTL_LEDGERS, TTL_LEDGERS);
            vault.total_shares -= amount;
            save_vault(&e, &vault);

            let payout = pay_out(
                &e,
                &deposit_token,
                &schedule.recipient,
                total_received.min(amount),
            );
            let remainder = amount - payout;

            if remainder > 0 {
                // The pools could not cover this execution in full. The unpaid
                // part rides the same queue an ordinary partial withdrawal uses.
                enqueue_request(&e, &schedule.recipient, remainder)?;
            }

            schedule.executions += 1;
            let finished = schedule.executions >= schedule.total_executions;
            if finished {
                schedule.closed = true;
            } else {
                schedule.next_execution_ledger = schedule
                    .next_execution_ledger
                    .saturating_add(schedule.interval_ledgers);
            }
            save_schedule(&e, id, &schedule);

            result.executed += 1;
            result.paid_out += payout;

            e.events().publish(
                (Symbol::new(&e, "scheduled_withdrawal_executed"), id),
                (
                    schedule.owner,
                    schedule.recipient,
                    amount,
                    payout,
                    schedule.executions,
                    schedule.total_executions,
                ),
            );

            if finished {
                schedules.remove(i);
            } else {
                i += 1;
            }
        }

        save_schedules(&e, &schedules);
        result.remaining = schedules.len();
        Ok(result)
    }

    /// Cancel a schedule and release the shares it had reserved but not yet paid
    /// out.  Only the owner may cancel, and only while the schedule is still
    /// open — once every execution has fired there is nothing left to release.
    pub fn cancel_withdrawal_schedule(e: Env, owner: Address, id: u32) -> Result<(), Error> {
        owner.require_auth();
        let mut schedule = load_schedule(&e, id)?;
        if schedule.owner != owner {
            return Err(Error::Unauthorized);
        }
        if schedule.closed || schedule.executions >= schedule.total_executions {
            return Err(Error::ScheduleClosed);
        }

        let remaining_executions = schedule.total_executions - schedule.executions;
        let released = schedule
            .amount_per_execution
            .checked_mul(remaining_executions as i128)
            .ok_or(Error::InvalidAmount)?;

        schedule.closed = true;
        save_schedule(&e, id, &schedule);
        remove_from_schedules(&e, id);

        // The shares were only ever reserved, so releasing them is a matter of
        // making them withdrawable again — no balance movement needed.
        adjust_escrowed(&e, &owner, -released);

        e.events().publish(
            (Symbol::new(&e, "withdrawal_schedule_cancelled"), id),
            (owner, released, remaining_executions),
        );
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

    /// A single withdrawal schedule, or `None` when the id was never issued.
    pub fn get_withdrawal_schedule(e: Env, id: u32) -> Option<WithdrawalSchedule> {
        e.storage().persistent().get(&schedule_key(id))
    }

    /// Ids of the open withdrawal schedules, in creation order.
    pub fn get_withdrawal_schedules(e: Env) -> Vec<u32> {
        load_schedules(&e)
    }

    /// How many withdrawal schedules are still open.
    pub fn schedule_count(e: Env) -> u32 {
        load_schedules(&e).len()
    }

    /// Shares `user` has reserved across all their open withdrawal schedules.
    ///
    /// Included in `vault_balance` but not withdrawable until the schedule fires
    /// or is cancelled.
    pub fn escrowed_shares(e: Env, user: Address) -> i128 {
        escrowed_of(&e, &user)
    }

    /// Ids of the schedules due to fire on `ledger`, in creation order.
    ///
    /// Lets a keeper decide whether a call to `execute_scheduled_withdrawals`
    /// would do anything, without paying to execute it speculatively.
    pub fn due_schedules(e: Env, ledger: u32) -> Vec<u32> {
        let schedules = load_schedules(&e);
        let mut due = Vec::new(&e);
        for i in 0..schedules.len() {
            let id = schedules.get(i).unwrap();
            if let Ok(schedule) = load_schedule(&e, id) {
                if !schedule.closed
                    && schedule.executions < schedule.total_executions
                    && ledger >= schedule.next_execution_ledger
                    && ledger <= schedule.end_ledger
                {
                    due.push_back(id);
                }
            }
        }
        due
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
