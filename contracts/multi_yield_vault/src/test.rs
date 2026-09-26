use crate::{Error, MultiYieldVault, MultiYieldVaultClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

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

// -- Partial withdrawals / withdrawal queue (issue #83) -----------------------

/// Register a single fully-weighted pool and return the pieces a partial
/// withdrawal test needs.
fn setup_single_pool_vault(
    e: &Env,
) -> (
    MultiYieldVaultClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    let (client, admin, deposit_token, vault_id) = setup(e);
    let pool = register_mock_pool(e, &admin, 0, 30);
    client.register_pool(&pool, &true);
    client.set_weights(&soroban_sdk::vec![e, 10_000]);
    (client, admin, deposit_token, vault_id, pool)
}

/// Burn `amount` of `token` held by `from`, simulating liquidity the vault can
/// no longer release.
fn burn(e: &Env, token: &Address, from: &Address, amount: i128) {
    soroban_sdk::token::StellarAssetClient::new(e, token).burn(from, &amount);
}

fn balance(e: &Env, token: &Address, who: &Address) -> i128 {
    soroban_sdk::token::Client::new(e, token).balance(who)
}

#[test]
fn test_withdraw_partially_fills_and_queues_remainder() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, deposit_token, vault_id, pool) = setup_single_pool_vault(&e);

    let user = Address::generate(&e);
    mint(&e, &admin, &deposit_token, &user, 1_000);
    client.deposit(&user, &1_000);

    // Liquidity disappears from both the pool and the vault's idle balance, so
    // only 600 of the 1_000 the shares entitle the depositor to can be paid now.
    MockPoolClient::new(&e, &pool).drain_reserves(&1_000, &0);
    burn(&e, &deposit_token, &vault_id, &400);

    let filled = client.withdraw(&user, &1_000);
    assert_eq!(filled, 600);
    assert_eq!(balance(&e, &deposit_token, &user), 600);

    // The shortfall is persisted rather than silently dropped (issue #83).
    let request = client
        .get_withdrawal_request(&user)
        .expect("remainder should be queued");
    assert_eq!(request.owner, user);
    assert_eq!(request.remaining_amount, 400);
    assert_eq!(request.queue_count, 1);
    assert_eq!(client.total_queued_amount(), Ok(400));
    assert_eq!(client.get_withdrawal_queue().len(), 1);
    assert_eq!(client.get_withdrawal_queue().get(0).unwrap(), user);

    // The shares are burned, so the queue entry is the only remaining claim.
    assert_eq!(client.vault_balance(&user), 0);
}

#[test]
fn test_fully_funded_withdrawal_creates_no_queue_entry() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, deposit_token, _, _) = setup_single_pool_vault(&e);

    let user = Address::generate(&e);
    mint(&e, &admin, &deposit_token, &user, &1_000);
    client.deposit(&user, &1_000);

    assert_eq!(client.withdraw(&user, &1_000), 1_000);
    assert_eq!(client.get_withdrawal_request(&user), None);
    assert_eq!(client.get_withdrawal_queue().len(), 0);
    assert_eq!(client.total_queued_amount(), Ok(0));
}

#[test]
fn test_claim_withdrawal_settles_queue_once_liquidity_returns() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, deposit_token, vault_id, pool) = setup_single_pool_vault(&e);

    let user = Address::generate(&e);
    mint(&e, &admin, &deposit_token, &user, 1_000);
    client.deposit(&user, &1_000);
    MockPoolClient::new(&e, &pool).drain_reserves(&1_000, &0);
    burn(&e, &deposit_token, &vault_id, &400);

    assert_eq!(client.withdraw(&user, &1_000), 600);

    // Liquidity frees up again and the depositor claims the queued remainder.
    mint(&e, &admin, &deposit_token, &vault_id, 400);
    assert_eq!(client.claim_withdrawal(&user), Ok(400));

    assert_eq!(client.get_withdrawal_request(&user), None);
    assert_eq!(client.get_withdrawal_queue().len(), 0);
    assert_eq!(client.total_queued_amount(), Ok(0));
    // Nothing was lost: the depositor ends up with the full original amount.
    assert_eq!(balance(&e, &deposit_token, &user), 1_000);
}

#[test]
fn test_claim_withdrawal_partially_settles_and_keeps_entry() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, deposit_token, vault_id, pool) = setup_single_pool_vault(&e);

    let user = Address::generate(&e);
    mint(&e, &admin, &deposit_token, &user, 1_000);
    client.deposit(&user, &1_000);
    MockPoolClient::new(&e, &pool).drain_reserves(&1_000, &0);
    burn(&e, &deposit_token, &vault_id, &400);

    assert_eq!(client.withdraw(&user, &1_000), 600);

    // Only part of the queued amount can be covered right now.
    mint(&e, &admin, &deposit_token, &vault_id, 150);
    assert_eq!(client.claim_withdrawal(&user), Ok(150));

    let request = client.get_withdrawal_request(&user).unwrap();
    assert_eq!(request.remaining_amount, 250);
    assert_eq!(client.total_queued_amount(), Ok(250));
    assert_eq!(client.get_withdrawal_queue().len(), 1);
}

#[test]
fn test_claim_withdrawal_without_entry_errors() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _, _, _, _) = setup_single_pool_vault(&e);

    let user = Address::generate(&e);
    assert_eq!(
        client.claim_withdrawal(&user),
        Err(Error::NoQueuedWithdrawal)
    );
}

#[test]
fn test_multiple_depositors_each_get_their_own_queue_entry() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, deposit_token, vault_id, pool) = setup_single_pool_vault(&e);

    let user1 = Address::generate(&e);
    let user2 = Address::generate(&e);
    mint(&e, &admin, &deposit_token, &user1, 1_000);
    mint(&e, &admin, &deposit_token, &user2, 1_000);
    client.deposit(&user1, &1_000);
    client.deposit(&user2, &1_000);

    // Vault is left holding far less than the 2_000 of shares outstanding.
    MockPoolClient::new(&e, &pool).drain_reserves(&2_000, &0);
    burn(&e, &deposit_token, &vault_id, &1_400);

    assert_eq!(client.withdraw(&user1, &1_000), 600);
    assert_eq!(client.withdraw(&user2, &1_000), 0);

    // Each depositor keeps an independent claim, in submission order.
    let queue = client.get_withdrawal_queue();
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.get(0).unwrap(), user1);
    assert_eq!(queue.get(1).unwrap(), user2);
    assert_eq!(
        client
            .get_withdrawal_request(&user1)
            .unwrap()
            .remaining_amount,
        400
    );
    assert_eq!(
        client
            .get_withdrawal_request(&user2)
            .unwrap()
            .remaining_amount,
        1_000
    );
    assert_eq!(client.total_queued_amount(), Ok(1_400));
}
