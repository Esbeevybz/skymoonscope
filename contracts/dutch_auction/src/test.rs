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

// ── Price floor (issue #074) ─────────────────────────────────────────────────

/// Deploy an auction and return `(client, payment_client, buyer, seller)` with
/// the NFT already escrowed.
fn setup_auction(
    env: &Env,
    start_price: i128,
    end_price: i128,
    duration_ledgers: u32,
) -> (DutchAuctionClient<'_>, TokenClient<'_>, Address, Address) {
    let seller = Address::generate(env);
    let buyer = Address::generate(env);

    let nft_contract_id = env.register(token_contract::Token, ());
    let nft_client = TokenClient::new(env, &nft_contract_id);
    nft_client.initialize(&seller, &0, &"NFT".into_val(env), &"NFT".into_val(env));
    nft_client.mint(&seller, &1);

    let payment_contract_id = env.register(token_contract::Token, ());
    let payment_client = TokenClient::new(env, &payment_contract_id);
    payment_client.initialize(&seller, &7, &"USD".into_val(env), &"USD".into_val(env));
    payment_client.mint(&buyer, &1_000_000);

    let auction_contract_id = env.register(DutchAuction, ());
    let auction_client = DutchAuctionClient::new(env, &auction_contract_id);

    nft_client.approve(&seller, &auction_contract_id, &1, &1000);
    nft_client.transfer(&seller, &auction_contract_id, &1);

    auction_client.initialize(
        &seller,
        &nft_contract_id,
        &1,
        &payment_contract_id,
        &start_price,
        &end_price,
        &duration_ledgers,
    );

    (auction_client, payment_client, buyer, seller)
}

fn set_sequence(env: &Env, sequence: u32) {
    env.ledger().set(LedgerInfo {
        timestamp: 1_000_000,
        protocol_version: 1,
        sequence_number: sequence,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });
}

#[test]
fn test_price_never_falls_below_the_floor() {
    let env = Env::default();
    env.mock_all_auths();

    let start_price: i128 = 1_000;
    let end_price: i128 = 100;
    let duration: u32 = 100;
    let (auction, _, _, _) = setup_auction(&env, start_price, end_price, duration);

    // Walk the whole decay and past the end, checking the invariant every step.
    for seq in 0..(duration * 2 + 5) {
        set_sequence(&env, seq);
        let price = auction.get_current_price();
        assert!(
            price >= end_price,
            "price {price} fell below the floor {end_price} at ledger {seq}"
        );
        assert!(
            price <= start_price,
            "price {price} exceeded the start price"
        );
    }
}

#[test]
fn test_price_saturates_at_floor_past_end_ledger() {
    let env = Env::default();
    env.mock_all_auths();

    let end_price: i128 = 250;
    let duration: u32 = 50;
    let (auction, _, _, _) = setup_auction(&env, 1_000, end_price, duration);

    set_sequence(&env, duration + 10_000);
    assert_eq!(auction.get_current_price(), end_price);
}

#[test]
fn test_price_interpolates_linearly_between_bounds() {
    let env = Env::default();
    env.mock_all_auths();

    let start_price: i128 = 1_000;
    let end_price: i128 = 200;
    let duration: u32 = 100;
    let (auction, _, _, _) = setup_auction(&env, start_price, end_price, duration);

    // At the start ledger the price is the start price.
    set_sequence(&env, 0);
    assert_eq!(auction.get_current_price(), start_price);

    // Halfway through, half the drop has been taken.
    set_sequence(&env, duration / 2);
    assert_eq!(auction.get_current_price(), 600);

    // Just before the end it approaches, but never passes, the floor.
    set_sequence(&env, duration - 1);
    let price = auction.get_current_price();
    assert!(price >= end_price && price < start_price);
}

#[test]
fn test_buy_settles_at_floor_after_the_auction_window() {
    let env = Env::default();
    env.mock_all_auths();

    let end_price: i128 = 300;
    let duration: u32 = 40;
    let (auction, payment, buyer, seller) = setup_auction(&env, 900, end_price, duration);

    set_sequence(&env, duration + 500);
    auction.buy(&buyer);

    // The buyer paid exactly the floor, not a decayed-below-floor amount.
    assert_eq!(payment.balance(&seller), end_price);
    assert_eq!(payment.balance(&buyer), 1_000_000 - end_price);
    assert!(auction.is_sold());
}
