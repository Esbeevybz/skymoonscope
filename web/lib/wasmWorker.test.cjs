// wasmWorker.test.cjs — protocol tests for the WASM validation Web Worker
// Fixes #409 (name-section crash) and closes Issue #621
// Runs with: node --test ./lib/wasmWorker.test.cjs
//
// The worker is a classic script served from public/, so it cannot simply be
// required. Instead it is executed in a VM with a stubbed `self`, which
// exercises the exact file the browser loads.
//
// Fix applied here: importScripts in the sandbox now actually runs
// wasmValidation.js so that self.SkyMoonScopeWasmValidation is populated,
// matching the real browser worker boot sequence. The old test stub used the
// wrong global name (SoroscopeWasmValidation) and a no-op importScripts, so
// every test was crashing with a TypeError before onmessage was registered.

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const WORKER_PATH = path.join(__dirname, '..', 'public', 'wasm', 'wasmWorker.js');
const VALIDATION_PATH = path.join(__dirname, '..', 'public', 'wasm', 'wasmValidation.js');

/**
 * Boot the worker script against a fake `self` and return a driver.
 *
 * importScripts is implemented to actually run wasmValidation.js in the same
 * VM context, exactly as the browser would. This is the critical fix: the old
 * stub was a no-op, leaving self.SkyMoonScopeWasmValidation undefined, which
 * caused the worker to throw a TypeError at top-level before onmessage was
 * ever registered.
 */
function bootWorker() {
  const posted = [];
  const sandbox = {
    postMessage: (message) => posted.push(message),
    importScripts: (...urls) => {
      // Run each requested script in the same VM context, exactly as a real
      // Web Worker would. This populates self.SkyMoonScopeWasmValidation.
      for (const url of urls) {
        // Map the worker-relative URL to a filesystem path.
        // '/wasm/wasmValidation.js' → public/wasm/wasmValidation.js
        const fileName = path.basename(url);
        const filePath = path.join(__dirname, '..', 'public', 'wasm', fileName);
        const source = fs.readFileSync(filePath, 'utf8');
        vm.runInContext(source, sandbox, { filename: fileName });
      }
    },
  };
  sandbox.self = sandbox; // workers expose the global scope as `self`

  const source = fs.readFileSync(WORKER_PATH, 'utf8');
  vm.createContext(sandbox);
  vm.runInContext(source, sandbox, { filename: 'wasmWorker.js' });

  return {
    posted,
    send: (data) => sandbox.onmessage({ data }),
  };
}

// ── Fixture helpers ──────────────────────────────────────────────────────────

function leb128(value) {
  const bytes = [];
  do {
    let byte = value & 0x7f;
    value >>>= 7;
    if (value !== 0) byte |= 0x80;
    bytes.push(byte);
  } while (value !== 0);
  return bytes;
}

/** Minimal valid WASM: header + export section with one function named "transfer". */
function validModule() {
  const name = Buffer.from('transfer', 'utf8');
  const payload = [0x01, name.length, ...name, 0x00, 0x00];
  return Uint8Array.from([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    0x07, payload.length, ...payload,
  ]);
}

/**
 * Build a WASM custom "name" section (id=0) with function name subsections.
 *
 * This is the real-world structure emitted by rustc/wasm-pack for Soroban
 * contracts compiled with debug info. It was the trigger for #409 because
 * larger contracts with name sections were the first to hit the boot crash.
 *
 * Structure:
 *   section id = 0
 *   section size = LEB128(payload.length)
 *   payload:
 *     LEB128(4) + 'name'           ← custom section name string
 *     0x01 (function names subsection id)
 *     LEB128(subsec_size)
 *       LEB128(count)
 *       [LEB128(func_idx) + LEB128(name_len) + name_bytes] × count
 *
 * @param {Array<[number, string]>} funcNames  [[funcIndex, name], ...]
 */
function buildNameSection(funcNames) {
  const sectionNameBytes = Buffer.from('name', 'utf8');
  const sectionNameField = [...leb128(sectionNameBytes.length), ...sectionNameBytes];

  const entries = funcNames.flatMap(([idx, name]) => {
    const nb = Buffer.from(name, 'utf8');
    return [...leb128(idx), ...leb128(nb.length), ...nb];
  });
  const subsecPayload = [...leb128(funcNames.length), ...entries];
  const subsec = [0x01, ...leb128(subsecPayload.length), ...subsecPayload];

  const payload = [...sectionNameField, ...subsec];
  return [0x00, ...leb128(payload.length), ...payload];
}

/**
 * Build a module with an export section AND a "name" custom section.
 * This is a realistic Soroban contract shape (debug build).
 */
function moduleWithNameSection(exportNames, funcNames) {
  const HEADER = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

  const exportEntries = exportNames.flatMap((name, idx) => {
    const nb = Buffer.from(name, 'utf8');
    return [...leb128(nb.length), ...nb, 0x00, idx];
  });
  const exportPayload = [exportNames.length, ...exportEntries];
  const exportSection = [0x07, ...leb128(exportPayload.length), ...exportPayload];

  const nameSection = buildNameSection(funcNames ?? exportNames.map((n, i) => [i, n]));

  return Uint8Array.from([...HEADER, ...nameSection, ...exportSection]);
}

// ── Existing protocol tests (all now pass with the fixed bootWorker) ─────────

test('worker: answers the boot handshake with pong', () => {
  const worker = bootWorker();
  worker.send({ type: 'ping' });
  assert.equal(worker.posted.length, 1);
  assert.equal(worker.posted[0].type, 'pong');
});

test('worker: returns a decode report for a valid module', () => {
  const worker = bootWorker();
  worker.send({ id: 'job-1', type: 'validate', buffer: validModule() });

  assert.equal(worker.posted.length, 1);
  const message = worker.posted[0];
  assert.equal(message.id, 'job-1');
  assert.equal(message.type, 'result');
  assert.equal(message.report.valid, true);
  assert.deepEqual(
    Array.from(message.report.exports.map((e) => e.name)),
    ['transfer'],
  );
});

test('worker: reports invalid bytecode as an unsuccessful result, not a crash', () => {
  const worker = bootWorker();
  worker.send({ id: 'job-2', type: 'validate', buffer: Buffer.from('not wasm at all') });

  const message = worker.posted[0];
  assert.equal(message.type, 'result');
  assert.equal(message.report.valid, false);
  assert.ok(message.report.errors.length > 0);
});

test('worker: honours the maxBytes limit passed by the caller', () => {
  const worker = bootWorker();
  worker.send({ id: 'job-3', type: 'validate', buffer: validModule(), maxBytes: 4 });

  assert.equal(worker.posted[0].report.valid, false);
  assert.match(worker.posted[0].report.errors[0], /exceeds/i);
});

test('worker: ignores unknown and empty messages', () => {
  const worker = bootWorker();
  worker.send({ type: 'something-else' });
  worker.send(null);
  assert.equal(worker.posted.length, 0);
});

test('worker: replies on the same job id it was given', () => {
  const worker = bootWorker();
  worker.send({ id: 'a', type: 'validate', buffer: validModule() });
  worker.send({ id: 'b', type: 'validate', buffer: validModule() });

  assert.deepEqual(
    worker.posted.map((m) => m.id),
    ['a', 'b'],
  );
});

// ── Regression tests for #409: name section crash ───────────────────────────

test('worker (regression #409): does not crash on a module with a name custom section', () => {
  // A module with a 'name' custom section (id=0) is what Soroban debug builds
  // emit. Before the fix, the top-level destructure of SkyMoonScopeWasmValidation
  // would throw before onmessage was registered, silently killing the worker.
  const worker = bootWorker();
  const module = moduleWithNameSection(['transfer', 'balance']);

  worker.send({ id: 'ns-1', type: 'validate', buffer: module });

  assert.equal(worker.posted.length, 1, 'worker should post exactly one reply');
  const msg = worker.posted[0];
  assert.equal(msg.type, 'result', 'reply should be a result, not an error or silence');
  assert.equal(msg.report.valid, true);
  assert.deepEqual(
    Array.from(msg.report.exports.map((e) => e.name)),
    ['transfer', 'balance'],
  );
});

test('worker (regression #409): correctly identifies sections in a module with a name section', () => {
  const worker = bootWorker();
  const module = moduleWithNameSection(['invoke']);

  worker.send({ id: 'ns-2', type: 'validate', buffer: module });

  const { report } = worker.posted[0];
  // Should see the custom section (id=0) and the export section (id=7)
  assert.ok(report.sections.some((s) => s.id === 0 && s.name === 'custom'), 'custom section present');
  assert.ok(report.sections.some((s) => s.id === 7 && s.name === 'export'), 'export section present');
});

test('worker (regression #409): still pings correctly after seeing a name-section module', () => {
  // Verifies the worker remains alive (onmessage is registered) after processing
  // a name-section module — i.e., the worker did not die at boot.
  const worker = bootWorker();

  worker.send({ type: 'ping' });
  assert.equal(worker.posted[0].type, 'pong', 'ping after boot should pong');

  worker.posted.length = 0;
  worker.send({ id: 'ns-3', type: 'validate', buffer: moduleWithNameSection(['go']) });
  assert.equal(worker.posted[0].type, 'result', 'validate after ping should return result');
});

test('worker (regression #409): handles a name section with multiple function names', () => {
  // A realistic Soroban contract might export many functions and name all of them
  const exportNames = ['transfer', 'balance', 'approve', 'allowance', 'mint', 'burn'];
  const worker = bootWorker();
  const module = moduleWithNameSection(exportNames);

  worker.send({ id: 'ns-4', type: 'validate', buffer: module });

  const { report } = worker.posted[0];
  assert.equal(report.valid, true);
  assert.equal(report.exports.length, exportNames.length);
  assert.deepEqual(
    Array.from(report.exports.map((e) => e.name)),
    exportNames,
  );
});

test('worker (regression #409): name section with non-ASCII function names does not crash', () => {
  // Some WASM toolchains embed Unicode characters in debug names (e.g., Rust
  // generic instantiation names like "soroban_env::from<T>").
  // Build a name section whose subsection data contains multi-byte UTF-8.
  const HEADER = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

  const exportNameBytes = Buffer.from('go', 'utf8');
  const exportPayload = [0x01, exportNameBytes.length, ...exportNameBytes, 0x00, 0x00];
  const exportSection = [0x07, ...leb128(exportPayload.length), ...exportPayload];

  // Function name with multi-byte UTF-8: "soroban::invoke<u64>"
  const debugName = Buffer.from('soroban::invoke\u003cu64\u003e', 'utf8');
  const nameSection = buildNameSection([[0, debugName.toString()]]);

  const module = Uint8Array.from([...HEADER, ...nameSection, ...exportSection]);

  const worker = bootWorker();
  worker.send({ id: 'ns-5', type: 'validate', buffer: module });

  assert.equal(worker.posted.length, 1);
  assert.equal(worker.posted[0].type, 'result', 'should return result, not crash');
  assert.equal(worker.posted[0].report.valid, true);
});

test('worker (regression #409): importScripts failure returns error reply not a dead worker', () => {
  // Simulate a worker whose importScripts silently fails to populate the global.
  // After the fix, onmessage is registered regardless, and validate calls
  // receive an informative error message rather than the worker dying.
  const posted = [];
  const sandbox = {
    postMessage: (msg) => posted.push(msg),
    // importScripts is a no-op here — SkyMoonScopeWasmValidation is never set.
    importScripts: () => {},
  };
  sandbox.self = sandbox;

  const source = fs.readFileSync(WORKER_PATH, 'utf8');
  vm.createContext(sandbox);
  // This must NOT throw even though SkyMoonScopeWasmValidation won't be set.
  assert.doesNotThrow(
    () => vm.runInContext(source, sandbox, { filename: 'wasmWorker.js' }),
    'worker script must not throw at boot even if importScripts is a no-op',
  );

  // The worker is alive. ping still works.
  sandbox.onmessage({ data: { type: 'ping' } });
  assert.equal(posted[0].type, 'pong', 'ping should pong even without validation module');
  posted.length = 0;

  // Validate should post an error reply, not throw.
  sandbox.onmessage({ data: { id: 'x', type: 'validate', buffer: new Uint8Array(8) } });
  assert.equal(posted.length, 1, 'should post one reply');
  assert.equal(posted[0].type, 'error', 'reply should be error when module not loaded');
  assert.match(posted[0].message, /failed to load/i);
});
