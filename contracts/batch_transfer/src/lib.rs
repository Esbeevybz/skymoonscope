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

/// Move `amount` from `sender` to `recipient` through the token contract,
/// returning `false` when the token rejected it instead of letting the failure
/// escape and abort the surrounding batch.
fn try_token_transfer(
    env: &Env,
    token: &Address,
    sender: &Address,
    recipient: &Address,
    amount: &i128,
) -> bool {
    let transfer = Symbol::new(env, "transfer");
    let args: Vec<Val> = Vec::from_array(
        env,
        [sender.to_val(), recipient.to_val(), amount.into_val(env)],
    );

    // `Val` is used as the success type because a SEP-41 `transfer` returns
    // nothing: any `Val` coming back means the invocation itself succeeded, so
    // this cleanly separates "the token refused" from "the token returned void".
    match env.try_invoke_contract::<Val, Error>(&token, &transfer, args) {
        Ok(Ok(_)) => true,
        // Either the token returned an error or it trapped. Both mean this
        // individual transfer did not happen.
        Ok(Err(_)) | Err(_) => false,
    }
}

fn process_batch(
    env: &Env,
    token: &Address,
    sender: &Address,
    recipients: &Vec<Address>,
    amounts: &Vec<i128>,
    mode: &ExecutionMode,
    do_transfer: bool,
) -> Result<Vec<TransferResult>, Error> {
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

    let token_client = BatchTokenClient::new(env, token);
    let mut remaining_balance = token_client.balance(sender);
    let mut results = Vec::new(env);

    let zero_address_g = Address::from_string(&String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));
    let zero_address_c = Address::from_string(&String::from_str(
        env,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
    ));

    let is_all_or_nothing = matches!(mode, ExecutionMode::AllOrNothing);

    for (recipient, amount) in recipients.iter().zip(amounts.iter()) {
        if amount <= 0 || recipient == zero_address_g || recipient == zero_address_c {
            if is_all_or_nothing {
                return Err(Error::InvalidAmount);
            }
            results.push_back(TransferResult {
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
            results.push_back(TransferResult {
                recipient,
                amount,
                success: false,
                failure: TransferFailure::InsufficientBalance,
            });
            continue;
        }

        remaining_balance -= amount;

        if do_transfer {
            // The pre-flight checks above only model what this contract can
            // observe. The token itself can still refuse the transfer (its own
            // limits, a frozen account, a non-standard implementation), and an
            // unguarded sub-call would let that failure abort the batch *after*
            // earlier recipients were already paid, losing their transfers with
            // no record of what happened (issue #81).
            //
            // Invoke through `try_invoke_contract` so the outcome is captured
            // per transfer: `AllOrNothing` turns any failure into a hard error
            // (Soroban reverts the transaction, so the batch stays atomic) and
            // `Partial` records it and carries on.
            if !try_token_transfer(env, token, sender, &recipient, &amount) {
                // Nothing moved, so hand the reserved balance back before
                // deciding what to do next.
                remaining_balance += amount;

                if is_all_or_nothing {
                    return Err(Error::TransferFailed);
                }
                results.push_back(TransferResult {
                    recipient,
                    amount,
                    success: false,
                    failure: TransferFailure::TransferFailed,
                });
                continue;
            }
        }

        results.push_back(TransferResult {
            recipient,
            amount,
            success: true,
            failure: TransferFailure::None,
        });
    }

    Ok(results)
}

#[contract]
pub struct BatchTransfer;

#[contractimpl]
impl BatchTransfer {
    pub fn execute(
        env: Env,
        token: Address,
        sender: Address,
        recipients: Vec<Address>,
        amounts: Vec<i128>,
        mode: ExecutionMode,
    ) -> Result<Vec<TransferResult>, Error> {
        sender.require_auth();
        let results = process_batch(&env, &token, &sender, &recipients, &amounts, &mode, true)?;

        // Structured summary of the batch outcome so an indexer can reconcile a
        // partially applied batch without replaying the per-item results.
        let mut succeeded: u32 = 0;
        for i in 0..results.len() {
            if results.get(i).unwrap().success {
                succeeded += 1;
            }
        }
        let total = results.len();
        env.events().publish(
            (symbol_short!("batch"), sender),
            (total, succeeded, total - succeeded),
        );

        Ok(results)
    }

    pub fn quote(
        env: Env,
        token: Address,
        sender: Address,
        recipients: Vec<Address>,
        amounts: Vec<i128>,
        mode: ExecutionMode,
    ) -> Result<Vec<TransferResult>, Error> {
        process_batch(&env, &token, &sender, &recipients, &amounts, &mode, false)
    }
}
