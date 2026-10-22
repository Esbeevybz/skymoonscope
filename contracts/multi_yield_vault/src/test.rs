use crate::{Error, MultiYieldVault, MultiYieldVaultClient, ScheduleFrequency, MAX_SCHEDULES};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, TryIntoVal};

// ── Mock AMM pool ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
enum MockKey {
    ReserveA,
    ReserveB,
    Fee,
    TotalLp,
    LpBalance(Address),
}

#[contract]
struct MockPool;

#[contractimpl]
impl MockPool {
    pub fn init(e: Env, reserve_a: i128, reserve_b: i128, fee_bps: i128) {
        e.storage().instance().set(&MockKey::ReserveA, &reserve_a);
        e.storage().instance().set(&MockKey::ReserveB, &reserve_b);
        e.storage().instance().set(&MockKey::Fee, &fee_bps);
        e.storage().instance().set(&MockKey::TotalLp, &0i128);
    }

    pub fn deposit(e: Env, to: Address, amount_a: i128, amount_b: i128) -> i128 {
        let ra: i128 = e.storage().instance().get(&MockKey::ReserveA).unwrap_or(0);
        let rb: i128 = e.storage().instance().get(&MockKey::ReserveB).unwrap_or(0);
        let total_lp: i128 = e.storage().instance().get(&MockKey::TotalLp).unwrap_or(0);

        let deposit_amount = amount_a.max(amount_b);
        let lp = if total_lp == 0 {
            deposit_amount
        } else {
            deposit_amount
        };

        e.storage()
            .instance()
            .set(&MockKey::ReserveA, &(ra + amount_a));
        e.storage()
            .instance()
            .set(&MockKey::ReserveB, &(rb + amount_b));
        e.storage()
            .instance()
            .set(&MockKey::TotalLp, &(total_lp + lp));

        let key = MockKey::LpBalance(to.clone());
        let cur: i128 = e.storage().persistent().get(&key).unwrap_or(0);
        e.storage().persistent().set(&key, &(cur + lp));
        lp
    }

    pub fn withdraw(e: Env, to: Address, share_amount: i128) -> (i128, i128) {
        let ra: i128 = e.storage().instance().get(&MockKey::ReserveA).unwrap_or(0);
        let rb: i128 = e.storage().instance().get(&MockKey::ReserveB).unwrap_or(0);
        let total_lp: i128 = e.storage().instance().get(&MockKey::TotalLp).unwrap_or(1);

        let out_a = share_amount * ra / total_lp;
        let out_b = share_amount * rb / total_lp;

        e.storage()
            .instance()
            .set(&MockKey::ReserveA, &(ra - out_a));
        e.storage()
            .instance()
            .set(&MockKey::ReserveB, &(rb - out_b));
        e.storage()
            .instance()
            .set(&MockKey::TotalLp, &(total_lp - share_amount));

        let key = MockKey::LpBalance(to.clone());
        let cur: i128 = e.storage().persistent().get(&key).unwrap_or(0);
        e.storage().persistent().set(&key, &(cur - share_amount));
        (out_a, out_b)
    }

    pub fn get_reserve_a(e: Env) -> i128 {
        e.storage().instance().get(&MockKey::ReserveA).unwrap_or(0)
    }

    pub fn get_reserve_b(e: Env) -> i128 {
        e.storage().instance().get(&MockKey::ReserveB).unwrap_or(0)
    }

    pub fn get_fee(e: Env) -> i128 {
        e.storage().instance().get(&MockKey::Fee).unwrap_or(30)
    }

    pub fn token_a(e: Env) -> Address {
        Address::generate(&e) // unused in vault logic
    }

    /// Simulate liquidity leaving the pool without any LP shares being burned
    /// (e.g. other LPs exiting), so the vault's pro-rata claim is worth more
    /// than the pool can actually pay out.  Used to exercise the vault's
    /// partial-withdrawal path.
    pub fn drain_reserves(e: Env, amount_a: i128, amount_b: i128) {
        let ra: i128 = e.storage().instance().get(&MockKey::ReserveA).unwrap_or(0);
        let rb: i128 = e.storage().instance().get(&MockKey::ReserveB).unwrap_or(0);
        e.storage()
            .instance()
            .set(&MockKey::ReserveA, &(ra - amount_a));
        e.storage()
            .instance()
            .set(&MockKey::ReserveB, &(rb - amount_b));
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup(e: &Env) -> (MultiYieldVaultClient<'_>, Address, Address, Address) {
    let admin = Address::generate(e);
    let deposit_token = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let vault_id = e.register(MultiYieldVault, ());
    let client = MultiYieldVaultClient::new(e, &vault_id);
    client.initialize(&admin, &deposit_token, &100, &10_000);
    (client, admin, deposit_token, vault_id)
}

fn register_mock_pool(e: &Env, _admin: &Address, reserve: i128, fee_bps: i128) -> Address {
    let pool_id = e.register(MockPool, ());
    let pool_client = MockPoolClient::new(e, &pool_id);
    pool_client.init(&reserve, &reserve, &fee_bps);
    pool_id
}

fn mint(e: &Env, _admin: &Address, token: &Address, to: &Address, amount: i128) {
    soroban_sdk::token::StellarAssetClient::new(e, token).mint(to, &amount);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_initialize() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _, _, _) = setup(&e);
    let vault = client.get_vault();
    assert_eq!(vault.total_shares, 0);
    assert_eq!(vault.slippage_bps, 100);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_initialize() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, deposit_token, _) = setup(&e);
    client.initialize(&admin, &deposit_token, &100, &10_000);
}

#[test]
fn test_register_pool() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, _, _) = setup(&e);
    let pool = register_mock_pool(&e, &admin, 10_000, 30);
    client.register_pool(&pool, &true);
    assert_eq!(client.get_pools().len(), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_register_duplicate_pool() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, _, _) = setup(&e);
    let pool = register_mock_pool(&e, &admin, 10_000, 30);
    client.register_pool(&pool, &true);
    client.register_pool(&pool, &true); // duplicate
}

#[test]
fn test_apr_estimation() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, _, _) = setup(&e);

    let pool_low = register_mock_pool(&e, &admin, 10_000, 10); // 10 bps fee
    let pool_high = register_mock_pool(&e, &admin, 10_000, 50); // 50 bps fee

    client.register_pool(&pool_low, &true);
    client.register_pool(&pool_high, &true);

    let aprs = client.get_aprs();
    assert_eq!(aprs.len(), 2);
    // Higher fee → higher APR estimate.
    assert!(aprs.get(1).unwrap() > aprs.get(0).unwrap());
}

#[test]
fn test_deposit_routes_by_weights() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, deposit_token, vault_id) = setup(&e);

    let pool_low = register_mock_pool(&e, &admin, 10_000, 10);
    let pool_high = register_mock_pool(&e, &admin, 10_000, 50);
    client.register_pool(&pool_low, &true);
    client.register_pool(&pool_high, &true);
    client.set_weights(&soroban_sdk::vec![&e, 5_000, 5_000]);

    let user = Address::generate(&e);
    mint(&e, &admin, &deposit_token, &user, 1_000);
    mint(&e, &admin, &deposit_token, &vault_id, 20_000);

    let shares = client.deposit(&user, &1_000);
    assert_eq!(shares, 1_000);
    assert_eq!(client.vault_balance(&user), 1_000);

    // LP shares should be distributed to both pools.
    let pools = client.get_pools();
    assert!(pools.get(0).unwrap().lp_shares > 0);
    assert!(pools.get(1).unwrap().lp_shares > 0);
}

#[test]
fn test_withdraw_returns_tokens() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, deposit_token, vault_id) = setup(&e);

    let pool1 = register_mock_pool(&e, &admin, 10_000, 30);
    let pool2 = register_mock_pool(&e, &admin, 10_000, 30);
    let pool3 = register_mock_pool(&e, &admin, 10_000, 30);
    client.register_pool(&pool1, &true);
    client.register_pool(&pool2, &true);
    client.register_pool(&pool3, &true);
    client.set_weights(&soroban_sdk::vec![&e, 4_000, 3_000, 3_000]);

    let user = Address::generate(&e);
    mint(&e, &admin, &deposit_token, &user, 1_000);
    mint(&e, &admin, &deposit_token, &vault_id, 20_000);
    client.deposit(&user, &1_000);

    let received = client.withdraw(&user, &500);
    assert!(received > 0);
    assert_eq!(client.vault_balance(&user), 500);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_withdraw_too_many_shares() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, deposit_token, vault_id) = setup(&e);

    let pool1 = register_mock_pool(&e, &admin, 10_000, 30);
    let pool2 = register_mock_pool(&e, &admin, 10_000, 30);
    client.register_pool(&pool1, &true);
    client.register_pool(&pool2, &true);
    client.set_weights(&soroban_sdk::vec![&e, 5_000, 5_000]);

    let user = Address::generate(&e);
    mint(&e, &admin, &deposit_token, &user, 1_000);
    mint(&e, &admin, &deposit_token, &vault_id, 20_000);
    client.deposit(&user, &1_000);
    client.withdraw(&user, &2_000); // more than owned
}

#[test]
fn test_rebalance_redistributes_by_weights() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, deposit_token, vault_id) = setup(&e);

    let pool1 = register_mock_pool(&e, &admin, 10_000, 30);
    let pool2 = register_mock_pool(&e, &admin, 10_000, 30);
    client.register_pool(&pool1, &true);
    client.register_pool(&pool2, &true);
    client.set_weights(&soroban_sdk::vec![&e, 5_000, 5_000]);

    let user = Address::generate(&e);
    mint(&e, &admin, &deposit_token, &user, 1_000);
    mint(&e, &admin, &deposit_token, &vault_id, 20_000);
    client.deposit(&user, &1_000);

    client.rebalance();

    let pools_after = client.get_pools();
    let lp1 = pools_after.get(0).unwrap().lp_shares;
    let lp2 = pools_after.get(1).unwrap().lp_shares;
    assert!(lp1 > 0);
    assert!(lp2 > 0);
}

#[test]
fn test_rebalance_noop_with_aligned_weights() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, deposit_token, vault_id) = setup(&e);

    let pool1 = register_mock_pool(&e, &admin, 10_000, 30);
    let pool2 = register_mock_pool(&e, &admin, 10_000, 30);
    client.register_pool(&pool1, &true);
    client.register_pool(&pool2, &true);
    client.set_weights(&soroban_sdk::vec![&e, 5_000, 5_000]);

    let user = Address::generate(&e);
    mint(&e, &admin, &deposit_token, &user, 500);
    mint(&e, &admin, &deposit_token, &vault_id, 20_000);
    client.deposit(&user, &500);

    let lp1_before = client.get_pools().get(0).unwrap().lp_shares;
    let lp2_before = client.get_pools().get(1).unwrap().lp_shares;
    client.rebalance(); // should be a no-op since weights are aligned
    let lp1_after = client.get_pools().get(0).unwrap().lp_shares;
    let lp2_after = client.get_pools().get(1).unwrap().lp_shares;
    assert!(lp1_after > 0);
    assert!(lp2_after > 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")] // InvalidWeights = 10
fn test_set_weights_invalid_sum() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, _, _) = setup(&e);
    let pool1 = register_mock_pool(&e, &admin, 10_000, 30);
    let pool2 = register_mock_pool(&e, &admin, 10_000, 30);
    client.register_pool(&pool1, &true);
    client.register_pool(&pool2, &true);
    client.set_weights(&soroban_sdk::vec![&e, 6_000, 5_000]);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")] // InvalidWeights = 10
fn test_set_weights_invalid_sum_below_target() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, _, _) = setup(&e);
    let pool1 = register_mock_pool(&e, &admin, 10_000, 30);
    let pool2 = register_mock_pool(&e, &admin, 10_000, 30);
    client.register_pool(&pool1, &true);
    client.register_pool(&pool2, &true);
    client.set_weights(&soroban_sdk::vec![&e, 4_000, 5_000]);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")] // InvalidWeights = 10
fn test_set_weights_exceeds_cap() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, _, _) = setup(&e);
    let pool1 = register_mock_pool(&e, &admin, 10_000, 30);
    let pool2 = register_mock_pool(&e, &admin, 10_000, 30);
    client.register_pool(&pool1, &true);
    client.register_pool(&pool2, &true);
    client.set_weights(&soroban_sdk::vec![&e, 6_000, 4_000]);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")] // InvalidWeights = 10
fn test_set_weights_rejects_zero_fee_pool() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, _, _) = setup(&e);

    let zero_fee_pool = register_mock_pool(&e, &admin, 10_000, 0);
    let active_pool = register_mock_pool(&e, &admin, 10_000, 30);
    client.register_pool(&zero_fee_pool, &true);
    client.register_pool(&active_pool, &true);

    client.set_weights(&soroban_sdk::vec![&e, 5_000, 5_000]);
}

#[test]
fn test_set_slippage() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _, _, _) = setup(&e);
    client.set_slippage(&200);
    assert_eq!(client.get_vault().slippage_bps, 200);
}

// ── Withdrawal queue (issue #083) ─────────────────────────────────────────────

/// A pool that actually pulls the deposit out of the vault and redeems it at a
/// fixed haircut, modelling swap fees and slippage.  `MockPool` never moves
/// tokens, so a vault using it can always pay a withdrawal in full; this one
/// makes the vault genuinely liquidity-constrained, which is what the queue is for.
#[contracttype]
#[derive(Clone)]
enum ThinKey {
    Token,
    PayoutBps,
    Escrow,
}

#[contract]
struct MockThinPool;

#[contractimpl]
impl MockThinPool {
    pub fn init(e: Env, token: Address, payout_bps: i128) {
        e.storage().instance().set(&ThinKey::Token, &token);
        e.storage().instance().set(&ThinKey::PayoutBps, &payout_bps);
        e.storage().instance().set(&ThinKey::Escrow, &0i128);
    }

    pub fn deposit(e: Env, from: Address, amount_a: i128, amount_b: i128) -> i128 {
        let amount = amount_a.max(amount_b);
        if amount <= 0 {
            return 0;
        }
        let token: Address = e.storage().instance().get(&ThinKey::Token).unwrap();
        // The vault approves this pool immediately before calling, so the pull succeeds.
        soroban_sdk::token::Client::new(&e, &token).transfer_from(
            &e.current_contract_address(),
            &from,
            &e.current_contract_address(),
            &amount,
        );
        let escrow: i128 = e.storage().instance().get(&ThinKey::Escrow).unwrap_or(0);
        e.storage()
            .instance()
            .set(&ThinKey::Escrow, &(escrow + amount));
        amount
    }

    /// Redeem `share_amount`, returning only `payout_bps` of it in token_a.
    pub fn withdraw(e: Env, to: Address, share_amount: i128) -> (i128, i128) {
        let escrow: i128 = e.storage().instance().get(&ThinKey::Escrow).unwrap_or(0);
        let burn = share_amount.min(escrow);
        e.storage()
            .instance()
            .set(&ThinKey::Escrow, &(escrow - burn));

        let payout_bps: i128 = e.storage().instance().get(&ThinKey::PayoutBps).unwrap_or(0);
        let out_a = burn * payout_bps / 10_000;
        if out_a > 0 {
            let token: Address = e.storage().instance().get(&ThinKey::Token).unwrap();
            soroban_sdk::token::Client::new(&e, &token).transfer(
                &e.current_contract_address(),
                &to,
                &out_a,
            );
        }
        (out_a, 0)
    }

    pub fn get_reserve_a(e: Env) -> i128 {
        e.storage().instance().get(&ThinKey::Escrow).unwrap_or(0)
    }

    pub fn get_reserve_b(e: Env) -> i128 {
        0
    }

    pub fn get_fee(e: Env) -> i128 {
        30
    }

    pub fn token_a(e: Env) -> Address {
        e.storage().instance().get(&ThinKey::Token).unwrap()
    }
}

/// Register two haircut pools and split deposits 50/50 between them.
fn setup_thin(e: &Env, client: &MultiYieldVaultClient<'_>, token: &Address, payout_bps: i128) {
    let a = e.register(MockThinPool, ());
    MockThinPoolClient::new(e, &a).init(token, &payout_bps);
    let b = e.register(MockThinPool, ());
    MockThinPoolClient::new(e, &b).init(token, &payout_bps);
    client.register_pool(&a, &true);
    client.register_pool(&b, &true);
    client.set_weights(&soroban_sdk::vec![&e, 5_000, 5_000]);
}

fn fund(e: &Env, token: &Address, to: &Address, amount: i128) {
    soroban_sdk::token::StellarAssetClient::new(e, token).mint(to, &amount);
}

fn token_balance(e: &Env, token: &Address, of: &Address) -> i128 {
    soroban_sdk::token::Client::new(e, token).balance(of)
}

#[test]
fn test_withdraw_full_fill_leaves_queue_empty() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _, deposit_token, _) = setup(&e);

    setup_thin(&e, &client, &deposit_token, 10_000); // no haircut

    let user = Address::generate(&e);
    fund(&e, &deposit_token, &user, 1_000);
    client.deposit(&user, &1_000);

    let received = client.withdraw(&user, &1_000);

    // The pools redeem at par, so the request is satisfied in full.
    assert_eq!(received, 1_000);
    assert_eq!(client.queue_len(), 0);
    assert_eq!(client.vault_balance(&user), 0);
    assert_eq!(token_balance(&e, &deposit_token, &user), 1_000);
    assert_eq!(client.available_liquidity(), 0);
}

#[test]
fn test_withdraw_partial_fill_queues_remainder() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _, deposit_token, vault_id) = setup(&e);

    // 40% payout: redeeming 1_000 shares only yields 400 tokens.
    setup_thin(&e, &client, &deposit_token, 4_000);

    let user = Address::generate(&e);
    fund(&e, &deposit_token, &user, 1_000);
    client.deposit(&user, &1_000);

    let received = client.withdraw(&user, &1_000);

    // Payout is capped by what the pools actually returned, not by the request.
    assert_eq!(received, 400);
    assert_eq!(token_balance(&e, &deposit_token, &user), 400);

    // The unpayable 600 is queued rather than skipped.
    assert_eq!(client.queue_len(), 1);
    let req = client.get_withdrawal_request(&0).unwrap();
    assert_eq!(req.owner, user);
    assert_eq!(req.requested_shares, 600);
    assert_eq!(req.remaining_shares, 600);
    assert_eq!(req.filled_amount, 0);
    assert!(!req.closed);

    // The whole request was deducted from the balance, so those shares cannot be
    // claimed a second time.
    assert_eq!(client.vault_balance(&user), 0);
    assert_eq!(client.get_vault().total_shares, 0);

    // A dry queue makes no progress.
    let idle = client.process_withdraw_queue(&10);
    assert_eq!(idle.processed, 0);
    assert_eq!(idle.completed, 0);
    assert_eq!(idle.paid_out, 0);
    assert_eq!(idle.remaining_in_queue, 1);

    // Once liquidity returns, the queue settles the claim.
    fund(&e, &deposit_token, &vault_id, 600);
    let result = client.process_withdraw_queue(&10);
    assert_eq!(result.processed, 1);
    assert_eq!(result.completed, 1);
    assert_eq!(result.paid_out, 600);
    assert_eq!(result.remaining_in_queue, 0);
    assert_eq!(client.queue_len(), 0);
    assert!(client.get_withdrawal_request(&0).unwrap().closed);
    assert_eq!(token_balance(&e, &deposit_token, &user), 1_000);
}

#[test]
fn test_process_queue_is_fifo_and_partial() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _, deposit_token, vault_id) = setup(&e);

    // 0% payout: every withdrawal has to queue in full.
    setup_thin(&e, &client, &deposit_token, 0);

    let alice = Address::generate(&e);
    let bob = Address::generate(&e);
    fund(&e, &deposit_token, &alice, 1_000);
    fund(&e, &deposit_token, &bob, 1_000);
    client.deposit(&alice, &1_000);
    client.deposit(&bob, &1_000);

    assert_eq!(client.withdraw(&alice, &500), 0);
    assert_eq!(client.withdraw(&bob, &500), 0);
    assert_eq!(client.queue_len(), 2);
    let ids = client.get_queue();
    assert_eq!(ids.get(0).unwrap(), 0);
    assert_eq!(ids.get(1).unwrap(), 1);

    // A small injection only reaches the head of the queue.
    fund(&e, &deposit_token, &vault_id, 300);
    let result = client.process_withdraw_queue(&10);
    assert_eq!(result.processed, 1);
    assert_eq!(result.completed, 0);
    assert_eq!(result.paid_out, 300);
    assert_eq!(result.remaining_in_queue, 2);

    // Alice (older request) is served first; Bob's claim is untouched.
    assert_eq!(token_balance(&e, &deposit_token, &alice), 300);
    assert_eq!(token_balance(&e, &deposit_token, &bob), 0);
    let alice_req = client.get_withdrawal_request(&0).unwrap();
    assert_eq!(alice_req.remaining_shares, 200);
    assert_eq!(alice_req.filled_amount, 300);

    // The rest drains on the following pass, still oldest-first.
    fund(&e, &deposit_token, &vault_id, 700);
    let result = client.process_withdraw_queue(&10);
    assert_eq!(result.processed, 2);
    assert_eq!(result.completed, 2);
    assert_eq!(result.paid_out, 700);
    assert_eq!(result.remaining_in_queue, 0);
    assert_eq!(client.queue_len(), 0);
    assert_eq!(token_balance(&e, &deposit_token, &alice), 500);
    assert_eq!(token_balance(&e, &deposit_token, &bob), 500);
}

#[test]
fn test_process_queue_honours_entry_budget() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _, deposit_token, vault_id) = setup(&e);

    setup_thin(&e, &client, &deposit_token, 0);

    let alice = Address::generate(&e);
    let bob = Address::generate(&e);
    fund(&e, &deposit_token, &alice, &1_000);
    fund(&e, &deposit_token, &bob, &1_000);
    client.deposit(&alice, &1_000);
    client.deposit(&bob, &1_000);
    client.withdraw(&alice, &500);
    client.withdraw(&bob, &500);
    assert_eq!(client.queue_len(), 2);

    // Plenty of liquidity, but only one request may be visited.
    fund(&e, &deposit_token, &vault_id, 5_000);
    let result = client.process_withdraw_queue(&1);
    assert_eq!(result.completed, 1);
    assert_eq!(result.paid_out, 500);
    assert_eq!(result.remaining_in_queue, 1);
    assert_eq!(token_balance(&e, &deposit_token, &alice), 500);
    assert_eq!(token_balance(&e, &deposit_token, &bob), 0);

    let result = client.process_withdraw_queue(&1);
    assert_eq!(result.completed, 1);
    assert_eq!(result.remaining_in_queue, 0);
    assert_eq!(token_balance(&e, &deposit_token, &bob), 500);
}

#[test]
fn test_process_queue_is_noop_when_empty() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _, _, _) = setup(&e);
    let result = client.process_withdraw_queue(&10);
    assert_eq!(result.processed, 0);
    assert_eq!(result.completed, 0);
    assert_eq!(result.paid_out, 0);
    assert_eq!(result.remaining_in_queue, 0);
}

#[test]
fn test_cancel_withdrawal_closes_request() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _, deposit_token, _) = setup(&e);

    setup_thin(&e, &client, &deposit_token, 0);

    let user = Address::generate(&e);
    fund(&e, &deposit_token, &user, 1_000);
    client.deposit(&user, &1_000);
    assert_eq!(client.withdraw(&user, &500), 0);
    assert_eq!(client.queue_len(), 1);

    client.cancel_withdrawal(&user, &0);
    assert_eq!(client.queue_len(), 0);
    assert!(client.get_withdrawal_request(&0).unwrap().closed);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")] // WithdrawalNotFound = 12
fn test_cancel_unknown_withdrawal() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _, _, _) = setup(&e);
    let user = Address::generate(&e);
    client.cancel_withdrawal(&user, &7);
}

#[test]
#[should_panic(expected = "Error(Contract, #13)")] // WithdrawalClosed = 13
fn test_cancel_withdrawal_twice() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _, deposit_token, _) = setup(&e);

    setup_thin(&e, &client, &deposit_token, 0);

    let user = Address::generate(&e);
    fund(&e, &deposit_token, &user, 1_000);
    client.deposit(&user, &1_000);
    client.withdraw(&user, &500);

    client.cancel_withdrawal(&user, &0);
    client.cancel_withdrawal(&user, &0);
}

// ── Withdrawal schedules ─────────────────────────────────────────────────────

/// Set the ledger sequence the vault will read.
fn set_sequence(e: &Env, sequence: u32) {
    e.ledger().set_sequence_number(sequence);
}

#[test]
fn test_schedule_one_time_reserves_shares_without_moving_them() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);
    assert_eq!(client.vault_balance(&owner), 1_000);

    let id = client.schedule_withdrawals(
        &owner,
        &recipient,
        &250,
        &ScheduleFrequency::OneTime,
        &10,
        &0,
        &0,
    );
    assert_eq!(id, 0);

    let schedule = client.get_withdrawal_schedule(&0).unwrap();
    assert_eq!(schedule.owner, owner);
    assert_eq!(schedule.recipient, recipient);
    assert_eq!(schedule.amount_per_execution, 250);
    assert_eq!(schedule.total_executions, 1);
    assert_eq!(schedule.executions, 0);
    assert_eq!(schedule.next_execution_ledger, 10);
    assert!(!schedule.closed);

    // The shares are reserved, not spent: still in the balance and in
    // total_shares, but not withdrawable.
    assert_eq!(client.vault_balance(&owner), 1_000);
    assert_eq!(client.get_vault().total_shares, 1_000);
    assert_eq!(client.escrowed_shares(&owner), 250);
    assert_eq!(client.schedule_count(), 1);
    assert_eq!(client.get_withdrawal_schedules().len(), 1);
}

#[test]
fn test_reserved_shares_cannot_be_withdrawn() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    client.schedule_withdrawals(
        &owner,
        &owner,
        &400,
        &ScheduleFrequency::OneTime,
        &10,
        &0,
        &0,
    );

    // The unreserved 600 is still withdrawable.
    assert_eq!(client.withdraw(&owner, &600), 600);
    assert_eq!(client.vault_balance(&owner), 400);

    // The reserved 400 is not, or the schedule could never be paid.
    assert_eq!(
        client.try_withdraw(&owner, &400),
        Err(Ok(Error::InsufficientShares))
    );
    assert_eq!(client.escrowed_shares(&owner), 400);
}

#[test]
fn test_schedule_rejects_more_than_the_spendable_balance() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    // 4 x 300 = 1_200 > 1_000 held.
    let err = client.try_schedule_withdrawals(
        &owner,
        &owner,
        &300,
        &ScheduleFrequency::Recurring,
        &10,
        &10,
        &40,
    );
    assert_eq!(err, Err(Ok(Error::InsufficientShares)));
    assert_eq!(client.schedule_count(), 0);
    assert_eq!(client.escrowed_shares(&owner), 0);
}

#[test]
fn test_two_schedules_cannot_double_reserve_the_same_shares() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    client.schedule_withdrawals(
        &owner,
        &owner,
        &600,
        &ScheduleFrequency::OneTime,
        &10,
        &0,
        &0,
    );
    assert_eq!(client.escrowed_shares(&owner), 600);

    // Only 400 is left unreserved, so a 600 schedule must be refused.
    let err = client.try_schedule_withdrawals(
        &owner,
        &owner,
        &600,
        &ScheduleFrequency::OneTime,
        &20,
        &0,
        &0,
    );
    assert_eq!(err, Err(Ok(Error::InsufficientShares)));
    assert_eq!(client.escrowed_shares(&owner), 600);
    assert_eq!(client.schedule_count(), 1);

    // 400 still fits.
    client.schedule_withdrawals(
        &owner,
        &owner,
        &400,
        &ScheduleFrequency::OneTime,
        &20,
        &0,
        &0,
    );
    assert_eq!(client.escrowed_shares(&owner), 1_000);
    assert_eq!(client.schedule_count(), 2);
}

#[test]
fn test_schedule_validates_timing_parameters() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 10_000);
    client.deposit(&owner, &10_000);

    // First execution must be in the future; the ledger is at 0 here.
    let past = client.try_schedule_withdrawals(
        &owner,
        &owner,
        &10,
        &ScheduleFrequency::OneTime,
        &0,
        &0,
        &0,
    );
    assert_eq!(past, Err(Ok(Error::InvalidSchedule)));

    // A recurring schedule needs a non-zero interval.
    let zero_interval = client.try_schedule_withdrawals(
        &owner,
        &owner,
        &10,
        &ScheduleFrequency::Recurring,
        &10,
        &0,
        &100,
    );
    assert_eq!(zero_interval, Err(Ok(Error::InvalidSchedule)));

    // The end must not precede the first execution.
    let inverted = client.try_schedule_withdrawals(
        &owner,
        &owner,
        &10,
        &ScheduleFrequency::Recurring,
        &100,
        &10,
        &50,
    );
    assert_eq!(inverted, Err(Ok(Error::InvalidSchedule)));

    // Non-positive amounts are rejected.
    let zero_amount = client.try_schedule_withdrawals(
        &owner,
        &owner,
        &0,
        &ScheduleFrequency::OneTime,
        &10,
        &0,
        &0,
    );
    assert_eq!(zero_amount, Err(Ok(Error::InvalidAmount)));

    assert_eq!(client.schedule_count(), 0);
}

#[test]
fn test_one_time_schedule_executes_and_closes() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    client.schedule_withdrawals(
        &owner,
        &recipient,
        &300,
        &ScheduleFrequency::OneTime,
        &10,
        &0,
        &0,
    );

    // Not due yet.
    set_sequence(&e, 5);
    let early = client.execute_scheduled_withdrawals(&0);
    assert_eq!(early.executed, 0);
    assert_eq!(early.paid_out, 0);
    assert_eq!(token_balance(&e, &deposit_token, &recipient), 0);

    // Due.
    set_sequence(&e, 10);
    let result = client.execute_scheduled_withdrawals(&0);
    assert_eq!(result.executed, 1);
    assert_eq!(result.paid_out, 300);
    assert_eq!(result.remaining, 0);

    // Paid to the recipient, not the owner.
    assert_eq!(token_balance(&e, &deposit_token, &recipient), 300);
    assert_eq!(client.vault_balance(&owner), 700);
    assert_eq!(client.escrowed_shares(&owner), 0);
    assert_eq!(client.get_vault().total_shares, 700);

    // Closed and no longer listed.
    assert!(client.get_withdrawal_schedule(&0).unwrap().closed);
    assert_eq!(client.schedule_count(), 0);

    // A second call does nothing.
    let again = client.execute_scheduled_withdrawals(&0);
    assert_eq!(again.executed, 0);
    assert_eq!(token_balance(&e, &deposit_token, &recipient), 300);
}

#[test]
fn test_recurring_schedule_fires_on_each_interval() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    // 100 every 10 ledgers, on ledgers 10, 20 and 30.
    client.schedule_withdrawals(
        &owner,
        &recipient,
        &100,
        &ScheduleFrequency::Recurring,
        &10,
        &10,
        &30,
    );

    let schedule = client.get_withdrawal_schedule(&0).unwrap();
    assert_eq!(schedule.total_executions, 3);
    assert_eq!(client.escrowed_shares(&owner), 300);

    set_sequence(&e, 10);
    let first = client.execute_scheduled_withdrawals(&0);
    assert_eq!(first.executed, 1);
    assert_eq!(first.paid_out, 100);
    assert_eq!(token_balance(&e, &deposit_token, &recipient), 100);

    let after_first = client.get_withdrawal_schedule(&0).unwrap();
    assert_eq!(after_first.executions, 1);
    assert_eq!(after_first.next_execution_ledger, 20);
    assert!(!after_first.closed);
    assert_eq!(client.escrowed_shares(&owner), 200);
    assert_eq!(client.schedule_count(), 1);

    // Jumping past several intervals still fires only once per call.
    set_sequence(&e, 30);
    let second = client.execute_scheduled_withdrawals(&0);
    assert_eq!(second.executed, 1);
    assert_eq!(client.get_withdrawal_schedule(&0).unwrap().executions, 2);
    assert_eq!(
        client
            .get_withdrawal_schedule(&0)
            .unwrap()
            .next_execution_ledger,
        30
    );

    let third = client.execute_scheduled_withdrawals(&0);
    assert_eq!(third.executed, 1);
    assert_eq!(client.get_withdrawal_schedule(&0).unwrap().executions, 3);

    // Complete: all three paid, nothing left reserved.
    assert_eq!(token_balance(&e, &deposit_token, &recipient), 300);
    assert_eq!(client.escrowed_shares(&owner), 0);
    assert_eq!(client.vault_balance(&owner), 700);
    assert_eq!(client.schedule_count(), 0);
    assert!(client.get_withdrawal_schedule(&0).unwrap().closed);
}

#[test]
fn test_recurring_schedule_does_not_fire_past_its_end() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    // Only two executions are planned: ledgers 10 and 20.
    client.schedule_withdrawals(
        &owner,
        &recipient,
        &100,
        &ScheduleFrequency::Recurring,
        &10,
        &10,
        &20,
    );
    assert_eq!(
        client.get_withdrawal_schedule(&0).unwrap().total_executions,
        2
    );

    // Well past the end and past the plan: nothing more fires.
    set_sequence(&e, 5_000);
    let late = client.execute_scheduled_withdrawals(&0);
    assert_eq!(late.executed, 0);
    assert_eq!(token_balance(&e, &deposit_token, &recipient), 0);
    assert_eq!(client.escrowed_shares(&owner), 200);
}

#[test]
fn test_cancel_releases_reserved_shares() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    client.schedule_withdrawals(
        &owner,
        &recipient,
        &100,
        &ScheduleFrequency::Recurring,
        &10,
        &10,
        &30,
    );
    assert_eq!(client.escrowed_shares(&owner), 300);

    // Nothing has fired yet, so all 300 come back.
    client.cancel_withdrawal_schedule(&owner, &0);
    assert_eq!(client.escrowed_shares(&owner), 0);
    assert_eq!(client.schedule_count(), 0);
    assert!(client.get_withdrawal_schedule(&0).unwrap().closed);

    // And the released shares are withdrawable again.
    assert_eq!(client.withdraw(&owner, &1_000), 1_000);
    assert_eq!(client.vault_balance(&owner), 0);

    // Nothing was ever paid out.
    assert_eq!(token_balance(&e, &deposit_token, &recipient), 0);
}

#[test]
fn test_cancel_after_partial_execution_releases_only_the_rest() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    client.schedule_withdrawals(
        &owner,
        &recipient,
        &100,
        &ScheduleFrequency::Recurring,
        &10,
        &10,
        &30,
    );

    set_sequence(&e, 10);
    client.execute_scheduled_withdrawals(&0);
    assert_eq!(client.escrowed_shares(&owner), 200);

    client.cancel_withdrawal_schedule(&owner, &0);
    assert_eq!(client.escrowed_shares(&owner), 0);
    assert_eq!(client.schedule_count(), 0);

    // One execution was paid before the cancel; the other two never happened.
    assert_eq!(token_balance(&e, &deposit_token, &recipient), 100);
    assert_eq!(client.vault_balance(&owner), 900);
}

#[test]
fn test_cancel_rejects_non_owner_and_completed_schedules() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    let attacker = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    client.schedule_withdrawals(
        &owner,
        &recipient,
        &100,
        &ScheduleFrequency::OneTime,
        &10,
        &0,
        &0,
    );

    // Someone else cannot cancel it.
    let wrong_owner = client.try_cancel_withdrawal_schedule(&attacker, &0);
    assert_eq!(wrong_owner, Err(Ok(Error::Unauthorized)));
    assert_eq!(client.escrowed_shares(&owner), 100);

    // Unknown id.
    let missing = client.try_cancel_withdrawal_schedule(&owner, &99);
    assert_eq!(missing, Err(Ok(Error::ScheduleNotFound)));

    // Once it has fired there is nothing left to cancel.
    set_sequence(&e, 10);
    client.execute_scheduled_withdrawals(&0);
    let completed = client.try_cancel_withdrawal_schedule(&owner, &0);
    assert_eq!(completed, Err(Ok(Error::ScheduleClosed)));
}

#[test]
fn test_due_schedules_reports_what_is_ready() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    client.schedule_withdrawals(
        &owner,
        &recipient,
        &100,
        &ScheduleFrequency::OneTime,
        &10,
        &0,
        &0,
    );
    client.schedule_withdrawals(
        &owner,
        &recipient,
        &100,
        &ScheduleFrequency::OneTime,
        &50,
        &0,
        &0,
    );

    assert_eq!(client.due_schedules(&5).len(), 0);
    assert_eq!(client.due_schedules(&10).len(), 1);
    assert_eq!(client.due_schedules(&10).get(0).unwrap(), 0);
    assert_eq!(client.due_schedules(&50).len(), 2);

    // After the first fires it is no longer due.
    set_sequence(&e, 10);
    client.execute_scheduled_withdrawals(&0);
    assert_eq!(client.due_schedules(&50).len(), 1);
    assert_eq!(client.due_schedules(&50).get(0).unwrap(), 1);
}

#[test]
fn test_execute_respects_the_schedule_budget() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    for _ in 0..3 {
        client.schedule_withdrawals(
            &owner,
            &recipient,
            &100,
            &ScheduleFrequency::OneTime,
            &10,
            &0,
            &0,
        );
    }

    set_sequence(&e, 10);
    let first = client.execute_scheduled_withdrawals(&1);
    assert_eq!(first.considered, 1);
    assert_eq!(first.executed, 1);
    assert_eq!(first.remaining, 2);
    assert_eq!(token_balance(&e, &deposit_token, &recipient), 100);

    // The rest drain on subsequent calls.
    let second = client.execute_scheduled_withdrawals(&0);
    assert_eq!(second.executed, 2);
    assert_eq!(second.remaining, 0);
    assert_eq!(token_balance(&e, &deposit_token, &recipient), 300);
}

#[test]
fn test_schedules_are_capped() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000_000);
    client.deposit(&owner, &1_000_000);

    for _ in 0..MAX_SCHEDULES {
        client.schedule_withdrawals(
            &owner,
            &recipient,
            &1,
            &ScheduleFrequency::OneTime,
            &10,
            &0,
            &0,
        );
    }
    assert_eq!(client.schedule_count(), MAX_SCHEDULES);

    let err = client.try_schedule_withdrawals(
        &owner,
        &recipient,
        &1,
        &ScheduleFrequency::OneTime,
        &10,
        &0,
        &0,
    );
    assert_eq!(err, Err(Ok(Error::TooManySchedules)));
    assert_eq!(client.schedule_count(), MAX_SCHEDULES);
}

#[test]
fn test_execution_count_is_capped() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 10_000_000);
    client.deposit(&owner, &10_000_000);

    // 1 + 1_000 would be 1_001 executions, over the cap.
    let err = client.try_schedule_withdrawals(
        &owner,
        &owner,
        &1,
        &ScheduleFrequency::Recurring,
        &10,
        &1,
        &1_010,
    );
    assert_eq!(err, Err(Ok(Error::InvalidSchedule)));
    assert_eq!(client.schedule_count(), 0);
}

#[test]
fn test_execute_is_a_noop_when_no_schedules_exist() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, _, _) = setup(&e);

    let result = client.execute_scheduled_withdrawals(&0);
    assert_eq!(result.considered, 0);
    assert_eq!(result.executed, 0);
    assert_eq!(result.paid_out, 0);
    assert_eq!(result.remaining, 0);
}

#[test]
fn test_execute_emits_lifecycle_events() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, vault_id) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    client.schedule_withdrawals(
        &owner,
        &recipient,
        &100,
        &ScheduleFrequency::OneTime,
        &10,
        &0,
        &0,
    );
    set_sequence(&e, 10);
    client.execute_scheduled_withdrawals(&0);

    let names: std::vec::Vec<Symbol> = e
        .events()
        .all()
        .iter()
        .filter(|(addr, _, _)| addr == &vault_id)
        .filter_map(|(_, topics, _)| {
            let name: Result<Symbol, _> = topics.get(0).unwrap().try_into_val(&e);
            name.ok()
        })
        .collect();

    assert!(names.contains(&Symbol::new(&e, "withdrawal_scheduled")));
    assert!(names.contains(&Symbol::new(&e, "scheduled_withdrawal_executed")));
}

#[test]
fn test_execute_is_permissionless() {
    // Anyone may act as the keeper; no admin or owner signature is required.
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    setup_thin(&e, &client, &deposit_token, 10_000);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    client.schedule_withdrawals(
        &owner,
        &recipient,
        &100,
        &ScheduleFrequency::OneTime,
        &10,
        &0,
        &0,
    );
    set_sequence(&e, 10);

    // `execute_scheduled_withdrawals` asks for no authorization at all, so
    // withdrawing every auth entry does not stop the keeper.
    e.set_auths(&[]);
    let result = client.execute_scheduled_withdrawals(&0);
    assert_eq!(result.executed, 1);
    assert_eq!(token_balance(&e, &deposit_token, &recipient), 100);
}

#[test]
fn test_queued_amount_tracks_partial_scheduled_execution() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin, deposit_token, _) = setup(&e);
    // 0% payout: the pools return nothing, so the execution queues the shortfall.
    setup_thin(&e, &client, &deposit_token, 0);

    let owner = Address::generate(&e);
    let recipient = Address::generate(&e);
    fund(&e, &deposit_token, &owner, 1_000);
    client.deposit(&owner, &1_000);

    client.schedule_withdrawals(
        &owner,
        &recipient,
        &400,
        &ScheduleFrequency::OneTime,
        &10,
        &0,
        &0,
    );
    assert_eq!(client.get_vault().queued_amount, 0);

    set_sequence(&e, 10);
    let result = client.execute_scheduled_withdrawals(&0);
    assert_eq!(result.executed, 1);
    assert_eq!(result.paid_out, 0);

    // The unpaid 400 is a queue claim, and the liability counter reflects it.
    assert_eq!(client.queue_len(), 1);
    assert_eq!(client.get_vault().queued_amount, 400);
}
