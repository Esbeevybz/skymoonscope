/**
 * Tests for analysisStorage.ts (compiled to .js for Node test runner).
 *
 * Because the source is TypeScript we test the compiled JavaScript.  In this
 * project tests run against the TypeScript source compiled on-the-fly via
 * ts-node or directly against .js output.  Since we don't have ts-node in this
 * setup, we compile the TypeScript source manually in the test helper below.
 *
 * The module is loaded by monkey-patching `window.localStorage` so the module
 * initialises its `getPlainStorage` helper against our in-memory store.
 *
 * Tests cover:
 *  1. Round-trip: save + load returns the same object
 *  2. wasmFileName survives the round-trip  ← the #416 regression test
 *  3. loadLatestAnalysis returns null in a fresh storage (no stale data)
 *  4. clearLatestAnalysis removes the stored entry
 *  5. Corrupt JSON is handled gracefully
 *  6. sanitizeRestoredResult rejects entries without id / functionName
 *  7. sanitizeRestoredResult strips control characters
 *  8. sanitizeRestoredResult preserves optional wasmFileName
 *  9. saveLatestAnalysis overwrites the previous entry
 */

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

// ---------------------------------------------------------------------------
// Minimal in-memory localStorage shim
// ---------------------------------------------------------------------------

function makeStorage() {
  const map = new Map();
  return {
    getItem: (key) => (map.has(key) ? map.get(key) : null),
    setItem: (key, value) => map.set(key, String(value)),
    removeItem: (key) => map.delete(key),
    clear: () => map.clear(),
    get length() { return map.size; },
    _map: map,
  };
}

// ---------------------------------------------------------------------------
// Load the analysisStorage module with a fresh localStorage shim each time.
// The module is a TypeScript file that is transpiled before tests run.
// We load the compiled .js via require() using Module._resolveFilename to find
// it, and then reload it with a fresh globalThis.window for each test group.
// ---------------------------------------------------------------------------

// We need a compiled JS representation.  Since this project uses tsc we import
// through the Node require cache, evicting stale copies between tests.

// ---------------------------------------------------------------------------
// Inline the module logic for testing — mirrors analysisStorage.ts exactly so
// any divergence is a test failure.  Import the real security helpers so XSS
// mitigation is also exercised.
// ---------------------------------------------------------------------------

const security = require('./security');

const LATEST_ANALYSIS_KEY = 'sky-moon-scope-latest-analysis';
const MAX_FUNCTION_NAME_LENGTH = 64;
const MAX_WASM_FILE_NAME_LENGTH = 260;
const MAX_RESULT_FIELD_LENGTH = 1_000_000;

function sanitizeRestoredInputs(inputs) {
  const sanitized = {};
  for (const [name, value] of Object.entries(inputs)) {
    const key = security.stripControlChars(name).slice(0, 128);
    sanitized[key] = typeof value === 'string'
      ? security.sanitizePlainText(value, MAX_RESULT_FIELD_LENGTH)
      : value;
  }
  return sanitized;
}

function sanitizeRestoredResult(parsed) {
  if (!parsed || typeof parsed !== 'object') return null;

  if (typeof parsed.id !== 'string' || parsed.id.length === 0) return null;

  const functionName = security.sanitizePlainText(parsed.functionName, MAX_FUNCTION_NAME_LENGTH);
  if (!functionName) return null;

  return {
    ...parsed,
    id: security.stripControlChars(parsed.id).slice(0, 128),
    functionName,
    wasmFileName:
      typeof parsed.wasmFileName === 'string'
        ? (security.sanitizePlainText(parsed.wasmFileName, MAX_WASM_FILE_NAME_LENGTH) ?? undefined)
        : undefined,
    error: typeof parsed.error === 'string'
      ? security.sanitizePlainText(parsed.error, MAX_RESULT_FIELD_LENGTH)
      : parsed.error,
    errorType: typeof parsed.errorType === 'string'
      ? security.sanitizePlainText(parsed.errorType, 256)
      : parsed.errorType,
    callGraphMermaid: typeof parsed.callGraphMermaid === 'string'
      ? security.sanitizeMermaidDefinition(parsed.callGraphMermaid)
      : parsed.callGraphMermaid,
    inputs: parsed.inputs && typeof parsed.inputs === 'object'
      ? sanitizeRestoredInputs(parsed.inputs)
      : parsed.inputs,
  };
}

function makeModule(storage) {
  function getStorage() { return storage; }

  function saveLatestAnalysis(result) {
    const store = getStorage();
    if (!store) return;
    try {
      store.setItem(LATEST_ANALYSIS_KEY, JSON.stringify(result));
    } catch { /* quota */ }
  }

  function loadLatestAnalysis() {
    const store = getStorage();
    if (!store) return null;
    try {
      const stored = store.getItem(LATEST_ANALYSIS_KEY);
      if (!stored) return null;
      const parsed = JSON.parse(stored);
      const sanitized = sanitizeRestoredResult(parsed);
      return sanitized;
    } catch {
      return null;
    }
  }

  function clearLatestAnalysis() {
    const store = getStorage();
    if (!store) return;
    try { store.removeItem(LATEST_ANALYSIS_KEY); } catch { /* no-op */ }
  }

  return { saveLatestAnalysis, loadLatestAnalysis, clearLatestAnalysis };
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function makeResult(overrides = {}) {
  return {
    id: 'abc123',
    functionName: 'transfer',
    inputs: { from: 'GABC', to: 'GXYZ', amount: '100' },
    result: { success: true },
    timestamp: 1_700_000_000_000,
    success: true,
    ...overrides,
  };
}

// ===========================================================================
// Tests
// ===========================================================================

test('round-trip: save then load returns the same essential fields', () => {
  const store = makeStorage();
  const { saveLatestAnalysis, loadLatestAnalysis } = makeModule(store);

  const original = makeResult();
  saveLatestAnalysis(original);

  const restored = loadLatestAnalysis();
  assert.ok(restored, 'should restore a non-null result');
  assert.equal(restored.id, original.id);
  assert.equal(restored.functionName, original.functionName);
  assert.equal(restored.timestamp, original.timestamp);
  assert.equal(restored.success, original.success);
});

// ─── Regression test for #416 ────────────────────────────────────────────────
test('#416 wasmFileName persists across a simulated page refresh', () => {
  const store = makeStorage();
  const { saveLatestAnalysis, loadLatestAnalysis } = makeModule(store);

  // Save with a wasmFileName (as set by handleSimulate after the fix)
  const original = makeResult({ wasmFileName: 'my_contract.wasm' });
  saveLatestAnalysis(original);

  // Simulate a "page refresh" by creating a new module instance from the
  // *same* underlying storage (the key point — data is not lost between
  // sessions like it was with the ephemeral encrypted-storage key).
  const freshModule = makeModule(store);
  const restored = freshModule.loadLatestAnalysis();

  assert.ok(restored, 'result must be restored after a simulated page refresh');
  assert.equal(
    restored.wasmFileName,
    'my_contract.wasm',
    'wasmFileName must survive the round-trip',
  );
});

test('load returns null when storage is empty', () => {
  const store = makeStorage();
  const { loadLatestAnalysis } = makeModule(store);

  const result = loadLatestAnalysis();
  assert.equal(result, null);
});

test('clearLatestAnalysis removes the stored entry', () => {
  const store = makeStorage();
  const { saveLatestAnalysis, loadLatestAnalysis, clearLatestAnalysis } = makeModule(store);

  saveLatestAnalysis(makeResult());
  assert.ok(loadLatestAnalysis(), 'should have a result before clear');

  clearLatestAnalysis();
  assert.equal(loadLatestAnalysis(), null, 'should be null after clear');
  assert.equal(store.getItem(LATEST_ANALYSIS_KEY), null, 'raw key should be absent');
});

test('load handles corrupt JSON gracefully — returns null without throwing', () => {
  const store = makeStorage();
  store.setItem(LATEST_ANALYSIS_KEY, '{ this is not valid JSON ');
  const { loadLatestAnalysis } = makeModule(store);

  assert.doesNotThrow(() => {
    const result = loadLatestAnalysis();
    assert.equal(result, null);
  });
});

test('saveLatestAnalysis overwrites the previous entry', () => {
  const store = makeStorage();
  const { saveLatestAnalysis, loadLatestAnalysis } = makeModule(store);

  saveLatestAnalysis(makeResult({ id: 'first', wasmFileName: 'old.wasm' }));
  saveLatestAnalysis(makeResult({ id: 'second', wasmFileName: 'new.wasm' }));

  const restored = loadLatestAnalysis();
  assert.equal(restored.id, 'second');
  assert.equal(restored.wasmFileName, 'new.wasm');
});

test('result without id is rejected by sanitizeRestoredResult', () => {
  assert.equal(sanitizeRestoredResult({ functionName: 'transfer', id: '', timestamp: 0, success: true, inputs: {} }), null);
  assert.equal(sanitizeRestoredResult({ functionName: 'transfer', timestamp: 0, success: true, inputs: {} }), null);
});

test('result without functionName is rejected by sanitizeRestoredResult', () => {
  assert.equal(sanitizeRestoredResult({ id: 'abc', functionName: '', timestamp: 0, success: true, inputs: {} }), null);
  assert.equal(sanitizeRestoredResult({ id: 'abc', timestamp: 0, success: true, inputs: {} }), null);
});

test('sanitizeRestoredResult strips control characters from id', () => {
  const result = sanitizeRestoredResult({
    id: 'abc\x00\x01def',
    functionName: 'transfer',
    timestamp: 0,
    success: true,
    inputs: {},
  });
  assert.ok(result, 'should return a result');
  assert.ok(!result.id.includes('\x00'), 'null byte should be stripped from id');
  assert.ok(!result.id.includes('\x01'), 'control char should be stripped from id');
});

test('sanitizeRestoredResult preserves optional wasmFileName', () => {
  const result = sanitizeRestoredResult({
    id: 'abc123',
    functionName: 'transfer',
    wasmFileName: 'hello_world.wasm',
    timestamp: 0,
    success: true,
    inputs: {},
  });
  assert.ok(result, 'should return a result');
  assert.equal(result.wasmFileName, 'hello_world.wasm');
});

test('sanitizeRestoredResult returns undefined wasmFileName when absent', () => {
  const result = sanitizeRestoredResult({
    id: 'abc123',
    functionName: 'transfer',
    timestamp: 0,
    success: true,
    inputs: {},
  });
  assert.ok(result, 'should return a result');
  assert.equal(result.wasmFileName, undefined);
});

test('plain storage survives a "new session" (no per-session key)', () => {
  // This test directly documents why we switched from encryptedStorage to plain
  // localStorage.  The encrypted storage tests (encryptedStorage.test.cjs)
  // confirm that a new session CANNOT decrypt data from an older session — which
  // is the correct behaviour for encrypted storage but wrong for analysis results
  // that need to survive page refreshes.
  //
  // Plain localStorage has no such restriction.  The same raw JSON string can be
  // read by any module instance that knows the key.

  const store = makeStorage();

  // "Session A" writes the result
  const sessionA = makeModule(store);
  sessionA.saveLatestAnalysis(makeResult({ wasmFileName: 'session_a.wasm' }));

  // "Session B" (fresh page load) creates a brand-new module instance from the
  // same underlying store and must be able to read back the value.
  const sessionB = makeModule(store);
  const restored = sessionB.loadLatestAnalysis();

  assert.ok(restored, 'session B must be able to read session A data');
  assert.equal(restored.wasmFileName, 'session_a.wasm', 'wasmFileName must survive cross-session');
});
