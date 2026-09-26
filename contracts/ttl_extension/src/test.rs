//! Tests for the TTL clamp (issue #89).

use super::*;
use soroban_sdk::Env;

fn setup() -> (Env, TtlExtensionClient<'static>) {
    let env = Env::default();
    let contract_id = env.register(TtlExtension, ());
    let client = TtlExtensionClient::new(&env, &contract_id);
    (env, client)
}

#[test]
fn test_ttl_extension() {
    let (_env, client) = setup();

    client.write(&1);
    let init_p = client.persistent_ttl();
    let init_i = client.instance_ttl();

    // Below the threshold: nothing is touched.
    client.extend_ttl(&0, &(init_p + 1000));
    assert_eq!(client.persistent_ttl(), init_p);
    assert_eq!(client.instance_ttl(), init_i);

    // Above the threshold: both entries are extended.
    client.extend_ttl(&(init_p + 1), &(init_p + 2000));
    assert!(client.persistent_ttl() > init_p);
    assert!(client.instance_ttl() > init_i);
}

#[test]
fn test_extend_ttl_clamps_target_to_max() {
    let (_env, client) = setup();
    client.write(&1);

    let current = client.persistent_ttl();

    // A target far beyond the protocol maximum is clamped, so this must not trap
    // (issue #89).
    client.extend_ttl(&(current + 1), &u32::MAX);

    assert!(
        client.persistent_ttl() <= MAX_TTL,
        "persistent ttl {} exceeded MAX_TTL {MAX_TTL}",
        client.persistent_ttl()
    );
    assert!(client.instance_ttl() <= MAX_TTL);
}

#[test]
fn test_bump_never_exceeds_max_ttl() {
    let (_env, client) = setup();
    client.write(&1);

    // An absurd bump must be clamped to the remaining headroom rather than
    // trapping the host.
    let applied = client.bump_ttl(&u32::MAX);
    assert!(applied <= MAX_TTL);
    assert!(client.instance_ttl() <= MAX_TTL);
    assert!(client.persistent_ttl() <= MAX_TTL);
}

#[test]
fn test_bump_applies_at_most_the_headroom() {
    let (_env, client) = setup();
    client.write(&1);

    let current = client.instance_ttl();
    let room = MAX_TTL.saturating_sub(current);

    // Asking for more than there is room for yields exactly the headroom.
    let applied = client.bump_ttl(&(room + 1_000));
    assert_eq!(applied, room);
    assert!(client.instance_ttl() <= MAX_TTL);
}

#[test]
fn test_bump_within_headroom_is_applied_in_full() {
    let (_env, client) = setup();
    client.write(&1);

    let before = client.instance_ttl();
    let applied = client.bump_ttl(&10);
    assert_eq!(applied, 10);
    assert!(client.instance_ttl() >= before);
    assert!(client.instance_ttl() <= MAX_TTL);
}

#[test]
fn test_bump_of_zero_is_a_no_op() {
    let (_env, client) = setup();
    client.write(&1);

    let before_i = client.instance_ttl();
    let before_p = client.persistent_ttl();

    assert_eq!(client.bump_ttl(&0), 0);
    assert_eq!(client.instance_ttl(), before_i);
    assert_eq!(client.persistent_ttl(), before_p);
}

#[test]
fn test_bump_persistent_clamps_too() {
    let (_env, client) = setup();
    client.write(&1);

    let current = client.persistent_ttl();
    let room = MAX_TTL.saturating_sub(current);

    let applied = client.bump_persistent_ttl(&u32::MAX);
    assert_eq!(applied, room);
    assert!(client.persistent_ttl() <= MAX_TTL);
}

#[test]
fn test_headroom_view_is_consistent() {
    let (_env, client) = setup();

    let current = client.instance_ttl();
    let room = client.max_ttl_headroom();
    assert_eq!(room, MAX_TTL.saturating_sub(current));
    assert!(room <= MAX_TTL);
}

#[test]
fn test_clamp_never_raises_a_target() {
    // A target at or below the current TTL is passed through unchanged.
    assert_eq!(clamp_target(0), 0);
    assert_eq!(clamp_target(100), 100);
    assert_eq!(clamp_target(MAX_TTL), MAX_TTL);
    assert_eq!(clamp_target(MAX_TTL + 1), MAX_TTL);
    assert_eq!(clamp_target(u32::MAX), MAX_TTL);
}

#[test]
fn test_read_round_trips_the_written_value() {
    let (_env, client) = setup();
    client.write(&42);
    let bytes = client.read();
    assert_eq!(bytes, Bytes::from_slice(&_env, &42u32.to_be_bytes()));
}
