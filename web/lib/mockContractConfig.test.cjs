const test = require('node:test');
const assert = require('node:assert/strict');
const { loadMockContractConfig, resolveMockResult } = require('./mockContractConfig');

const defaults = {
  contractId: 'CDEFAULT',
  hasContractIdOverride: false,
  functions: [{ name: 'balance', inputs: [] }],
  results: {},
  transactions: null,
};

test('loads injected contract functions, results, and transactions', () => {
  const functions = [{ name: 'mint', inputs: [{ name: 'amount', type: 'u128' }] }];
  const results = { mint: { success: true, amount: 7 } };
  const transactions = [{
    hash: 'tx-1',
    functionName: 'mint',
    status: 'success',
    timestamp: 123,
    contractId: 'CINJECTED',
  }];
  const config = loadMockContractConfig(
    {
      NEXT_PUBLIC_CONTRACT_ID: 'CINJECTED',
      NEXT_PUBLIC_CONTRACT_FUNCTIONS: JSON.stringify(functions),
      NEXT_PUBLIC_CONTRACT_MOCK_RESULTS: JSON.stringify(results),
      NEXT_PUBLIC_CONTRACT_MOCK_TRANSACTIONS: JSON.stringify(transactions),
    },
    defaults,
  );

  assert.deepEqual(config, {
    contractId: 'CINJECTED',
    hasContractIdOverride: true,
    functions,
    results,
    transactions,
  });
  assert.deepEqual(resolveMockResult('mint', config.results, () => 'generated'), results.mint);
});

test('uses defaults for absent or invalid injected values', () => {
  const config = loadMockContractConfig(
    {
      NEXT_PUBLIC_CONTRACT_ID: '   ',
      NEXT_PUBLIC_CONTRACT_FUNCTIONS: '{invalid json',
      NEXT_PUBLIC_CONTRACT_MOCK_RESULTS: '[]',
      NEXT_PUBLIC_CONTRACT_MOCK_TRANSACTIONS: '[{"status":"unknown"}]',
    },
    defaults,
  );

  assert.deepEqual(config, defaults);
});

test('generated mock results remain the fallback for unconfigured functions', () => {
  let fallbackCalls = 0;
  const value = resolveMockResult('balance', { mint: 7 }, () => {
    fallbackCalls += 1;
    return 'generated';
  });

  assert.equal(value, 'generated');
  assert.equal(fallbackCalls, 1);
});