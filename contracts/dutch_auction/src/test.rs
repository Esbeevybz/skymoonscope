#![cfg(test)]
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
use soroban_sdk::{token, vec, Address, Env, IntoVal, Symbol, Vec};
use token_contract::TokenClient;

#[test]
fn test_dutch_auction() {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);

    // Deploy NFT contract
    let nft_contract_id = env.register(token_contract::Token, ());
    let nft_client = TokenClient::new(&env, &nft_contract_id);
    nft_client.initialize(&seller, &0, &"NFT".into_val(&env), &"NFT".into_val(&env));
    nft_client.mint(&seller, &1);

    // Deploy payment token
    let payment_contract_id = env.register(token_contract::Token, ());
    let payment_client = TokenClient::new(&env, &payment_contract_id);
    payment_client.initialize(&seller, &7, &"USD".into_val(&env), &"USD".into_val(&env));
    payment_client.mint(&buyer, &1000);

    // Deploy auction contract
    let auction_contract_id = env.register(DutchAuction, ());
    let auction_client = DutchAuctionClient::new(&env, &auction_contract_id);

    // Transfer NFT to auction
    nft_client.approve(&seller, &auction_contract_id, &1, &1000);
    nft_client.transfer(&seller, &auction_contract_id, &1);

    // Initialize auction: start 200, end 100, duration 10 ledgers
    auction_client.initialize(
        &seller,
        &nft_contract_id,
        &1,
        &payment_contract_id,
        &200,
        &100,
        &10,
    );

    // At start, price should be 200
    assert_eq!(auction_client.get_current_price(), 200);

    // Advance 5 ledgers, price should be 150
    env.ledger().set(LedgerInfo {
        timestamp: 500000,
        protocol_version: 1,
        sequence_number: 5,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });
    assert_eq!(auction_client.get_current_price(), 150);

    // Buy
    auction_client.buy(&buyer);

    // Check NFT transferred
    assert_eq!(nft_client.balance(&buyer), 1);
    // Check payment
    assert_eq!(payment_client.balance(&seller), 150);
    assert_eq!(auction_client.is_sold(), true);
}

#[test]
fn test_dutch_auction_30_day_duration_saturates_at_floor() {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);

    let nft_contract_id = env.register(token_contract::Token, ());
    let nft_client = TokenClient::new(&env, &nft_contract_id);
    nft_client.initialize(&seller, &0, &"NFT".into_val(&env), &"NFT".into_val(&env));
    nft_client.mint(&seller, &1);

    let payment_contract_id = env.register(token_contract::Token, ());
    let payment_client = TokenClient::new(&env, &payment_contract_id);
    payment_client.initialize(&seller, &7, &"USD".into_val(&env), &"USD".into_val(&env));
    payment_client.mint(&buyer, &1000);

    let auction_contract_id = env.register(DutchAuction, ());
    let auction_client = DutchAuctionClient::new(&env, &auction_contract_id);

    nft_client.approve(&seller, &auction_contract_id, &1, &1000);
    nft_client.transfer(&seller, &auction_contract_id, &1);

    let duration_ledgers: u32 = 30 * 24 * 60 * 60;
    let start_price = i128::MAX / 2;
    let end_price = 0_i128;
    auction_client.initialize(
        &seller,
        &nft_contract_id,
        &1,
        &payment_contract_id,
        &start_price,
        &end_price,
        &duration_ledgers,
    );

    env.ledger().set(LedgerInfo {
        timestamp: 1_000_000,
        protocol_version: 1,
        sequence_number: duration_ledgers + 10,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });

    assert_eq!(auction_client.get_current_price(), end_price);
    assert_eq!(auction_client.is_sold(), false);
    assert_eq!(payment_client.balance(&buyer), 1000);
}

// -- Price floor / long-auction decay (issue #74) ------------------------------

/// Deploy a ready-to-use auction: start 200, floor 100, 10 ledgers.
fn setup_auction(
    env: &Env,
    duration_ledgers: u32,
) -> (DutchAuctionClient<'_>, TokenClient<'_>, Address) {
    let seller = Address::generate(env);
    let buyer = Address::generate(env);

    let nft_id = env.register(token_contract::Token, ());
    let nft = TokenClient::new(env, &nft_id);
    nft.initialize(&seller, &0, &"NFT".into_val(env), &"NFT".into_val(env));
    nft.mint(&seller, &1);

    let pay_id = env.register(token_contract::Token, ());
    let pay = TokenClient::new(env, &pay_id);
    pay.initialize(&seller, &7, &"USD".into_val(env), &"USD".into_val(env));
    pay.mint(&buyer, &1_000_000);

    let auction_id = env.register(DutchAuction, ());
    let auction = DutchAuctionClient::new(env, &auction_id);

    nft.approve(&seller, &auction_id, &1, &1_000);
    nft.transfer(&seller, &auction_id, &1);

    auction.initialize(&seller, &nft_id, &1, &pay_id, &200, &100, &duration_ledgers);

    (auction, pay, buyer)
}

/// A long auction that runs well past its designed end time must settle at the
/// floor, never below it and never negative (issue #74).
#[test]
fn test_price_never_drops_below_the_floor_on_a_long_auction() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.sequence = 0);

    let (auction, _pay, _buyer) = setup_auction(&env, 10);

    // Far past the end ledger the price is pinned to the floor.
    env.ledger().with_mut(|li| li.sequence = 1_000_000);
    let price = auction.get_current_price();
    assert_eq!(price, 100);
    assert!(price >= 0, "price must never go negative");
}

/// Walking the whole decay keeps the price monotonically non-increasing and
/// always at or above the floor.
#[test]
fn test_decay_is_monotonic_and_bounded() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.sequence = 0);

    let (auction, _pay, _buyer) = setup_auction(&env, 10);

    let mut previous = i128::MAX;
    for seq in 0..40u32 {
        env.ledger().with_mut(|li| li.sequence = seq);
        let price = auction.get_current_price();
        assert!(price >= 100, "price below the floor at sequence {seq}");
        assert!(price >= 0, "negative price at sequence {seq}");
        assert!(
            price <= previous,
            "price increased at sequence {seq}: {price} > {previous}"
        );
        previous = price;
    }
    assert_eq!(
        previous, 100,
        "the auction should have settled at the floor"
    );
}

/// Before the end ledger the price decays strictly between start and floor.
#[test]
fn test_price_decays_between_start_and_floor() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.sequence = 0);

    let (auction, _pay, _buyer) = setup_auction(&env, 10);

    env.ledger().with_mut(|li| li.sequence = 0);
    assert_eq!(auction.get_current_price(), 200);

    env.ledger().with_mut(|li| li.sequence = 5);
    let mid = auction.get_current_price();
    assert!(mid < 200 && mid > 100, "mid-auction price was {mid}");

    env.ledger().with_mut(|li| li.sequence = 9);
    assert!(auction.get_current_price() >= 100);

    env.ledger().with_mut(|li| li.sequence = 10);
    assert_eq!(auction.get_current_price(), 100);
}
