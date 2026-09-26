extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env};

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

// ── Admin-only upgrade path (issue #80) ───────────────────────────────────────

#[test]
fn upgrade_to_does_not_bypass_the_timelock() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);

    // `upgrade_to` is an alias for `propose_upgrade`, so it must only queue the
    // change and never repoint the implementation on its own.
    proxy.upgrade_to(&impl_v2_id);
    assert_eq!(proxy.get_implementation(), impl_v1_id);
    let pending = proxy.get_pending_upgrade().unwrap();
    assert_eq!(pending.new_implementation, impl_v2_id);

    // Still gated on the timelock.
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 172_800);
    proxy.execute_upgrade();
    assert_eq!(proxy.get_implementation(), impl_v2_id);
}

#[test]
fn non_admin_cannot_propose_an_upgrade() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;

    let env = Env::default();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);

    // Only the attacker authorizes the call, so the admin's require_auth() finds
    // no matching authorization.
    let attacker = Address::generate(&env);
    let res = proxy
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &proxy_id,
                fn_name: "propose_upgrade",
                args: (impl_v2_id.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_propose_upgrade(&impl_v2_id);

    assert!(res.is_err());
    // Nothing was queued and the implementation is untouched.
    assert!(proxy.get_pending_upgrade().is_none());
    assert_eq!(proxy.get_implementation(), impl_v1_id);
}

#[test]
fn non_admin_cannot_execute_an_upgrade() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());
    let impl_v2_id = env.register(ProxyLogicV2, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);
    proxy.propose_upgrade(&impl_v2_id);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 172_800);

    let attacker = Address::generate(&env);
    let res = proxy
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &proxy_id,
                fn_name: "execute_upgrade",
                args: ().into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_execute_upgrade();

    assert!(res.is_err());
    assert_eq!(proxy.get_implementation(), impl_v1_id);
}

#[test]
fn non_admin_cannot_write_proxy_state() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;

    let env = Env::default();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);

    let attacker = Address::generate(&env);
    let res = proxy
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &proxy_id,
                fn_name: "set_value",
                args: (99i32,).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_set_value(&99);

    assert!(res.is_err());
    assert_eq!(proxy.get_value(), 0);
}

#[test]
fn increment_still_works_without_admin_authorization() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let proxy_id = env.register(Proxy, ());
    let impl_v1_id = env.register(ProxyLogicV1, ());

    let proxy = ProxyClient::new(&env, &proxy_id);
    proxy.initialize(&admin, &impl_v1_id);

    // `increment` is deliberately permissionless, so it must not depend on the
    // admin-gated `set_value`.
    assert_eq!(proxy.increment(&5), 5);
    assert_eq!(proxy.get_value(), 5);
    assert_eq!(proxy.increment(&3), 8);
}
