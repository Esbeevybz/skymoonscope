// sorobanMeta.test.cjs — unit tests for Soroban custom section decoder (#413)
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  decodeSorobanMeta,
  decodeMetaSection,
  decodeSpecSection,
  findCustomSections,
  SPEC_TYPE_NAMES,
  _internal,
} = require('./sorobanMeta');

// ── Helpers to craft synthetic XDR & WASM binary chunks ──────────────────────

function u32be(n) {
  return [
    (n >>> 24) & 0xff,
    (n >>> 16) & 0xff,
    (n >>> 8) & 0xff,
    n & 0xff,
  ];
}

function i32be(n) {
  return u32be(n >>> 0);
}

function xdrString(str) {
  const bytes = Buffer.from(str, 'utf8');
  const len = bytes.length;
  const pad = (4 - (len % 4)) % 4;
  return [...u32be(len), ...bytes, ...new Array(pad).fill(0)];
}

function encodeLeb128(val) {
  const bytes = [];
  let n = val;
  do {
    let b = n & 0x7f;
    n >>>= 7;
    if (n !== 0) b |= 0x80;
    bytes.push(b);
  } while (n !== 0);
  return bytes;
}

function customSection(name, payload) {
  const nameBytes = Buffer.from(name, 'utf8');
  const sectionContent = [
    ...encodeLeb128(nameBytes.length),
    ...nameBytes,
    ...payload,
  ];
  return [
    0x00, // custom section ID
    ...encodeLeb128(sectionContent.length),
    ...sectionContent,
  ];
}

function makeWasmModule(customSectionsList = []) {
  const header = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
  const body = customSectionsList.flatMap((s) => customSection(s.name, s.payload));
  return Uint8Array.from([...header, ...body]);
}

// ── Tests ────────────────────────────────────────────────────────────────────

test('sorobanMeta: internal XDR primitives', () => {
  const bytes = Uint8Array.from([
    ...u32be(42),
    ...xdrString('hello'),
  ]);

  const intRes = _internal.readInt32BE(bytes, 0);
  assert.equal(intRes.value, 42);
  assert.equal(intRes.next, 4);

  const strRes = _internal.readXdrString(bytes, 4);
  assert.equal(strRes.value, 'hello');
  // 'hello' is 5 bytes + 3 pad = 8 bytes data + 4 bytes length = 12 bytes total
  assert.equal(strRes.next, 4 + 4 + 8);
});

test('sorobanMeta: decodeMetaSection parses valid key-values', () => {
  // ScMetaEntry::ScMetaV0 = discriminant 0
  const metaPayload = Uint8Array.from([
    ...i32be(0),
    ...xdrString('rssdkver'),
    ...xdrString('22.0.0'),
    ...i32be(0),
    ...xdrString('rsver'),
    ...xdrString('1.84.0'),
  ]);

  const { entries, errors } = decodeMetaSection(metaPayload);
  assert.equal(errors.length, 0);
  assert.equal(entries.length, 2);
  assert.deepEqual(entries[0], { key: 'rssdkver', value: '22.0.0' });
  assert.deepEqual(entries[1], { key: 'rsver', value: '1.84.0' });
});

test('sorobanMeta: decodeMetaSection handles corrupted payload gracefully', () => {
  const corruptPayload = Uint8Array.from([
    ...i32be(0),
    ...u32be(100), // claims 100 bytes, but payload ends
    0x01, 0x02,
  ]);

  const { entries, errors } = decodeMetaSection(corruptPayload);
  assert.equal(entries.length, 0);
  assert.equal(errors.length, 1);
  assert.match(errors[0], /contractmetav0 at offset 0/);
});

test('sorobanMeta: decodeSpecSection decodes function with inputs and outputs', () => {
  // ScSpecEntry::FunctionV0 = discriminant 0
  const specPayload = Uint8Array.from([
    ...i32be(0),
    ...xdrString('Initialize contract and set admin'), // doc
    ...xdrString('init'),                              // name
    ...u32be(2),                                       // 2 inputs
    // input 1: admin: Address (typeCode 17)
    ...xdrString('Admin address'),
    ...xdrString('admin'),
    ...u32be(17),
    // input 2: amount: U64 (typeCode 6)
    ...xdrString('Initial amount'),
    ...xdrString('amount'),
    ...u32be(6),
    // outputs: 1 output Bool (typeCode 1)
    ...u32be(1),
    ...u32be(1),
  ]);

  const { functions, types, errors } = decodeSpecSection(specPayload);
  assert.equal(errors.length, 0);
  assert.equal(functions.length, 1);
  assert.equal(types.length, 0);

  const fn = functions[0];
  assert.equal(fn.kind, 'function');
  assert.equal(fn.name, 'init');
  assert.equal(fn.doc, 'Initialize contract and set admin');
  assert.equal(fn.inputs.length, 2);
  assert.deepEqual(fn.inputs[0], { doc: 'Admin address', name: 'admin', type: 'Address' });
  assert.deepEqual(fn.inputs[1], { doc: 'Initial amount', name: 'amount', type: 'U64' });
  assert.deepEqual(fn.outputs, ['Bool']);
});

test('sorobanMeta: decodeSpecSection decodes struct and enum definitions', () => {
  // Struct: discriminant 1
  // UdtStructV0: doc, lib, name, fields[]
  const structPayload = [
    ...i32be(1),
    ...xdrString('State struct doc'),
    ...xdrString(''), // lib
    ...xdrString('State'),
    ...u32be(2), // 2 fields
    ...xdrString('count doc'),
    ...xdrString('count'),
    ...u32be(4), // U32
    ...xdrString('owner doc'),
    ...xdrString('owner'),
    ...u32be(17), // Address
  ];

  // Enum: discriminant 3
  // UdtEnumV0: doc, lib, name, cases[]
  const enumPayload = [
    ...i32be(3),
    ...xdrString('Status enum doc'),
    ...xdrString(''), // lib
    ...xdrString('Status'),
    ...u32be(2), // 2 cases
    ...xdrString('Active status'),
    ...xdrString('Active'),
    ...u32be(0),
    ...xdrString('Paused status'),
    ...xdrString('Paused'),
    ...u32be(1),
  ];

  const payload = Uint8Array.from([...structPayload, ...enumPayload]);
  const { functions, types, errors } = decodeSpecSection(payload);
  assert.equal(errors.length, 0);
  assert.equal(functions.length, 0);
  assert.equal(types.length, 2);

  assert.deepEqual(types[0], {
    kind: 'struct',
    name: 'State',
    doc: 'State struct doc',
  });
  assert.deepEqual(types[1], {
    kind: 'enum',
    name: 'Status',
    doc: 'Status enum doc',
  });
});

test('sorobanMeta: decodeSorobanMeta extracts both sections from WASM binary', () => {
  const metaBytes = [
    ...i32be(0),
    ...xdrString('rssdkver'),
    ...xdrString('22.0.0'),
  ];

  const specBytes = [
    ...i32be(0),
    ...xdrString('Get total supply'),
    ...xdrString('total_supply'),
    ...u32be(0), // 0 inputs
    ...u32be(1), // 1 output
    ...u32be(10), // U128
  ];

  const wasm = makeWasmModule([
    { name: 'contractmetav0', payload: metaBytes },
    { name: 'contractspecv0', payload: specBytes },
  ]);

  const report = decodeSorobanMeta(wasm);
  assert.equal(report.errors.length, 0);
  assert.equal(report.warnings.length, 0);

  assert.equal(report.contractMeta.length, 1);
  assert.deepEqual(report.contractMeta[0], { key: 'rssdkver', value: '22.0.0' });

  assert.equal(report.specFunctions.length, 1);
  assert.equal(report.specFunctions[0].name, 'total_supply');
  assert.deepEqual(report.specFunctions[0].outputs, ['U128']);
});

test('sorobanMeta: decodeSorobanMeta warns when no Soroban sections are present', () => {
  const wasm = makeWasmModule([
    { name: 'name', payload: [0x01, 0x02] },
  ]);

  const report = decodeSorobanMeta(wasm);
  assert.equal(report.errors.length, 0);
  assert.equal(report.warnings.length, 1);
  assert.match(report.warnings[0], /No Soroban metadata sections found/);
});

test('sorobanMeta: decodeSorobanMeta rejects non-WASM input', () => {
  const invalidBytes = Uint8Array.from([1, 2, 3, 4, 5]);
  const report = decodeSorobanMeta(invalidBytes);
  assert.equal(report.errors.length, 1);
  assert.match(report.errors[0], /missing magic number/);
});

test('sorobanMeta: compound types decode correctly (Vec, Option, Map)', () => {
  // Option<U32>
  const optionBytes = Uint8Array.from([
    ...u32be(1000), // Option
    ...u32be(4),    // U32
  ]);
  const optRes = _internal.decodeSpecTypeDef(optionBytes, 0);
  assert.equal(optRes.typeName, 'Option<U32>');

  // Vec<Address>
  const vecBytes = Uint8Array.from([
    ...u32be(1002), // Vec
    ...u32be(17),   // Address
  ]);
  const vecRes = _internal.decodeSpecTypeDef(vecBytes, 0);
  assert.equal(vecRes.typeName, 'Vec<Address>');

  // Map<Symbol, U64>
  const mapBytes = Uint8Array.from([
    ...u32be(1003), // Map
    ...u32be(16),   // Symbol
    ...u32be(6),    // U64
  ]);
  const mapRes = _internal.decodeSpecTypeDef(mapBytes, 0);
  assert.equal(mapRes.typeName, 'Map<Symbol, U64>');
});
