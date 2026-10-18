use crate::LiquidityPoolClient;
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn test_swap_invariant(
        reserve_a in 1_000i128..1_000_000_000_000_000_000i128,
        reserve_b in 1_000i128..1_000_000_000_000_000_000i128,
        amount_out in 1i128..1_000_000_000_000_000_000i128,
        buy_a in any::<bool>(),
    ) {
        let e = Env::default();
        e.mock_all_auths();
        e.cost_estimate().budget().reset_unlimited();

        // Derive a strictly smaller amount_out using modulo so we don't reject generated examples
        let max_out = if buy_a { reserve_a } else { reserve_b };
        let valid_amount_out = (amount_out % (max_out - 1)) + 1;

        let admin = Address::generate(&e);
        let token_a = e.register_stellar_asset_contract_v2(admin.clone()).address();
        let token_b = e.register_stellar_asset_contract_v2(admin.clone()).address();

        let contract_id = e.register(crate::LiquidityPool, ());
        let client = LiquidityPoolClient::new(&e, &contract_id);

        client.initialize(&admin, &token_a, &token_b);

        let user = Address::generate(&e);
        let token_a_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token_a);
        let token_b_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token_b);

        // Give the user enough to deposit
        token_a_admin.mint(&user, &reserve_a);
        token_b_admin.mint(&user, &reserve_b);
        client.deposit(&user, &reserve_a, &reserve_b);

        // Invariant 3 – after initial deposit, total_shares == user LP balance
        let user_shares_after_deposit = client.balance(&user);
        let total_shares_after_deposit = client.total_supply();
        assert!(
            total_shares_after_deposit == user_shares_after_deposit,
            "Invariant 3 violated after deposit: total_shares ({}) != user shares ({})",
            total_shares_after_deposit,
            user_shares_after_deposit
        );

        let k_before = reserve_a * reserve_b; // Fits in i128 if inputs are < 10^18

        // We need another user for swapping
        let swapper = Address::generate(&e);
        // We need to give the swapper an effectively infinite input amount
        let in_max = 50_000_000_000_000_000_000i128; // Give swapper 50 tokens of 10^18 precision
        token_a_admin.mint(&swapper, &in_max);
        token_b_admin.mint(&swapper, &in_max);

        // Perform the swap
        // A swap can fail due to slippage exceeded if in_max wasn't enough, which is an expected error.
        // But with a huge in_max it shouldn't fail.
        let res = client.try_swap(&swapper, &buy_a, &valid_amount_out, &in_max);

        if let Ok(Ok(_)) = res {
            // --- Invariant 1: non-negative reserves after swap ---
            let (pool_reserve_a, pool_reserve_b) = client.get_reserves();
            assert!(
                pool_reserve_a >= 0,
                "Invariant 1 violated: reserve_a ({}) is negative after swap",
                pool_reserve_a
            );
            assert!(
                pool_reserve_b >= 0,
                "Invariant 1 violated: reserve_b ({}) is negative after swap",
                pool_reserve_b
            );

            // --- Original invariant: K must not decrease ---
            let token_a_client = soroban_sdk::token::Client::new(&e, &token_a);
            let token_b_client = soroban_sdk::token::Client::new(&e, &token_b);

            let pool_balance_a = token_a_client.balance(&contract_id);
            let pool_balance_b = token_b_client.balance(&contract_id);

            let k_after = pool_balance_a * pool_balance_b;

            assert!(k_after >= k_before, "Invariant violated! K before: {}, K after: {}", k_before, k_after);

            // --- Invariant 2: balance-reserve consistency after swap ---
            // The actual on-chain token balances must equal the pool's recorded reserves.
            assert!(
                pool_balance_a == pool_reserve_a,
                "Invariant 2 violated: on-chain balance_a ({}) != reserve_a ({}) after swap",
                pool_balance_a,
                pool_reserve_a
            );
            assert!(
                pool_balance_b == pool_reserve_b,
                "Invariant 2 violated: on-chain balance_b ({}) != reserve_b ({}) after swap",
                pool_balance_b,
                pool_reserve_b
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn test_deposit_invariants(
        reserve_a in 1_000i128..100_000_000_000i128,
        reserve_b in 1_000i128..100_000_000_000i128,
    ) {
        let e = Env::default();
        e.mock_all_auths();
        e.cost_estimate().budget().reset_unlimited();

        let admin = Address::generate(&e);
        let token_a = e.register_stellar_asset_contract_v2(admin.clone()).address();
        let token_b = e.register_stellar_asset_contract_v2(admin.clone()).address();

        let contract_id = e.register(crate::LiquidityPool, ());
        let client = LiquidityPoolClient::new(&e, &contract_id);

        client.initialize(&admin, &token_a, &token_b);

        let user = Address::generate(&e);
        let token_a_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token_a);
        let token_b_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token_b);

        token_a_admin.mint(&user, &reserve_a);
        token_b_admin.mint(&user, &reserve_b);
        client.deposit(&user, &reserve_a, &reserve_b);

        // --- Invariant 1: non-negative reserves after deposit ---
        let (pool_reserve_a, pool_reserve_b) = client.get_reserves();
        assert!(
            pool_reserve_a >= 0,
            "Invariant 1 violated: reserve_a ({}) is negative after deposit",
            pool_reserve_a
        );
        assert!(
            pool_reserve_b >= 0,
            "Invariant 1 violated: reserve_b ({}) is negative after deposit",
            pool_reserve_b
        );

        // --- Invariant 3: total_shares == sum of user shares ---
        // Only one depositor here; total supply must equal user's LP balance.
        let user_shares = client.balance(&user);
        let total_shares = client.total_supply();
        assert!(
            total_shares == user_shares,
            "Invariant 3 violated after deposit: total_shares ({}) != user shares ({})",
            total_shares,
            user_shares
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn test_round_trip_no_value_extraction(
        deposit_a in 1_000_000i128..100_000_000_000i128,
        deposit_b in 1_000_000i128..100_000_000_000i128,
    ) {
        let e = Env::default();
        e.mock_all_auths();
        e.cost_estimate().budget().reset_unlimited();

        let admin = Address::generate(&e);
        let token_a = e.register_stellar_asset_contract_v2(admin.clone()).address();
        let token_b = e.register_stellar_asset_contract_v2(admin.clone()).address();

        let contract_id = e.register(crate::LiquidityPool, ());
        let client = LiquidityPoolClient::new(&e, &contract_id);

        client.initialize(&admin, &token_a, &token_b);

        let user = Address::generate(&e);
        let token_a_client = soroban_sdk::token::Client::new(&e, &token_a);
        let token_b_client = soroban_sdk::token::Client::new(&e, &token_b);
        let token_a_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token_a);
        let token_b_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token_b);

        token_a_admin.mint(&user, &deposit_a);
        token_b_admin.mint(&user, &deposit_b);

        // Record balances before deposit
        let bal_a_before = token_a_client.balance(&user);
        let bal_b_before = token_b_client.balance(&user);

        // Deposit
        client.deposit(&user, &deposit_a, &deposit_b);

        // --- Invariant 1: non-negative reserves after deposit ---
        let (pool_reserve_a, pool_reserve_b) = client.get_reserves();
        assert!(
            pool_reserve_a >= 0,
            "Invariant 1 violated: reserve_a ({}) is negative after deposit",
            pool_reserve_a
        );
        assert!(
            pool_reserve_b >= 0,
            "Invariant 1 violated: reserve_b ({}) is negative after deposit",
            pool_reserve_b
        );

        // Withdraw all shares
        let shares = client.balance(&user);
        if shares > 0 {
            let withdraw_res = client.try_withdraw(&user, &shares);
            if let Ok(Ok(_)) = withdraw_res {
                let bal_a_after = token_a_client.balance(&user);
                let bal_b_after = token_b_client.balance(&user);

                // --- Invariant 4: no-value-extraction on round-trip ---
                // User cannot receive more tokens back than they deposited.
                assert!(
                    bal_a_after <= bal_a_before,
                    "Invariant 4 violated: returned_a ({}) > deposited_a ({}) — value extraction detected",
                    bal_a_after,
                    bal_a_before
                );
                assert!(
                    bal_b_after <= bal_b_before,
                    "Invariant 4 violated: returned_b ({}) > deposited_b ({}) — value extraction detected",
                    bal_b_after,
                    bal_b_before
                );

                // --- Invariant 1: non-negative reserves after withdraw ---
                let (pool_reserve_a_post, pool_reserve_b_post) = client.get_reserves();
                assert!(
                    pool_reserve_a_post >= 0,
                    "Invariant 1 violated: reserve_a ({}) is negative after withdraw",
                    pool_reserve_a_post
                );
                assert!(
                    pool_reserve_b_post >= 0,
                    "Invariant 1 violated: reserve_b ({}) is negative after withdraw",
                    pool_reserve_b_post
                );

                // --- Invariant 3: total_shares is consistent after full withdrawal ---
                // After full withdrawal by the sole LP, total_shares must equal remaining
                // user balance (which is 0 since all shares were withdrawn).
                let remaining_shares = client.balance(&user);
                let total_shares_post = client.total_supply();
                assert!(
                    total_shares_post == remaining_shares,
                    "Invariant 3 violated after full withdrawal: total_shares ({}) != remaining user shares ({})",
                    total_shares_post,
                    remaining_shares
                );
            }
        }
    }
}
