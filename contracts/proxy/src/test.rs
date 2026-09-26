extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env,
};

#[test]
fn test_proxy_upgrade_with_timelock() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);

    proxy.initialize(&admin, &impl_v1_id);
    assert_eq!(proxy.get_admin(), admin);
    assert_eq!(proxy.get_implementation(), impl_v1_id);

    // 1. Propose upgrade
    proxy.propose_upgrade(&impl_v2_id);

    let pending = proxy.get_pending_upgrade().unwrap();
    assert_eq!(pending.new_implementation, impl_v2_id);
    assert_eq!(pending.eta, env.ledger().timestamp() + 172_800);

    // 2. Fast-forward time by 48 hours (172,800 seconds)
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 172_800);

    // 3. Execute upgrade
    proxy.execute_upgrade();
    assert_eq!(proxy.get_implementation(), impl_v2_id);
    assert_eq!(proxy.get_pending_upgrade(), None);
}

#[test]
#[should_panic(expected = "Timelock delay of 48 hours has not elapsed")]
fn test_execute_upgrade_before_timelock_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);

    proxy.propose_upgrade(&impl_v2_id);

    // Try executing immediately without advancing time -> should panic
    proxy.execute_upgrade();
}

#[test]
fn test_cancel_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);

    proxy.propose_upgrade(&impl_v2_id);
    assert!(proxy.get_pending_upgrade().is_some());

    proxy.cancel_upgrade();
    assert_eq!(proxy.get_pending_upgrade(), None);
}

// ── Access control on the upgrade path (issue #080) ───────────────────────────

/// Register a proxy, returning the env, client, admin, and the original
/// implementation address.
fn setup() -> (Env, ProxyClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let client = ProxyClient::new(&env, &proxy_id);
    client.initialize(&admin, &impl_v1_id);
    (env, client, admin, impl_v1_id)
}

/// Reject every `require_auth` from here on, standing in for a caller that is
/// not the admin and therefore carries no authorization entry for it.
fn deny_all_auths(env: &Env) {
    env.set_auths(&[]);
}

#[test]
fn test_legacy_upgrade_to_cannot_skip_timelock() {
    let (env, client, _admin, impl_v1_id) = setup();
    let impl_v2_id = env.register(ProxyLogicV2, ());

    // `upgrade_to` is the legacy alias for `propose_upgrade`: it must not apply
    // the new implementation immediately.
    client.upgrade_to(&impl_v2_id);

    assert_eq!(client.get_implementation(), impl_v1_id);
    let pending = client.get_pending_upgrade().unwrap();
    assert_eq!(pending.new_implementation, impl_v2_id);

    // And it still cannot be executed early.
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    assert!(client.try_execute_upgrade().is_err());
    assert_eq!(client.get_implementation(), impl_v1_id);
}

#[test]
fn test_non_authorized_caller_cannot_propose_upgrade() {
    let (env, client, _admin, impl_v1_id) = setup();
    let impl_v2_id = env.register(ProxyLogicV2, ());

    deny_all_auths(&env);

    assert!(client.try_propose_upgrade(&impl_v2_id).is_err());
    assert_eq!(client.get_pending_upgrade(), None);
    assert_eq!(client.get_implementation(), impl_v1_id);
}

#[test]
fn test_non_authorized_caller_cannot_execute_upgrade() {
    let (env, client, _admin, impl_v1_id) = setup();
    let impl_v2_id = env.register(ProxyLogicV2, ());

    client.propose_upgrade(&impl_v2_id);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + TIMELOCK_DELAY);

    deny_all_auths(&env);
    assert!(client.try_execute_upgrade().is_err());
    assert_eq!(client.get_implementation(), impl_v1_id);
}

#[test]
fn test_non_authorized_caller_cannot_cancel_or_set_storage() {
    let (env, client, _admin, _) = setup();
    let impl_v2_id = env.register(ProxyLogicV2, ());
    let key = BytesN::from_array(&env, &[7u8; 32]);

    client.propose_upgrade(&impl_v2_id);

    deny_all_auths(&env);
    assert!(client.try_cancel_upgrade().is_err());
    assert!(client.try_set_storage(&key, &1i32).is_err());
    assert!(client.try_set_value(&42i32).is_err());
    // The proposal survives the rejected cancel.
    assert_eq!(
        client.get_pending_upgrade().unwrap().new_implementation,
        impl_v2_id
    );
    assert_eq!(client.get_value(), 0);
    assert!(client.get_storage(&key).is_none());
}

#[test]
fn test_set_value_requires_admin() {
    let (env, client, _admin, _) = setup();

    client.set_value(&42i32);
    assert_eq!(client.get_value(), 42);

    deny_all_auths(&env);
    assert!(client.try_set_value(&99i32).is_err());
    assert_eq!(client.get_value(), 42);
}

#[test]
fn test_increment_still_works_through_internal_write() {
    let (env, client, _admin, _) = setup();

    let next = client.increment(&5i32);
    assert_eq!(next, 5);
    assert_eq!(client.get_value(), 5);
}

#[test]
fn test_set_admin_transfers_authority() {
    let (env, client, admin, _) = setup();
    let new_admin = Address::generate(&env);

    client.set_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
    assert_ne!(client.get_admin(), admin);

    // With authorization withdrawn, nobody can act.
    deny_all_auths(&env);
    let impl_v2_id = env.register(ProxyLogicV2, ());
    assert!(client.try_propose_upgrade(&impl_v2_id).is_err());
}

#[test]
fn test_admin_can_still_upgrade_end_to_end() {
    let (env, client, _admin, _) = setup();
    let impl_v2_id = env.register(ProxyLogicV2, ());

    client.propose_upgrade(&impl_v2_id);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + TIMELOCK_DELAY);
    client.execute_upgrade();

    assert_eq!(client.get_implementation(), impl_v2_id);
    assert_eq!(client.get_pending_upgrade(), None);
}
