// WasmSectionSummary.test.cjs — tests for #408
// Covers:
//   1. parseDataSection (new)
//   2. parseImportSection with non-function import kinds (bug fix)
//   3. validateWasmModule now populates dataSegments[]
//   4. WasmSectionSummaryPanel rendering logic (pure / headless)
//
// Runs with: node --test ./components/WasmSectionSummary.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  validateWasmModule,
  parseDataSection,
  readVarUint32,
} = require('../lib/wasmValidation');

// ── Fixture helpers ──────────────────────────────────────────────────────────

const HEADER = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

function leb128(value) {
  // Encode a non-negative integer as unsigned LEB128.
  const bytes = [];
  do {
    let byte = value & 0x7f;
    value >>>= 7;
    if (value !== 0) byte |= 0x80;
    bytes.push(byte);
  } while (value !== 0);
  return bytes;
}

function encodeString(text) {
  const bytes = Array.from(Buffer.from(text, 'utf8'));
  return [...leb128(bytes.length), ...bytes];
}

/**
 * Build a minimal WASM module from raw sections.
 * sections: [[id, payloadBytes], ...]
 */
function buildModule(sections = []) {
  const body = sections.flatMap(([id, payload]) => [
    id,
    ...leb128(payload.length),
    ...payload,
  ]);
  return Uint8Array.from([...HEADER, ...body]);
}

function exportSection(names) {
  // section id 7
  const entries = names.flatMap((name, idx) => [...encodeString(name), 0x00, idx]);
  return [0x07, [names.length, ...entries]];
}

/**
 * Build an import section with entries of varying kinds.
 * entries: [{ module, name, kind, descriptor }]
 *   descriptor for function: typeIndex (number)
 *   descriptor for table:    null (we'll produce a minimal reftype + limits)
 *   descriptor for memory:   null (minimal limits)
 *   descriptor for global:   null (minimal valtype+mut)
 */
function buildImportSectionBytes(entries) {
  const payload = [
    ...leb128(entries.length),
    ...entries.flatMap(({ module: mod, name, kind, typeIndex }) => {
      const nameBytes = [...encodeString(mod), ...encodeString(name)];
      let descriptor;
      if (kind === 0) {
        // function: type index LEB128
        descriptor = leb128(typeIndex ?? 0);
      } else if (kind === 1) {
        // table: funcref (0x70) + limits flag=0, min=0
        descriptor = [0x70, 0x00, ...leb128(0)];
      } else if (kind === 2) {
        // memory: limits flag=0, min=0
        descriptor = [0x00, ...leb128(0)];
      } else if (kind === 3) {
        // global: i32 (0x7f) + immutable (0x00)
        descriptor = [0x7f, 0x00];
      } else {
        descriptor = [];
      }
      return [...nameBytes, kind, ...descriptor];
    }),
  ];
  return [0x02, payload];
}

/**
 * Build a data section with simple active segments.
 * segments: [{ offset, data: Uint8Array }]
 */
function buildDataSectionBytes(segments) {
  // Each segment: kind=0 (active, memory 0) + i32.const offset (0x41 + leb128 + 0x0b) + data
  const payload = [
    ...leb128(segments.length),
    ...segments.flatMap(({ offset = 0, data = [] }) => [
      ...leb128(0), // segment kind 0 (active)
      0x41, ...leb128(offset), 0x0b, // i32.const <offset> end
      ...leb128(data.length), ...data,
    ]),
  ];
  return [11, payload]; // section id 11 (data)
}

// ── parseDataSection ─────────────────────────────────────────────────────────

test('parseDataSection: parses a single active segment', () => {
  const module = buildModule([
    exportSection(['init']),
    buildDataSectionBytes([{ offset: 0x1000, data: [0xde, 0xad, 0xbe, 0xef] }]),
  ]);

  const report = validateWasmModule(module);
  assert.equal(report.valid, true);
  assert.equal(report.dataSegments.length, 1);

  const seg = report.dataSegments[0];
  assert.equal(seg.index, 0);
  assert.equal(seg.kind, 'active');
  assert.equal(seg.memoryIndex, 0);
  assert.equal(seg.offsetValue, 0x1000);
  assert.equal(seg.byteLength, 4);
});

test('parseDataSection: parses multiple active segments', () => {
  const module = buildModule([
    exportSection(['main']),
    buildDataSectionBytes([
      { offset: 0, data: [1, 2, 3] },
      { offset: 100, data: [4, 5] },
      { offset: 200, data: [] },
    ]),
  ]);

  const report = validateWasmModule(module);
  assert.equal(report.valid, true);
  assert.equal(report.dataSegments.length, 3);
  assert.equal(report.dataSegments[0].byteLength, 3);
  assert.equal(report.dataSegments[1].offsetValue, 100);
  assert.equal(report.dataSegments[2].byteLength, 0);
});

test('parseDataSection: passive segment has null memoryIndex and offsetValue', () => {
  // Passive segment: kind=1 + data only (no memory index or offset)
  const data = [0xca, 0xfe];
  const payload = [
    ...leb128(1), // count: 1 segment
    ...leb128(1), // segment kind: 1 (passive)
    ...leb128(data.length), ...data,
  ];
  const module = buildModule([
    exportSection(['noop']),
    [11, payload], // section id 11 (data)
  ]);

  const report = validateWasmModule(module);
  assert.equal(report.valid, true);
  assert.equal(report.dataSegments.length, 1);
  const seg = report.dataSegments[0];
  assert.equal(seg.kind, 'passive');
  assert.equal(seg.memoryIndex, null);
  assert.equal(seg.offsetValue, null);
  assert.equal(seg.byteLength, 2);
});

test('parseDataSection: module with no data section has empty dataSegments', () => {
  const module = buildModule([exportSection(['transfer'])]);
  const report = validateWasmModule(module);
  assert.equal(report.valid, true);
  assert.deepEqual(report.dataSegments, []);
});

test('validateWasmModule: dataSegments is present on a valid report', () => {
  const report = validateWasmModule(buildModule([exportSection(['go'])]));
  assert.ok(Array.isArray(report.dataSegments));
});

test('validateWasmModule: dataSegments is present even on an invalid report', () => {
  const report = validateWasmModule(Buffer.from('not wasm'));
  assert.ok(Array.isArray(report.dataSegments));
});

// ── parseImportSection (bug fix: non-function kinds) ─────────────────────────

test('parseImportSection: parses a pure function import', () => {
  const module = buildModule([
    buildImportSectionBytes([
      { module: 'env', name: 'log', kind: 0, typeIndex: 1 },
    ]),
    exportSection(['main']),
  ]);

  const report = validateWasmModule(module);
  assert.equal(report.valid, true);
  assert.equal(report.imports.length, 1);
  assert.equal(report.imports[0].module, 'env');
  assert.equal(report.imports[0].name, 'log');
  assert.equal(report.imports[0].kind, 'function');
});

test('parseImportSection: parses a memory import', () => {
  const module = buildModule([
    buildImportSectionBytes([
      { module: 'env', name: 'memory', kind: 2 },
    ]),
    exportSection(['main']),
  ]);

  const report = validateWasmModule(module);
  assert.equal(report.valid, true);
  assert.equal(report.imports.length, 1);
  assert.equal(report.imports[0].kind, 'memory');
});

test('parseImportSection: parses a global import', () => {
  const module = buildModule([
    buildImportSectionBytes([
      { module: 'env', name: '__stack_pointer', kind: 3 },
    ]),
    exportSection(['main']),
  ]);

  const report = validateWasmModule(module);
  assert.equal(report.valid, true);
  assert.equal(report.imports.length, 1);
  assert.equal(report.imports[0].kind, 'global');
});

test('parseImportSection: parses a table import', () => {
  const module = buildModule([
    buildImportSectionBytes([
      { module: 'env', name: '__indirect_function_table', kind: 1 },
    ]),
    exportSection(['main']),
  ]);

  const report = validateWasmModule(module);
  assert.equal(report.valid, true);
  assert.equal(report.imports.length, 1);
  assert.equal(report.imports[0].kind, 'table');
});

test('parseImportSection: parses mixed import kinds without stopping early', () => {
  const module = buildModule([
    buildImportSectionBytes([
      { module: 'env', name: 'memory', kind: 2 },
      { module: 'env', name: '__stack_pointer', kind: 3 },
      { module: 'env', name: 'log', kind: 0, typeIndex: 0 },
    ]),
    exportSection(['main']),
  ]);

  const report = validateWasmModule(module);
  assert.equal(report.valid, true);
  // All three imports must be decoded — previously the parser stopped after
  // the memory import (kind !== 0 broke out of the loop).
  assert.equal(report.imports.length, 3);
  assert.equal(report.imports[0].kind, 'memory');
  assert.equal(report.imports[1].kind, 'global');
  assert.equal(report.imports[2].kind, 'function');
});

// ── WasmSectionSummary panel logic (pure / headless) ────────────────────────
// We test the report-shaping logic without a real DOM (no jsdom required).

test('panel logic: counts per tab are derived from report fields', () => {
  const report = validateWasmModule(
    buildModule([
      buildImportSectionBytes([
        { module: 'env', name: 'memory', kind: 2 },
        { module: 'soroban', name: '__check_auth', kind: 0, typeIndex: 1 },
      ]),
      exportSection(['transfer', 'balance']),
      buildDataSectionBytes([
        { offset: 0, data: [1, 2, 3, 4, 5, 6, 7, 8] },
      ]),
    ])
  );

  assert.equal(report.imports.length, 2, 'imports count');
  assert.equal(report.exports.length, 2, 'exports count');
  assert.equal(report.dataSegments.length, 1, 'data segment count');
  // sections = import(2) + export(7) + data(11) = 3
  assert.equal(report.sections.length, 3, 'sections count');
});

test('panel logic: report with no data section shows 0 data segments', () => {
  const report = validateWasmModule(buildModule([exportSection(['go'])]));
  assert.equal(report.dataSegments.length, 0);
  // Sections contains only the export section
  assert.equal(report.sections.length, 1);
  assert.equal(report.sections[0].name, 'export');
});

test('panel logic: warnings are present for an exportless module', () => {
  const report = validateWasmModule(buildModule([]));
  assert.ok(report.warnings.some((w) => /exports nothing/i.test(w)));
});

test('panel logic: offsetValue is null for passive segments', () => {
  const data = [0x00];
  const payload = [...leb128(1), ...leb128(1), ...leb128(data.length), ...data];
  const module = buildModule([exportSection(['noop']), [11, payload]]); // id 11 = data section
  const report = validateWasmModule(module);
  assert.equal(report.dataSegments[0].offsetValue, null);
});

test('panel logic: active segment byteLength tracks initialiser data size', () => {
  const initData = new Array(128).fill(0xff);
  const module = buildModule([
    exportSection(['store']),
    buildDataSectionBytes([{ offset: 0, data: initData }]),
  ]);
  const report = validateWasmModule(module);
  assert.equal(report.dataSegments[0].byteLength, 128);
});

test('panel logic: validateWasmModule populates sorobanMeta for modules with Soroban sections', () => {
  // Build a custom section for contractmetav0:
  // ScMetaEntry::ScMetaV0: discriminant 0, key 'rssdkver', value '22.0.0'
  function xdrStr(str) {
    const bytes = Array.from(Buffer.from(str, 'utf8'));
    const len = bytes.length;
    const pad = (4 - (len % 4)) % 4;
    return [
      0x00, 0x00, 0x00, len,
      ...bytes,
      ...new Array(pad).fill(0),
    ];
  }
  const metaPayload = [
    0x00, 0x00, 0x00, 0x00, // discriminant 0
    ...xdrStr('rssdkver'),
    ...xdrStr('22.0.0'),
  ];
  const sectionName = Array.from(Buffer.from('contractmetav0', 'utf8'));
  const customSectionBytes = [
    ...leb128(sectionName.length),
    ...sectionName,
    ...metaPayload,
  ];

  const module = buildModule([
    exportSection(['init']),
    [0x00, customSectionBytes], // id 0 = custom section
  ]);

  const report = validateWasmModule(module);
  assert.ok(report.sorobanMeta, 'sorobanMeta should be populated');
  assert.equal(report.sorobanMeta.contractMeta.length, 1);
  assert.equal(report.sorobanMeta.contractMeta[0].key, 'rssdkver');
  assert.equal(report.sorobanMeta.contractMeta[0].value, '22.0.0');
});

test('panel logic: validateWasmModule does not populate sorobanMeta when no Soroban sections exist', () => {
  const module = buildModule([exportSection(['init'])]);
  const report = validateWasmModule(module);
  assert.equal(report.sorobanMeta, undefined);
});

