const CONTRACT_TYPES = new Set([
  'address',
  'u32',
  'i128',
  'u128',
  'string',
  'symbol',
  'bool',
  'struct',
  'enum',
]);

function isRecord(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function parseJson(raw, validate) {
  if (!raw) return undefined;

  try {
    const value = JSON.parse(raw);
    return validate(value) ? value : undefined;
  } catch {
    return undefined;
  }
}

function isContractFunction(value) {
  return (
    isRecord(value) &&
    typeof value.name === 'string' &&
    value.name.trim().length > 0 &&
    Array.isArray(value.inputs) &&
    value.inputs.every(
      (input) =>
        isRecord(input) &&
        typeof input.name === 'string' &&
        input.name.trim().length > 0 &&
        CONTRACT_TYPES.has(input.type) &&
        (input.optional === undefined || typeof input.optional === 'boolean'),
    ) &&
    (value.outputs === undefined || CONTRACT_TYPES.has(value.outputs))
  );
}

function isTransaction(value) {
  return (
    isRecord(value) &&
    typeof value.hash === 'string' &&
    typeof value.functionName === 'string' &&
    ['success', 'failed', 'pending'].includes(value.status) &&
    typeof value.timestamp === 'number' &&
    typeof value.contractId === 'string' &&
    (value.fee === undefined || typeof value.fee === 'string')
  );
}

function loadMockContractConfig(env, defaults) {
  const contractId = env.NEXT_PUBLIC_CONTRACT_ID?.trim();
  const functions = parseJson(env.NEXT_PUBLIC_CONTRACT_FUNCTIONS, (value) =>
    Array.isArray(value) && value.length > 0 && value.every(isContractFunction),
  );
  const results = parseJson(env.NEXT_PUBLIC_CONTRACT_MOCK_RESULTS, isRecord);
  const transactions = parseJson(env.NEXT_PUBLIC_CONTRACT_MOCK_TRANSACTIONS, (value) =>
    Array.isArray(value) && value.every(isTransaction),
  );

  return {
    contractId: contractId || defaults.contractId,
    hasContractIdOverride: Boolean(contractId),
    functions: functions || defaults.functions,
    results: results || defaults.results,
    transactions: transactions || defaults.transactions,
  };
}

function resolveMockResult(functionName, overrides, fallback) {
  return Object.prototype.hasOwnProperty.call(overrides, functionName)
    ? overrides[functionName]
    : fallback();
}

module.exports = { loadMockContractConfig, resolveMockResult };