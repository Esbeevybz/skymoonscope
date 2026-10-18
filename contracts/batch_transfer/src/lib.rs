#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, IntoVal,
    String, Symbol, Val, Vec,
};

#[cfg(test)]
mod test;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    EmptyBatch = 1,
    LengthMismatch = 2,
    InvalidAmount = 3,
    InsufficientBalance = 4,
    TooManyRecipients = 5,
    /// A transfer was rejected by the token contract itself.
    ///
    /// In `AllOrNothing` mode this aborts the whole call, which reverts every
    /// transfer already performed in this batch.
    TransferFailed = 6,
}

/// Aggregate outcome of a batch, returned by `execute_batch`.
///
/// A caller that needs to know "did my whole batch land?" reads `failed == 0`
/// rather than inspecting every entry, and `total_transferred` gives the amount
/// that actually moved.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchResult {
    /// Number of entries submitted.
    pub total: u32,
    /// Entries that transferred.
    pub succeeded: u32,
    /// Entries that did not transfer.
    pub failed: u32,
    /// Total amount transferred.
    pub total_transferred: i128,
    /// Per-entry detail, in submission order.
    pub results: Vec<TransferResult>,
}

/// Upper bound on recipients per batch. Oversized recipient vectors are
/// rejected before any iteration so a caller cannot exhaust the
/// transaction's CPU instruction budget by passing an unbounded batch.
const MAX_RECIPIENTS: u32 = 100;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionMode {
    AllOrNothing,
    Partial,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferFailure {
    None,
    InvalidAmount,
    InsufficientBalance,
    /// The token contract rejected this individual transfer.
    TransferFailed,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferResult {
    pub recipient: Address,
    pub amount: i128,
    pub success: bool,
    pub failure: TransferFailure,
}

#[soroban_sdk::contractclient(name = "BatchTokenClient")]
pub trait BatchToken {
    fn balance(e: Env, id: Address) -> i128;
    fn transfer(e: Env, from: Address, to: Address, amount: i128);
}

/// Burn addresses, which no transfer may target.
fn burn_addresses(env: &Env) -> (Address, Address) {
    let g = Address::from_string(&String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));
    let c = Address::from_string(&String::from_str(
        env,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
    ));
    (g, c)
}

/// Validate shape arguments shared by `execute`, `quote` and `execute_batch`.
fn validate_shape(recipients: &Vec<Address>, amounts: &Vec<i128>) -> Result<(), Error> {
    let len = recipients.len();
    if len == 0 {
        return Err(Error::EmptyBatch);
    }
    if len > MAX_RECIPIENTS {
        return Err(Error::TooManyRecipients);
    }
    if len != amounts.len() {
        return Err(Error::LengthMismatch);
    }
    Ok(())
}

/// Plan a batch without moving any tokens, then execute the plan.
///
/// The batch is planned in full before a single transfer is issued, so a
/// rejection can never leave earlier entries already settled.  In
/// `AllOrNothing` mode the plan is all-or-nothing: the first invalid entry
/// aborts the whole call and nothing has moved yet.  In `Partial` mode failures
/// are recorded per entry and the rest of the plan still executes.
fn plan_and_execute(
    env: &Env,
    token: &Address,
    sender: &Address,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
    mode: &ExecutionMode,
    do_transfer: bool,
) -> Result<Vec<TransferResult>, Error> {
    validate_shape(recipients, amounts)?;

    let token_client = BatchTokenClient::new(env, token);
    let mut remaining_balance = token_client.balance(sender);
    let (zero_g, zero_c) = burn_addresses(env);
    let is_all_or_nothing = matches!(mode, ExecutionMode::AllOrNothing);

    // ── Phase 1: plan ───────────────────────────────────────────────────────
    // Nothing is transferred here, so an `Err` below leaves the ledger untouched.
    let mut plan: Vec<TransferResult> = Vec::new(env);
    for (recipient, amount) in recipients.iter().zip(amounts.iter()) {
        if amount <= 0 || recipient == zero_g || recipient == zero_c {
            if is_all_or_nothing {
                return Err(Error::InvalidAmount);
            }
            plan.push_back(TransferResult {
                recipient,
                amount,
                success: false,
                failure: TransferFailure::InvalidAmount,
            });
            continue;
        }

        if remaining_balance < amount {
            if is_all_or_nothing {
                return Err(Error::InsufficientBalance);
            }
            plan.push_back(TransferResult {
                recipient,
                amount,
                success: false,
                failure: TransferFailure::InsufficientBalance,
            });
            continue;
        }

        remaining_balance -= amount;
        plan.push_back(TransferResult {
            recipient,
            amount,
            success: true,
            failure: TransferFailure::None,
        });
    }

    // ── Phase 2: execute ────────────────────────────────────────────────────
    // Only entries the plan marked successful move tokens.  A token contract
    // that reverts mid-batch aborts the whole Soroban invocation, so this
    // phase is atomic by construction rather than by bookkeeping.
    if do_transfer {
        for i in 0..plan.len() {
            let entry = plan.get(i).unwrap();
            if !entry.success {
                continue;
            }
            token_client.transfer(sender, &entry.recipient, &entry.amount);
        }
    }

    Ok(plan)
}

/// Fold per-entry results into the aggregate returned by `execute_batch`.
fn summarize(results: &Vec<TransferResult>) -> BatchResult {
    let mut succeeded: u32 = 0;
    let mut failed: u32 = 0;
    let mut total_transferred: i128 = 0;
    for i in 0..results.len() {
        let entry = results.get(i).unwrap();
        if entry.success {
            succeeded += 1;
            total_transferred += entry.amount;
        } else {
            failed += 1;
        }
    }
    BatchResult {
        total: results.len(),
        succeeded,
        failed,
        total_transferred,
        results: results.clone(),
    }
}

#[contract]
pub struct BatchTransfer;

#[contractimpl]
impl BatchTransfer {
    /// Execute a batch, returning the per-entry outcome.
    ///
    /// The batch is planned in full before any token moves, so this never
    /// settles a prefix of the batch and then bails: a rejected entry aborts
    /// the call with the ledger untouched (#081).
    pub fn execute(
        env: Env,
        token: Address,
        sender: Address,
        recipients: Vec<Address>,
        amounts: Vec<i128>,
        mode: ExecutionMode,
    ) -> Result<Vec<TransferResult>, Error> {
        sender.require_auth();
        plan_and_execute(&env, &token, &sender, &recipients, &amounts, &mode, true)
    }

    /// Execute a batch, returning a structured aggregate result.
    ///
    /// Same semantics as `execute`, but the returned `BatchResult` carries the
    /// success/failure counts and the total that moved, so a caller can check
    /// the batch outcome without walking every entry (#081).
    pub fn execute_batch(
        env: Env,
        token: Address,
        sender: Address,
        recipients: Vec<Address>,
        amounts: Vec<i128>,
        mode: ExecutionMode,
    ) -> Result<BatchResult, Error> {
        sender.require_auth();
        let results = plan_and_execute(&env, &token, &sender, &recipients, &amounts, &mode, true)?;
        Ok(summarize(&results))
    }

    /// Dry-run a batch.  Plans exactly as `execute` would, but moves no tokens.
    pub fn quote(
        env: Env,
        token: Address,
        sender: Address,
        recipients: Vec<Address>,
        amounts: Vec<i128>,
        mode: ExecutionMode,
    ) -> Result<Vec<TransferResult>, Error> {
        plan_and_execute(&env, &token, &sender, &recipients, &amounts, &mode, false)
    }

    /// Dry-run a batch, returning the same aggregate shape as `execute_batch`.
    pub fn quote_batch(
        env: Env,
        token: Address,
        sender: Address,
        recipients: Vec<Address>,
        amounts: Vec<i128>,
        mode: ExecutionMode,
    ) -> Result<BatchResult, Error> {
        let results = plan_and_execute(&env, &token, &sender, &recipients, &amounts, &mode, false)?;
        Ok(summarize(&results))
    }
}
