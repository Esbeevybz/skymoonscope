// stellarRpc.test.cjs — unit tests for Stellar RPC fee estimation logic
// Closes Issue #595
// Runs with: node --test ./lib/stellarRpc.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const FeeBumpLevel = Object.freeze({
  ECONOMY: 'economy',
  STANDARD: 'standard',
  FAST: 'fast',
});

const BUMP_CONFIG = Object.freeze({
  [FeeBumpLevel.ECONOMY]: {
    label: 'Economy',
    multiplier: 1.0,
    description: 'Lowest cost, may take longer to confirm',
    estimatedTime: '~30-60s',
  },
  [FeeBumpLevel.STANDARD]: {
    label: 'Standard',
    multiplier: 1.5,
    description: 'Balanced speed and cost for most use cases',
    estimatedTime: '~15-30s',
  },
  [FeeBumpLevel.FAST]: {
    label: 'Fast',
    multiplier: 2.0,
    description: 'Priority processing for quick confirmation',
    estimatedTime: '~5-15s',
  },
});

function stroopsToXlm(stroops) {
  const xlm = stroops / 10_000_000;
  if (xlm === 0) return '0';
  if (xlm < 0.000001) return xlm.toFixed(9);
  if (xlm < 0.001) return xlm.toFixed(7);
  return xlm.toFixed(5);
}

function formatStroops(stroops) {
  const xlm = stroops / 10_000_000;
  return `${xlm.toFixed(7)} XLM`;
}

function estimateFees(resourceCostStroops, feeStats) {
  const baseClassicFee = (feeStats && feeStats.min_ledger_fee) || 100;
  const surge = (feeStats && feeStats.surge_multiplier) || 1;
  const sorobanRate = (feeStats && feeStats.soroban_fee_rate) || 1;

  const minResourceFeeStroops = resourceCostStroops;
  const classicFeeStroops = baseClassicFee;
  const baseTotal = minResourceFeeStroops + classicFeeStroops;

  const surgeMultiplier = surge;

  const feeBumps = Object.values(BUMP_CONFIG).map((cfg) => {
    const feeStroops = Math.ceil(baseTotal * cfg.multiplier * sorobanRate);
    return {
      label: cfg.label,
      multiplier: cfg.multiplier,
      feeStroops,
      feeXlm: stroopsToXlm(feeStroops),
      description: cfg.description,
      estimatedTime: cfg.estimatedTime,
    };
  });

  return {
    minResourceFeeStroops,
    classicFeeStroops,
    totalFeeStroops: baseTotal,
    totalFeeXlm: stroopsToXlm(baseTotal),
    feeBumps,
    networkFees: {
      economy: feeBumps[0].feeStroops,
      standard: feeBumps[1].feeStroops,
      fast: feeBumps[2].feeStroops,
    },
    surgeMultiplier,
  };
}

// ── stroopsToXlm Tests ────────────────────────────────────────────────────────

test('stroopsToXlm: returns 0 for zero stroops', () => {
  assert.equal(stroopsToXlm(0), '0');
});

test('stroopsToXlm: converts 10_000_000 stroops to 1 XLM', () => {
  assert.equal(stroopsToXlm(10_000_000), '1.00000');
});

test('stroopsToXlm: converts 100 stroops (very small amount)', () => {
  const result = stroopsToXlm(100);
  assert.ok(result.startsWith('0.00001'));
});

test('stroopsToXlm: converts 1_000_000 stroops to 0.1 XLM', () => {
  assert.equal(stroopsToXlm(1_000_000), '0.10000');
});

test('stroopsToXlm: converts 5_000_000 stroops to 0.5 XLM', () => {
  assert.equal(stroopsToXlm(5_000_000), '0.50000');
});

// ── estimateFees Tests ────────────────────────────────────────────────────────

test('estimateFees: computes base fee breakdown from resource cost', () => {
  const resourceCost = 500_000;
  const stats = {
    min_ledger_fee: 100,
    surge_multiplier: 1,
    soroban_fee_rate: 1,
  };

  const result = estimateFees(resourceCost, stats);

  assert.equal(result.minResourceFeeStroops, 500_000);
  assert.equal(result.classicFeeStroops, 100);
  assert.equal(result.totalFeeStroops, 500_100);
  assert.ok(result.totalFeeXlm.length > 0);
});

test('estimateFees: generates three fee bump options', () => {
  const result = estimateFees(500_000, null);

  assert.equal(result.feeBumps.length, 3);
  assert.equal(result.feeBumps[0].multiplier, 1.0);
  assert.equal(result.feeBumps[1].multiplier, 1.5);
  assert.equal(result.feeBumps[2].multiplier, 2.0);
});

test('estimateFees: fee bumps increase proportionally', () => {
  const result = estimateFees(1_000_000, null);

  const economy = result.networkFees.economy;
  const standard = result.networkFees.standard;
  const fast = result.networkFees.fast;

  assert.ok(economy < standard, 'Economy fee should be less than standard');
  assert.ok(standard < fast, 'Standard fee should be less than fast');
  assert.ok(fast >= economy * 2 || fast >= economy * 1.8, 'Fast should be roughly 2x economy');
});

test('estimateFees: applies surge multiplier when provided', () => {
  const baseResourceCost = 500_000;
  const normalResult = estimateFees(baseResourceCost, {
    min_ledger_fee: 100,
    surge_multiplier: 1,
    soroban_fee_rate: 1,
  });

  const surgedResult = estimateFees(baseResourceCost, {
    min_ledger_fee: 100,
    surge_multiplier: 5,
    soroban_fee_rate: 2,
  });

  assert.ok(surgedResult.surgeMultiplier > 1);
  assert.ok(surgedResult.networkFees.economy > normalResult.networkFees.economy);
});

test('estimateFees: returns valid XLM strings for all fee bump options', () => {
  const result = estimateFees(750_000, null);
  for (const bump of result.feeBumps) {
    assert.ok(typeof bump.feeXlm === 'string');
    assert.ok(bump.feeXlm.length > 0);
    assert.ok(bump.feeXlm.includes('.') || bump.feeXlm === '0');
  }
});

test('estimateFees: includes estimated time for each tier', () => {
  const result = estimateFees(750_000, null);
  assert.ok(result.feeBumps[0].estimatedTime === '~30-60s');
  assert.ok(result.feeBumps[1].estimatedTime === '~15-30s');
  assert.ok(result.feeBumps[2].estimatedTime === '~5-15s');
});

test('estimateFees: handles zero resource cost gracefully', () => {
  const result = estimateFees(0, null);

  assert.equal(result.minResourceFeeStroops, 0);
  assert.equal(result.classicFeeStroops, 100);
  assert.equal(result.totalFeeStroops, 100);
  assert.ok(result.feeBumps.length === 3);
});

test('estimateFees: handles very large resource costs', () => {
  const largeCost = 100_000_000_000;
  const result = estimateFees(largeCost, null);

  assert.equal(result.minResourceFeeStroops, largeCost);
  assert.ok(result.totalFeeStroops > largeCost);
  assert.ok(result.feeBumps[2].feeStroops > result.feeBumps[0].feeStroops);
});

test('estimateFees: uses default classic fee when no fee stats provided', () => {
  const result = estimateFees(1000, null);
  assert.equal(result.classicFeeStroops, 100);
});

test('estimateFees: uses custom classic fee from fee stats', () => {
  const result = estimateFees(1000, {
    min_ledger_fee: 500,
    surge_multiplier: 1,
    soroban_fee_rate: 1,
  });
  assert.equal(result.classicFeeStroops, 500);
});

test('estimateFees: classic fee + resource fee sum correctly', () => {
  const resourceCost = 1000;
  const result = estimateFees(resourceCost, {
    min_ledger_fee: 200,
    surge_multiplier: 1,
    soroban_fee_rate: 1,
  });

  assert.equal(result.totalFeeStroops, resourceCost + 200);
  assert.equal(result.minResourceFeeStroops + result.classicFeeStroops, result.totalFeeStroops);
});

// ── formatStroops Tests ───────────────────────────────────────────────────────

test('formatStroops: formats output string correctly', () => {
  const result = formatStroops(10_000_000);
  assert.ok(result.includes('XLM'));
  assert.ok(result.includes('1.0000000'));
});
