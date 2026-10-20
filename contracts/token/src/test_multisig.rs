#![cfg(test)]

//! Threshold multi-sig tests exercising the EmergencyGuard RBAC model:
//! - Guardians (multi-sig) can trigger emergency_pause.
//! - Admins (multi-sig) can resume and manage roles.

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use emergency_guard::{EmergencyGuard, EmergencyGuardClient};

fn setup_guard<'a>(
    env: &'a Env,
    admins: &[Address],
    threshold: u32,
    guardian: &'a Address,
) -> EmergencyGuardClient<'a> {
    let contract_id = env.register(EmergencyGuard, ());
    let client = EmergencyGuardClient::new(env, &contract_id);
    let admins_vec = {
        let mut v = vec![env];
        for a in admins {
            v.push_back(a.clone());
        }
        v
    };
    client.initialize(&admins_vec, &threshold, guardian);
    client
}

/// Guardians can trigger emergency_pause with multi-sig meeting the threshold.
#[test]
fn test_guardian_multi_sig_emergency_pause() {
    let env = Env::default();
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let a3 = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let client = setup_guard(&env, &[a1.clone(), a2.clone(), a3.clone()], 2, &guardian);

    // Add two more guardians so the guardian set has 3 members; threshold is 2.
    let approvers = vec![&env, a1.clone(), a2.clone()];
    client.add_guardian(&approvers, &g1);
    client.add_guardian(&approvers, &g2);

    // Two guardians meet the threshold of 2.
    let pause_approvers = vec![&env, g1.clone(), g2.clone()];
    client.emergency_pause(&pause_approvers);
    assert!(client.is_paused(&emergency_guard::PauseType::MINT));
}

/// An admin CANNOT trigger emergency_pause (only guardians can).
#[test]
fn test_admin_cannot_emergency_pause() {
    let env = Env::default();
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let client = setup_guard(&env, &[a1.clone(), a2.clone()], 2, &guardian);

    // Admin is not a guardian, so emergency_pause is rejected.
    let result = client.try_emergency_pause(&vec![&env, a1.clone()]);
    assert!(result.is_err());
}

/// A non-guardian, non-admin cannot trigger emergency_pause.
#[test]
fn test_outsider_cannot_emergency_pause() {
    let env = Env::default();
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let outsider = Address::generate(&env);
    let client = setup_guard(&env, &[a1.clone(), a2.clone()], 2, &guardian);

    let result = client.try_emergency_pause(&vec![&env, outsider.clone()]);
    assert!(result.is_err());
}

/// 2-of-3: resume succeeds with exactly 2 admin approvers.
#[test]
fn test_multisig_2_of_3_resume_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let a3 = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let client = setup_guard(&env, &[a1.clone(), a2.clone(), a3.clone()], 2, &guardian);

    // Add guardians so guardians can emergency_pause with multi-sig.
    let approvers = vec![&env, a1.clone(), a2.clone()];
    client.add_guardian(&approvers, &g1);
    client.add_guardian(&approvers, &g2);

    // Guardians pause
    let pause_approvers = vec![&env, g1.clone(), g2.clone()];
    client.emergency_pause(&pause_approvers);
    assert!(client.is_paused(&emergency_guard::PauseType::MINT));

    // 2-of-3 admins resume
    let approvers = vec![&env, a1.clone(), a2.clone()];
    client.resume(&approvers);
    assert!(!client.is_paused(&emergency_guard::PauseType::MINT));
}

/// resume fails with only 1 approver when threshold is 2.
#[test]
fn test_multisig_resume_fails_insufficient() {
    let env = Env::default();
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let a3 = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let client = setup_guard(&env, &[a1.clone(), a2.clone(), a3.clone()], 2, &guardian);

    let approvers = vec![&env, a1.clone(), a2.clone()];
    client.add_guardian(&approvers, &g1);
    client.add_guardian(&approvers, &g2);

    let pause_approvers = vec![&env, g1.clone(), g2.clone()];
    client.emergency_pause(&pause_approvers);

    let approvers = vec![&env, a1.clone()];
    let result = client.try_resume(&approvers);
    assert!(result.is_err());
}

/// add_admin requires multi-sig; new admin appears in list.
#[test]
fn test_multisig_add_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let guardian = Address::generate(&env);
    let client = setup_guard(&env, &[a1.clone(), a2.clone()], 2, &guardian);

    let approvers = vec![&env, a1.clone(), a2.clone()];
    client.add_admin(&approvers, &new_admin);

    let admins = client.get_admins();
    assert!(admins.iter().any(|a| a == new_admin));
}

/// remove_admin requires multi-sig; removed admin no longer in list.
#[test]
fn test_multisig_remove_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let a3 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let client = setup_guard(&env, &[a1.clone(), a2.clone(), a3.clone()], 2, &guardian);

    let approvers = vec![&env, a1.clone(), a2.clone()];
    client.remove_admin(&approvers, &a3);

    let admins = client.get_admins();
    assert!(!admins.iter().any(|a| a == a3));
}

/// 3-of-3: all admins required for resume; succeeds with all 3.
#[test]
fn test_multisig_3_of_3_all_required() {
    let env = Env::default();
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let a3 = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let client = setup_guard(&env, &[a1.clone(), a2.clone(), a3.clone()], 3, &guardian);

    // Add guardians so guardians can emergency_pause with multi-sig (threshold 3).
    let approvers = vec![&env, a1.clone(), a2.clone(), a3.clone()];
    client.add_guardian(&approvers, &g1);
    client.add_guardian(&approvers, &g2);
    client.add_guardian(&approvers, &g3);

    // All 3 guardians pause
    let pause_approvers = vec![&env, g1.clone(), g2.clone(), g3.clone()];
    client.emergency_pause(&pause_approvers);

    // All 3 admins resume
    let approvers = vec![&env, a1.clone(), a2.clone(), a3.clone()];
    client.resume(&approvers);
    assert!(!client.is_paused(&emergency_guard::PauseType::MINT));
}

/// Duplicate approvers do not count twice toward threshold.
#[test]
fn test_multisig_duplicate_approvers_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let client = setup_guard(&env, &[a1.clone(), a2.clone()], 2, &guardian);

    let approvers = vec![&env, a1.clone(), a2.clone()];
    client.add_guardian(&approvers, &g1);
    client.add_guardian(&approvers, &g2);

    client.emergency_pause(&vec![&env, g1.clone(), g2.clone()]);

    // Provide a1 twice for resume — should only count as 1 unique approver
    let approvers = vec![&env, a1.clone(), a1.clone()];
    let result = client.try_resume(&approvers);
    assert!(result.is_err());
}
