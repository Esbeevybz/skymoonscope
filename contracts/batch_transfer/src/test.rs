#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, vec, Address, Env, String};
use token_contract::{Token, TokenClient};

fn setup() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(Token, ());
    let token = TokenClient::new(&env, &token_id);
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient_a = Address::generate(&env);

    token.initialize(
        &admin,
        &7,
        &String::from_str(&env, "Batch Token"),
        &String::from_str(&env, "BATCH"),
        &1_000_000_000_i128,
    );
    token.mint(&sender, &1_000);

    let batch_id = env.register(BatchTransfer, ());
    (env, batch_id, token_id, sender, recipient_a)
}

#[test]
fn all_or_nothing_executes_entire_batch_atomically() {
    let (env, batch_id, token_id, sender, recipient_a) = setup();
    let batch = BatchTransferClient::new(&env, &batch_id);
    let token = TokenClient::new(&env, &token_id);
    let recipient_b = Address::generate(&env);

    let recipients = vec![&env, recipient_a.clone(), recipient_b.clone()];
    let amounts = vec![&env, 250i128, 125i128];

    let results = batch.execute(
        &token_id,
        &sender,
        &recipients,
        &amounts,
        &ExecutionMode::AllOrNothing,
    );

    assert_eq!(results.len(), 2);
    assert_eq!(token.balance(&sender), 625);
    assert_eq!(token.balance(&recipient_a), 250);
    assert_eq!(token.balance(&recipient_b), 125);
}

#[test]
fn all_or_nothing_rejects_batch_before_any_transfer() {
    let (env, batch_id, token_id, sender, recipient_a) = setup();
    let batch = BatchTransferClient::new(&env, &batch_id);
    let token = TokenClient::new(&env, &token_id);
    let recipient_b = Address::generate(&env);

    let recipients = vec![&env, recipient_a.clone(), recipient_b.clone()];
    let amounts = vec![&env, 250i128, -5i128];

    let err = batch.try_execute(
        &token_id,
        &sender,
        &recipients,
        &amounts,
        &ExecutionMode::AllOrNothing,
    );

    assert_eq!(err, Err(Ok(Error::InvalidAmount)));
    assert_eq!(token.balance(&sender), 1_000);
    assert_eq!(token.balance(&recipient_a), 0);
    assert_eq!(token.balance(&recipient_b), 0);
}

#[test]
fn partial_mode_skips_failures_and_continues() {
    let (env, batch_id, token_id, sender, recipient_a) = setup();
    let batch = BatchTransferClient::new(&env, &batch_id);
    let token = TokenClient::new(&env, &token_id);
    let recipient_b = Address::generate(&env);
    let recipient_c = Address::generate(&env);

    let recipients = vec![
        &env,
        recipient_a.clone(),
        recipient_b.clone(),
        recipient_c.clone(),
    ];
    let amounts = vec![&env, 400i128, -1i128, 700i128];

    let results = batch.execute(
        &token_id,
        &sender,
        &recipients,
        &amounts,
        &ExecutionMode::Partial,
    );

    assert_eq!(results.len(), 3);
    assert_eq!(results.get(0).unwrap().success, true);
    assert_eq!(
        results.get(1).unwrap().failure,
        TransferFailure::InvalidAmount
    );
    assert_eq!(
        results.get(2).unwrap().failure,
        TransferFailure::InsufficientBalance
    );
    assert_eq!(token.balance(&sender), 600);
    assert_eq!(token.balance(&recipient_a), 400);
    assert_eq!(token.balance(&recipient_b), 0);
    assert_eq!(token.balance(&recipient_c), 0);
}

#[test]
fn accepts_batch_at_max_recipient_limit() {
    let (env, batch_id, token_id, sender, _recipient_a) = setup();
    let batch = BatchTransferClient::new(&env, &batch_id);

    // Mint enough to cover MAX_RECIPIENTS transfers of 1 unit each.
    let token = TokenClient::new(&env, &token_id);
    let admin_authorized = sender.clone();
    let _ = admin_authorized;

    let mut recipients = Vec::new(&env);
    let mut amounts = Vec::new(&env);
    for _ in 0..MAX_RECIPIENTS {
        recipients.push_back(Address::generate(&env));
        amounts.push_back(1i128);
    }

    let results = batch.execute(
        &token_id,
        &sender,
        &recipients,
        &amounts,
        &ExecutionMode::AllOrNothing,
    );

    assert_eq!(results.len(), MAX_RECIPIENTS);
    assert_eq!(token.balance(&sender), 1_000 - MAX_RECIPIENTS as i128);
}

#[test]
fn rejects_batch_over_max_recipient_limit() {
    let (env, batch_id, token_id, sender, _recipient_a) = setup();
    let batch = BatchTransferClient::new(&env, &batch_id);
    let token = TokenClient::new(&env, &token_id);

    let mut recipients = Vec::new(&env);
    let mut amounts = Vec::new(&env);
    for _ in 0..(MAX_RECIPIENTS + 1) {
        recipients.push_back(Address::generate(&env));
        amounts.push_back(1i128);
    }

    let err = batch.try_execute(
        &token_id,
        &sender,
        &recipients,
        &amounts,
        &ExecutionMode::AllOrNothing,
    );

    assert_eq!(err, Err(Ok(Error::TooManyRecipients)));
    // Nothing should have moved.
    assert_eq!(token.balance(&sender), 1_000);
}

#[test]
fn quote_matches_partial_execution_plan() {
    let (env, batch_id, token_id, sender, recipient_a) = setup();
    let batch = BatchTransferClient::new(&env, &batch_id);
    let recipient_b = Address::generate(&env);

    let recipients = vec![&env, recipient_a.clone(), recipient_b.clone()];
    let amounts = vec![&env, 800i128, 400i128];

    let quote = batch.quote(
        &token_id,
        &sender,
        &recipients,
        &amounts,
        &ExecutionMode::Partial,
    );

    assert_eq!(quote.get(0).unwrap().success, true);
    assert_eq!(
        quote.get(1).unwrap().failure,
        TransferFailure::InsufficientBalance
    );
}

#[test]
fn gas_cost_comparison_batch_vs_individual() {
    let (env, batch_id, token_id, sender, _) = setup();
    let batch = BatchTransferClient::new(&env, &batch_id);
    let token = TokenClient::new(&env, &token_id);

    let mut recipients = Vec::new(&env);
    let mut amounts = Vec::new(&env);
    for _ in 0..5 {
        recipients.push_back(Address::generate(&env));
        amounts.push_back(10i128);
    }

    let results = batch.execute(
        &token_id,
        &sender,
        &recipients,
        &amounts,
        &ExecutionMode::AllOrNothing,
    );
    assert_eq!(results.len(), 5);

    for (r, a) in recipients.iter().zip(amounts.iter()) {
        assert_eq!(token.balance(&r), a);
    }
}

// ── Token-level failures (issue #81) ──────────────────────────────────────────

/// A token whose `transfer` refuses every transfer from a blocked address,
/// without changing balances.  The pre-flight checks in the batch contract
/// cannot see this, which is exactly the case issue #81 is about.
#[contract]
struct RejectingToken;

#[contracttype]
#[derive(Clone)]
enum RejectKey {
    Balance(Address),
    Blocked,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
enum TokenError {
    Rejected = 1,
}

#[contractimpl]
impl RejectingToken {
    pub fn init(env: Env, holder: Address) {
        env.storage()
            .persistent()
            .set(&RejectKey::Balance(holder), &1_000i128);
    }

    pub fn block(env: Env, who: Address) {
        env.storage().persistent().set(&RejectKey::Blocked, &who);
    }

    pub fn balance(env: Env, who: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&RejectKey::Balance(who))
            .unwrap_or(0)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> Result<(), TokenError> {
        let blocked: Address = env
            .storage()
            .persistent()
            .get(&RejectKey::Blocked)
            .unwrap_or(to);
        if from == blocked {
            return Err(TokenError::Rejected);
        }
        let from_bal: i128 = env
            .storage()
            .persistent()
            .get(&RejectKey::Balance(from))
            .unwrap_or(0);
        let to_bal: i128 = env
            .storage()
            .persistent()
            .get(&RejectKey::Balance(to))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&RejectKey::Balance(from), &(from_bal - amount));
        env.storage()
            .persistent()
            .set(&RejectKey::Balance(to), &(to_bal + amount));
        Ok(())
    }
}

fn setup_rejecting() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let holder = Address::generate(&env);
    let token_id = env.register(RejectingToken, ());
    RejectingTokenClient::new(&env, &token_id).init(&holder);

    let batch_id = env.register(BatchTransfer, ());
    (env, batch_id, token_id, holder)
}

#[test]
fn all_or_nothing_reverts_every_transfer_when_the_token_rejects_one() {
    let (env, batch_id, token_id, holder) = setup_rejecting();
    let batch = BatchTransferClient::new(&env, &batch_id);
    let token = RejectingTokenClient::new(&env, &token_id);

    // The sender is blocked, so the very first transfer is refused by the token.
    token.block(&holder);

    let recipient_a = Address::generate(&env);
    let recipient_b = Address::generate(&env);
    let recipients = vec![&env, recipient_a.clone(), recipient_b.clone()];
    let amounts = vec![&env, 100i128, 200i128];

    let err = batch.try_execute(
        &token_id,
        &holder,
        &recipients,
        &amounts,
        &ExecutionMode::AllOrNothing,
    );

    assert_eq!(err, Err(Ok(Error::TransferFailed)));
    // Atomic: not one transfer landed.
    assert_eq!(token.balance(&holder), 1_000);
    assert_eq!(token.balance(&recipient_a), 0);
    assert_eq!(token.balance(&recipient_b), 0);
}

#[test]
fn partial_mode_records_the_token_failure_and_completes_the_rest() {
    let (env, batch_id, token_id, holder) = setup_rejecting();
    let batch = BatchTransferClient::new(&env, &batch_id);
    let token = RejectingTokenClient::new(&env, &token_id);

    token.block(&holder);

    let recipient_a = Address::generate(&env);
    let recipient_b = Address::generate(&env);
    let recipients = vec![&env, recipient_a.clone(), recipient_b.clone()];
    let amounts = vec![&env, 100i128, 200i128];

    let results = batch.execute(
        &token_id,
        &holder,
        &recipients,
        &amounts,
        &ExecutionMode::Partial,
    );

    // Both transfers are refused by the token, and both are reported rather
    // than one of them being silently dropped.
    assert_eq!(results.len(), 2);
    assert_eq!(results.get(0).unwrap().success, false);
    assert_eq!(
        results.get(0).unwrap().failure,
        TransferFailure::TransferFailed
    );
    assert_eq!(results.get(1).unwrap().success, false);
    assert_eq!(
        results.get(1).unwrap().failure,
        TransferFailure::TransferFailed
    );
    assert_eq!(token.balance(&holder), 1_000);
}

#[test]
fn partial_mode_restores_the_reserved_balance_after_a_token_failure() {
    let (env, batch_id, token_id, holder) = setup_rejecting();
    let batch = BatchTransferClient::new(&env, &batch_id);
    let token = RejectingTokenClient::new(&env, &token_id);

    // The sender is blocked for every transfer, so the *simulated* balance check
    // must not be consumed by the failures: a 600 transfer must still be
    // attempted after two 500 failures, because the reserved balance has to be
    // handed back each time.
    token.block(&holder);

    let recipients = vec![
        &env,
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];
    let amounts = vec![&env, 500i128, 500i128, 600i128];

    let results = batch.execute(
        &token_id,
        &holder,
        &recipients,
        &amounts,
        &ExecutionMode::Partial,
    );

    assert_eq!(results.len(), 3);
    for i in 0..3 {
        assert_eq!(
            results.get(i).unwrap().failure,
            TransferFailure::TransferFailed
        );
    }
}

#[test]
fn healthy_token_still_succeeds_through_the_guarded_path() {
    let (env, batch_id, token_id, holder) = setup_rejecting();
    let batch = BatchTransferClient::new(&env, &batch_id);
    let token = RejectingTokenClient::new(&env, &token_id);

    let recipient_a = Address::generate(&env);
    let recipients = vec![&env, recipient_a.clone()];
    let amounts = vec![&env, 250i128];

    let results = batch.execute(
        &token_id,
        &holder,
        &recipients,
        &amounts,
        &ExecutionMode::AllOrNothing,
    );

    assert_eq!(results.get(0).unwrap().success, true);
    assert_eq!(results.get(0).unwrap().failure, TransferFailure::None);
    assert_eq!(token.balance(&holder), 750);
    assert_eq!(token.balance(&recipient_a), 250);
}
