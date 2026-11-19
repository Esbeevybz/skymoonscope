// wasmFunctionExtractor.test.js — unit tests for the client-side WASM
// export-section parser used to refresh the search index after a contract
// upload (issue #393).
//
// Runs with: node --test lib/__tests__/wasmFunctionExtractor.test.js

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { extractFunctionNamesFromWasm } = require('../wasmFunctionExtractor.js');

// ─── WASM binary builder helpers ─────────────────────────────────────────────
//
// These helpers produce the absolute minimum valid WASM binary structure
// needed to exercise the export-section parser. They intentionally avoid
// any third-party WASM tooling so the tests have zero extra dependencies.

/** Encode a non-negative integer as an unsigned LEB128 byte sequence. */
function encodeULEB128(value) {
  const bytes = [];
  do {
    let byte = value & 0x7f;
    value >>>= 7;
    if (value !== 0) byte |= 0x80;
    bytes.push(byte);
  } while (value !== 0);
  return Buffer.from(bytes);
}

/** Encode a UTF-8 string as a LEB128 length prefix followed by raw bytes. */
function encodeString(str) {
  const encoded = Buffer.from(str, 'utf8');
  return Buffer.concat([encodeULEB128(encoded.length), encoded]);
}

/**
 * Build a minimal valid WASM binary containing a type section (required
 * even if empty) and an export section with the given function entries.
 *
 * @param {Array<{name: string, index: number}>} exports
 * @returns {ArrayBuffer}
 */
function buildWasm(exports) {
  const MAGIC = Buffer.from([0x00, 0x61, 0x73, 0x6d]);
  const VERSION = Buffer.from([0x01, 0x00, 0x00, 0x00]);

  // Type section (id=1): zero types — satisfies the spec minimum.
  const typeSection = Buffer.concat([
    Buffer.from([0x01]),          // section id
    encodeULEB128(1),             // section size: 1 byte
    encodeULEB128(0),             // type count: 0
  ]);

  if (exports.length === 0) {
    // No export section at all — valid WASM with no exports.
    const wasm = Buffer.concat([MAGIC, VERSION, typeSection]);
    return wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength);
  }

  // Export section (id=7)
  const exportEntries = exports.map(({ name, index, kind = 0 }) =>
    Buffer.concat([
      encodeString(name),
      Buffer.from([kind]),        // 0 = function, 1 = table, 2 = memory, 3 = global
      encodeULEB128(index),
    ])
  );

  const exportPayload = Buffer.concat([
    encodeULEB128(exports.length),
    ...exportEntries,
  ]);

  const exportSection = Buffer.concat([
    Buffer.from([0x07]),          // section id: export
    encodeULEB128(exportPayload.length),
    exportPayload,
  ]);

  const wasm = Buffer.concat([MAGIC, VERSION, typeSection, exportSection]);
  return wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength);
}

/** Convenience: WASM with a broken magic number. */
function buildBrokenMagic() {
  const buf = Buffer.alloc(8);
  buf.writeUInt32BE(0xDEADBEEF, 0); // wrong magic
  buf.writeUInt32LE(1, 4);          // correct version
  return buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
}

// ─── Tests ───────────────────────────────────────────────────────────────────

test('returns empty array for a completely empty buffer', () => {
  assert.deepEqual(extractFunctionNamesFromWasm(new ArrayBuffer(0)), []);
});

test('returns empty array for a buffer that is too short to hold a header', () => {
  assert.deepEqual(extractFunctionNamesFromWasm(new ArrayBuffer(4)), []);
});

test('returns empty array when the WASM magic is wrong', () => {
  assert.deepEqual(extractFunctionNamesFromWasm(buildBrokenMagic()), []);
});

test('returns empty array for valid WASM with no export section', () => {
  const wasm = buildWasm([]);
  assert.deepEqual(extractFunctionNamesFromWasm(wasm), []);
});

test('extracts a single function export', () => {
  const wasm = buildWasm([{ name: 'transfer', index: 0 }]);
  assert.deepEqual(extractFunctionNamesFromWasm(wasm), ['transfer']);
});

test('extracts multiple function exports and sorts them alphabetically', () => {
  const wasm = buildWasm([
    { name: 'mint',     index: 2 },
    { name: 'balance',  index: 1 },
    { name: 'transfer', index: 0 },
  ]);
  assert.deepEqual(extractFunctionNamesFromWasm(wasm), ['balance', 'mint', 'transfer']);
});

test('excludes exports whose names start with underscore', () => {
  const wasm = buildWasm([
    { name: 'transfer',   index: 0 },
    { name: '_internal',  index: 1 },
    { name: '__alloc',    index: 2 },
    { name: 'balance',    index: 3 },
  ]);
  assert.deepEqual(extractFunctionNamesFromWasm(wasm), ['balance', 'transfer']);
});

test('excludes non-function exports (table, memory, global)', () => {
  const wasm = buildWasm([
    { name: 'transfer', index: 0, kind: 0 }, // function — keep
    { name: 'memory',   index: 0, kind: 2 }, // memory — exclude
    { name: '__table',  index: 0, kind: 1 }, // table — exclude (also _ prefix)
    { name: 'balance',  index: 1, kind: 0 }, // function — keep
  ]);
  assert.deepEqual(extractFunctionNamesFromWasm(wasm), ['balance', 'transfer']);
});

test('returns empty array when all exports are non-function or internal', () => {
  const wasm = buildWasm([
    { name: '_init',  index: 0, kind: 0 },
    { name: 'memory', index: 0, kind: 2 },
  ]);
  assert.deepEqual(extractFunctionNamesFromWasm(wasm), []);
});

test('does not throw and returns empty array for a non-WASM ArrayBuffer', () => {
  const buf = Buffer.from('not wasm at all, just a string');
  const ab = buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
  assert.doesNotThrow(() => extractFunctionNamesFromWasm(ab));
  assert.deepEqual(extractFunctionNamesFromWasm(ab), []);
});

test('handles a WASM binary with sections before the export section', () => {
  // buildWasm already puts a type section before the export section, so a
  // successful result here confirms the section-skipping loop works.
  const wasm = buildWasm([
    { name: 'invoke', index: 0 },
    { name: 'query',  index: 1 },
  ]);
  assert.deepEqual(extractFunctionNamesFromWasm(wasm), ['invoke', 'query']);
});

test('produces the same result called twice on the same buffer (no side effects)', () => {
  const wasm = buildWasm([{ name: 'transfer', index: 0 }]);
  const first  = extractFunctionNamesFromWasm(wasm);
  const second = extractFunctionNamesFromWasm(wasm);
  assert.deepEqual(first, second);
});

test('output integrates correctly with buildCommandRegistry', () => {
  // End-to-end: parsed names → ContractFunction stubs → search commands.
  const { buildCommandRegistry } = require('../searchCommands.js');

  const wasm = buildWasm([
    { name: 'mint',     index: 0 },
    { name: 'transfer', index: 1 },
  ]);
  const names = extractFunctionNamesFromWasm(wasm);
  assert.deepEqual(names, ['mint', 'transfer']);

  // Simulate what SimulationContext does after parsing.
  const functions = names.map((name) => ({ name, inputs: [] }));
  const registry = buildCommandRegistry({ functions });

  const fnIds = registry.filter((c) => c.id.startsWith('fn-')).map((c) => c.id);
  assert.deepEqual(fnIds.sort(), ['fn-mint', 'fn-transfer']);
});
