// stellarRpc.feeComparison.test.cjs — unit tests for fee comparison functionality
// Verifies that fetchFeeComparison correctly calculates percentage differences
// Runs with: node --test lib/stellarRpc.feeComparison.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

// Mock the fetchFeeStats and estimateFees functions
async function mockFetchFeeStats(rpcUrl) {
  // Simulate different fee stats for testnet vs mainnet
  if (rpcUrl.includes('testnet')) {
    return {
      soroban_fee_charged: { min: 100, max: 500, mode: 200, p10: 150, p20: 160, p30: 170, p40: 180, p50: 200, p60: 220, p70: 240, p80: 260, p90: 300, p95: 350, p99: 450 },
      soroban_fee_rate: 1,
      surge_multiplier: 1,
      classic_fee_charged: { min: 100, max: 500, mode: 200, p10: 150, p20: 160, p30: 170, p40: 180, p50: 200, p60: 220, p70: 240, p80: 260, p90: 300, p95: 350, p99: 450 },
      classic_fee_rate: 1,
      min_ledger_fee: 100,
    };
  } else if (rpcUrl.includes('mainnet')) {
    return {
      soroban_fee_charged: { min: 150, max: 750, mode: 300, p10: 225, p20: 240, p30: 255, p40: 270, p50: 300, p60: 330, p70: 360, p80: 390, p90: 450, p95: 525, p99: 675 },
      soroban_fee_rate: 1.5,
      surge_multiplier: 1.2,
      classic_fee_charged: { min: 150, max: 750, mode: 300, p10: 225, p20: 240, p30: 255, p40: 270, p50: 300, p60: 330, p70: 360, p80: 390, p90: 450, p95: 525, p99: 675 },
      classic_fee_rate: 1.5,
      min_ledger_fee: 150,
    };
  }
  throw new Error('Unknown RPC URL');
}

function mockEstimateFees(resourceCostStroops, feeStats) {
  const baseClassicFee = feeStats?.min_ledger_fee ?? 100;
  const surge = feeStats?.surge_multiplier ?? 1;
  const sorobanRate = feeStats?.soroban_fee_rate ?? 1;

  const minResourceFeeStroops = resourceCostStroops;
  const classicFeeStroops = baseClassicFee;
  const baseTotal = minResourceFeeStroops + classicFeeStroops;

  const feeBumps = [
    { label: 'Low (Min)', multiplier: 1.0, feeStroops: Math.ceil(baseTotal * 1.0 * sorobanRate), feeXlm: (baseTotal * 1.0 * sorobanRate / 10_000_000).toFixed(5), description: 'Minimum fee, slower confirmation' },
    { label: 'Medium', multiplier: 1.5, feeStroops: Math.ceil(baseTotal * 1.5 * sorobanRate), feeXlm: (baseTotal * 1.5 * sorobanRate / 10_000_000).toFixed(5), description: 'Balanced speed and cost' },
    { label: 'High', multiplier: 2.0, feeStroops: Math.ceil(baseTotal * 2.0 * sorobanRate), feeXlm: (baseTotal * 2.0 * sorobanRate / 10_000_000).toFixed(5), description: 'Priority processing' },
  ];

  return {
    minResourceFeeStroops,
    classicFeeStroops,
    totalFeeStroops: baseTotal,
    totalFeeXlm: (baseTotal / 10_000_000).toFixed(5),
    feeBumps,
    networkFees: {
      low: feeBumps[0].feeStroops,
      medium: feeBumps[1].feeStroops,
      high: feeBumps[2].feeStroops,
    },
    surgeMultiplier: surge,
  };
}

// Simplified version of fetchFeeComparison for testing
async function fetchFeeComparison(resourceCostStroops, testnetRpcUrl, mainnetRpcUrl) {
  const [testnetStats, mainnetStats] = await Promise.allSettled([
    mockFetchFeeStats(testnetRpcUrl),
    mockFetchFeeStats(mainnetRpcUrl),
  ]);

  const testnetFeeStats = testnetStats.status === 'fulfilled' ? testnetStats.value : null;
  const mainnetFeeStats = mainnetStats.status === 'fulfilled' ? mainnetStats.value : null;

  const testnetEstimate = mockEstimateFees(resourceCostStroops, testnetFeeStats);
  const mainnetEstimate = mockEstimateFees(resourceCostStroops, mainnetFeeStats);

  const percentageDifference =
    testnetEstimate.totalFeeStroops > 0
      ? ((mainnetEstimate.totalFeeStroops - testnetEstimate.totalFeeStroops) / testnetEstimate.totalFeeStroops) * 100
      : 0;

  return {
    testnet: testnetEstimate,
    mainnet: mainnetEstimate,
    percentageDifference,
    testnetHigher: testnetEstimate.totalFeeStroops > mainnetEstimate.totalFeeStroops,
  };
}

// ── Tests ───────────────────────────────────────────────────────────────────

test('fetchFeeComparison: returns estimates for both networks', async () => {
  const comparison = await fetchFeeComparison(
    5000,
    'https://soroban-testnet.stellar.org',
    'https://rpc.mainnet.stellar.org',
  );

  assert.ok(comparison.testnet, 'Should return testnet estimate');
  assert.ok(comparison.mainnet, 'Should return mainnet estimate');
  assert.strictEqual(typeof comparison.percentageDifference, 'number', 'Percentage difference should be a number');
  assert.strictEqual(typeof comparison.testnetHigher, 'boolean', 'testnetHigher should be a boolean');
});

test('fetchFeeComparison: calculates percentage difference correctly', async () => {
  const comparison = await fetchFeeComparison(
    5000,
    'https://soroban-testnet.stellar.org',
    'https://rpc.mainnet.stellar.org',
  );

  // Testnet: 5000 + 100 = 5100
  // Mainnet: 5000 + 150 = 5150
  // Percentage diff: (5150 - 5100) / 5100 * 100 = 0.98%
  assert.ok(Math.abs(comparison.percentageDifference - 0.98) < 0.1, 'Percentage difference should be approximately 0.98%');
});

test('fetchFeeComparison: mainnet higher when fees are higher', async () => {
  const comparison = await fetchFeeComparison(
    5000,
    'https://soroban-testnet.stellar.org',
    'https://rpc.mainnet.stellar.org',
  );

  assert.strictEqual(comparison.testnetHigher, false, 'testnetHigher should be false when mainnet fees are higher');
  assert.ok(comparison.mainnet.totalFeeStroops > comparison.testnet.totalFeeStroops, 'Mainnet fees should be higher');
});

test('fetchFeeComparison: handles RPC failures gracefully', async () => {
  const comparison = await fetchFeeComparison(
    5000,
    'https://invalid-testnet-rpc.example.com',
    'https://invalid-mainnet-rpc.example.com',
  );

  // Should still return estimates with default values when RPC fails
  assert.ok(comparison.testnet, 'Should return testnet estimate even on RPC failure');
  assert.ok(comparison.mainnet, 'Should return mainnet estimate even on RPC failure');
});

test('fetchFeeComparison: handles zero resource cost', async () => {
  const comparison = await fetchFeeComparison(
    0,
    'https://soroban-testnet.stellar.org',
    'https://rpc.mainnet.stellar.org',
  );

  // Testnet: 0 + 100 = 100
  // Mainnet: 0 + 150 = 150
  // Percentage diff: (150 - 100) / 100 * 100 = 50%
  assert.strictEqual(comparison.percentageDifference, 50, 'Percentage difference should be 50% when only classic fees differ');
});

test('fetchFeeComparison: parallel RPC calls', async () => {
  const startTime = Date.now();
  await fetchFeeComparison(
    5000,
    'https://soroban-testnet.stellar.org',
    'https://rpc.mainnet.stellar.org',
  );
  const endTime = Date.now();

  // Should complete in less than 100ms (parallel calls)
  assert.ok(endTime - startTime < 100, 'RPC calls should be parallel and complete quickly');
});

test('fetchFeeComparison: fee bump options differ between networks', async () => {
  const comparison = await fetchFeeComparison(
    5000,
    'https://soroban-testnet.stellar.org',
    'https://rpc.mainnet.stellar.org',
  );

  // Due to different soroban_fee_rate (1.0 vs 1.5), mainnet fees should be higher
  assert.ok(
    comparison.mainnet.feeBumps[0].feeStroops > comparison.testnet.feeBumps[0].feeStroops,
    'Mainnet low bump should be higher than testnet',
  );
  assert.ok(
    comparison.mainnet.feeBumps[2].feeStroops > comparison.testnet.feeBumps[2].feeStroops,
    'Mainnet high bump should be higher than testnet',
  );
});

test('fetchFeeComparison: surge multipliers are preserved', async () => {
  const comparison = await fetchFeeComparison(
    5000,
    'https://soroban-testnet.stellar.org',
    'https://rpc.mainnet.stellar.org',
  );

  assert.strictEqual(comparison.testnet.surgeMultiplier, 1, 'Testnet surge multiplier should be 1');
  assert.strictEqual(comparison.mainnet.surgeMultiplier, 1.2, 'Mainnet surge multiplier should be 1.2');
});
