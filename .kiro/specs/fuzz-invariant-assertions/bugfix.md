# Bugfix Requirements Document

## Introduction

The fuzz test in `skymoonscope/contracts/liquidity_pool/src/fuzz_test.rs` calls pool operations
(`deposit`, `swap`, `withdraw`) with random inputs but only asserts that they do not panic and,
for the swap case, that the constant-product K does not decrease. It never verifies the broader
pool invariants after each operation: that reserve balances remain non-negative, that the total
LP share supply is consistent with the shares held by each user, and that the constant-sum
relationship between actual on-chain token balances and the pool's recorded reserves holds. As a
result, invariant-breaking bugs (e.g. reserve accounting drift, share supply mismatch, negative
balance scenarios) could go undetected across all 256 fuzz iterations.

---

## Bug Analysis

### Current Behavior (Defect)

1.1 WHEN a fuzz iteration calls `deposit`, `swap`, or `withdraw` with random valid inputs THEN
    the system only checks that the call does not panic, without asserting that pool reserves
    remain non-negative afterward.

1.2 WHEN a fuzz iteration successfully executes `swap` THEN the system only verifies
    `K_after >= K_before` and does not assert that the pool's recorded `reserve_a` and
    `reserve_b` match the actual on-chain token balances held by the contract.

1.3 WHEN a fuzz iteration calls `deposit` or `withdraw` THEN the system does not assert that
    `pool.total_shares` equals the sum of LP shares credited to all depositing users.

1.4 WHEN any fuzz operation sequence completes a full round-trip (deposit then withdraw) THEN
    the system does not assert that the user receives no more tokens than they deposited (modulo
    fees and penalties), so value-extraction bugs are not caught.

### Expected Behavior (Correct)

2.1 WHEN a fuzz iteration calls `deposit`, `swap`, or `withdraw` successfully THEN the system
    SHALL assert that `pool.reserve_a >= 0` and `pool.reserve_b >= 0` after every operation.

2.2 WHEN a fuzz iteration successfully executes `swap` THEN the system SHALL assert that the
    actual on-chain balance of token A held by the contract equals `pool.reserve_a`, and the
    actual on-chain balance of token B equals `pool.reserve_b` (balance–reserve consistency).

2.3 WHEN a fuzz iteration calls `deposit` or `withdraw` successfully THEN the system SHALL
    assert that `pool.total_shares` equals the cumulative sum of LP shares minted to each
    depositor minus shares burned by each withdrawer.

2.4 WHEN a fuzz round-trip (deposit followed by immediate withdraw of all shares) completes
    successfully THEN the system SHALL assert that the user's returned token amounts do not
    exceed their original deposit amounts, enforcing the no-value-extraction invariant.

### Unchanged Behavior (Regression Prevention)

3.1 WHEN a `swap` call is made with valid positive `out` and sufficient `in_max` THEN the system
    SHALL CONTINUE TO enforce `K_after >= K_before` (the existing constant-product invariant
    check must remain in place).

3.2 WHEN a `swap` call fails due to slippage, insufficient liquidity, or arithmetic overflow
    THEN the system SHALL CONTINUE TO treat those as expected, non-panic outcomes and skip
    invariant assertions for that iteration (the `if let Ok(Ok(_))` guard must be preserved).

3.3 WHEN the fuzz test environment is initialised with `mock_all_auths()` and an unlimited
    budget THEN the system SHALL CONTINUE TO operate without authorization errors or budget
    panics, so random-input coverage is not artificially narrowed.

3.4 WHEN a fuzz test generates an `amount_out` that is derived via the existing modulo clamp
    (`(amount_out % (max_out - 1)) + 1`) THEN the system SHALL CONTINUE TO use that clamp to
    prevent test-case rejection due to invalid swap sizes.

---

## Bug Condition Pseudocode

### Bug Condition Function

```pascal
FUNCTION isBugCondition(iteration)
  INPUT:  iteration — one fuzz test execution (env + random inputs + operation sequence)
  OUTPUT: boolean

  // The bug is present when a successful pool operation completes
  // but no pool-level invariants are checked afterward.
  RETURN operation_succeeded(iteration)
     AND NOT invariants_asserted_after(iteration)
END FUNCTION
```

### Property: Fix Checking

```pascal
// After every successful pool operation, ALL of the following must hold:

FOR ALL iteration WHERE isBugCondition(iteration) DO
  pool  ← load_pool_state(iteration.env)
  bal_a ← on_chain_balance(token_a, contract_id)
  bal_b ← on_chain_balance(token_b, contract_id)

  // Invariant 1 – non-negative reserves
  ASSERT pool.reserve_a >= 0
  ASSERT pool.reserve_b >= 0

  // Invariant 2 – balance–reserve consistency (swap only)
  IF operation = swap THEN
    ASSERT bal_a = pool.reserve_a
    ASSERT bal_b = pool.reserve_b
  END IF

  // Invariant 3 – total share supply equals user shares
  ASSERT pool.total_shares = SUM(user_shares_minted) - SUM(user_shares_burned)

  // Invariant 4 – no-value-extraction (round-trip)
  IF operation = round_trip THEN
    ASSERT returned_a <= deposited_a
    ASSERT returned_b <= deposited_b
  END IF
END FOR
```

### Property: Preservation Checking

```pascal
// For iterations where the operation fails (expected error paths),
// the original panic-free guarantee must still hold.

FOR ALL iteration WHERE NOT isBugCondition(iteration) DO
  // F(iteration) = try_op returns Err(_) without panic
  ASSERT F(iteration) = F'(iteration)   // no change in error-path behaviour
END FOR
```
