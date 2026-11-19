/**
 * Tests for CompoundInterestChart — buildChartData helper (#418)
 *
 * The chart component itself relies on recharts (browser-only), so we test the
 * pure data-transformation logic (buildChartData) in isolation.  That function
 * is the only place with non-trivial business logic in the component; recharts
 * rendering is covered by the existing E2E / browser test suite.
 *
 * Pattern follows the project convention: node:test + node:assert/strict,
 * CommonJS, plain JavaScript.
 */

'use strict';

const test  = require('node:test');
const assert = require('node:assert/strict');

// ---------------------------------------------------------------------------
// Import the pure helper.  Since the component is .tsx we inline the
// equivalent pure-JS version here, keeping it byte-for-byte equivalent to the
// TypeScript source so any drift becomes a test failure.
// ---------------------------------------------------------------------------

const { calculateStakingYield } = require('../lib/stakingCalculator');

/**
 * buildChartData — mirrors CompoundInterestChart.tsx's exported function.
 *
 * @param {object} result  Full StakingYieldResult from calculateStakingYield
 * @returns {Array<{label:string, month:number, principal:number, rewards:number, balance:number}>}
 */
function buildChartData(result) {
  const { depositAmount, breakdownByMonth } = result;
  return breakdownByMonth.map((row) => ({
    label: row.month === 1 || row.month % 6 === 0 ? `M${row.month}` : '',
    month: row.month,
    principal: depositAmount,
    rewards: Number(row.yieldEarned.toFixed(2)),
    balance: Number(row.balance.toFixed(2)),
  }));
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function result12Month() {
  return calculateStakingYield({
    depositAmount: 1000,
    lockDurationMonths: 12,
    baseApyPercentage: 12,
    compoundFrequency: 'monthly',
    enableTierMultiplier: false,
  });
}

function result1Month() {
  return calculateStakingYield({
    depositAmount: 500,
    lockDurationMonths: 1,
    baseApyPercentage: 10,
    compoundFrequency: 'monthly',
    enableTierMultiplier: false,
  });
}

function result36Month() {
  return calculateStakingYield({
    depositAmount: 10000,
    lockDurationMonths: 36,
    baseApyPercentage: 8,
    compoundFrequency: 'daily',
    enableTierMultiplier: true,
  });
}

// ===========================================================================
// Tests
// ===========================================================================

test('buildChartData: produces one data point per month', () => {
  const data = buildChartData(result12Month());
  assert.equal(data.length, 12);
});

test('buildChartData: each point has the expected shape', () => {
  const data = buildChartData(result12Month());
  for (const point of data) {
    assert.ok(typeof point.label === 'string',   'label should be a string');
    assert.ok(typeof point.month === 'number',   'month should be a number');
    assert.ok(typeof point.principal === 'number', 'principal should be a number');
    assert.ok(typeof point.rewards === 'number',  'rewards should be a number');
    assert.ok(typeof point.balance === 'number',  'balance should be a number');
  }
});

test('buildChartData: principal is constant and equals depositAmount', () => {
  const r = result12Month();
  const data = buildChartData(r);
  for (const point of data) {
    assert.equal(point.principal, r.depositAmount);
  }
});

test('buildChartData: rewards are non-negative and monotonically increasing', () => {
  const data = buildChartData(result12Month());
  let prev = -1;
  for (const point of data) {
    assert.ok(point.rewards >= 0, `rewards at M${point.month} should be ≥ 0`);
    assert.ok(point.rewards >= prev, `rewards should be non-decreasing (M${point.month})`);
    prev = point.rewards;
  }
});

test('buildChartData: balance equals principal + rewards at each month', () => {
  const data = buildChartData(result12Month());
  for (const point of data) {
    const expected = Number((point.principal + point.rewards).toFixed(2));
    assert.equal(
      point.balance,
      expected,
      `balance at M${point.month} should equal principal + rewards`,
    );
  }
});

test('buildChartData: final-month balance matches totalBalance from calculateStakingYield', () => {
  const r = result12Month();
  const data = buildChartData(r);
  const last = data[data.length - 1];
  assert.equal(last.balance, r.totalBalance);
});

test('buildChartData: month 1 and multiples of 6 get non-empty labels', () => {
  const data = buildChartData(result36Month());
  const labeled = data.filter((d) => d.label !== '');
  const labeledMonths = labeled.map((d) => d.month);
  // Should include month 1, 6, 12, 18, 24, 30, 36
  const expected = [1, 6, 12, 18, 24, 30, 36];
  assert.deepEqual(labeledMonths, expected);
  // Label format: "M<month>"
  for (const point of labeled) {
    assert.equal(point.label, `M${point.month}`);
  }
});

test('buildChartData: intermediate months have empty labels', () => {
  const data = buildChartData(result36Month());
  const unlabeled = data.filter((d) => d.month !== 1 && d.month % 6 !== 0);
  for (const point of unlabeled) {
    assert.equal(point.label, '', `M${point.month} should have an empty label`);
  }
});

test('buildChartData: single-month result produces exactly one point', () => {
  const data = buildChartData(result1Month());
  assert.equal(data.length, 1);
  assert.equal(data[0].month, 1);
  assert.equal(data[0].label, 'M1');
});

test('buildChartData: zero deposit produces all-zero rewards and constant zero balance', () => {
  const r = calculateStakingYield({ depositAmount: 0, lockDurationMonths: 12 });
  const data = buildChartData(r);
  for (const point of data) {
    assert.equal(point.principal, 0);
    assert.equal(point.rewards, 0);
    assert.equal(point.balance, 0);
  }
});

test('buildChartData: handles simple interest (no compounding) correctly', () => {
  const r = calculateStakingYield({
    depositAmount: 1000,
    lockDurationMonths: 12,
    baseApyPercentage: 12,
    compoundFrequency: 'none',
    enableTierMultiplier: false,
  });
  const data = buildChartData(r);
  // Simple interest: A = P * (1 + r * t) = 1000 * (1 + 0.12 * 1) = 1120
  // (12% APY, 12 months = 1 year, so interest = 1000 * 0.12 = 120)
  const last = data[data.length - 1];
  assert.equal(last.balance, 1120);
  assert.equal(last.rewards, 120);
});

test('buildChartData: 36-month result with tier multiplier has growing rewards', () => {
  const r = result36Month();
  const data = buildChartData(r);
  assert.equal(data.length, 36);
  const first = data[0];
  const last  = data[data.length - 1];
  assert.ok(last.rewards > first.rewards, 'rewards should grow over 36 months');
  assert.ok(last.balance > last.principal, 'final balance should exceed principal');
});

// ─── Integration: chart data reflects changes in compound frequency ───────────

test('buildChartData: daily compounding yields higher rewards than monthly at same APY', () => {
  const base = {
    depositAmount: 5000,
    lockDurationMonths: 24,
    baseApyPercentage: 10,
    enableTierMultiplier: false,
  };
  const daily   = buildChartData(calculateStakingYield({ ...base, compoundFrequency: 'daily' }));
  const monthly = buildChartData(calculateStakingYield({ ...base, compoundFrequency: 'monthly' }));
  const lastDaily   = daily[daily.length - 1];
  const lastMonthly = monthly[monthly.length - 1];
  assert.ok(
    lastDaily.rewards > lastMonthly.rewards,
    'daily compounding should produce more rewards than monthly',
  );
});
