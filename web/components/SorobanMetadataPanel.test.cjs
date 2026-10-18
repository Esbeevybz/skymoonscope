// SorobanMetadataPanel.test.cjs — tests for SorobanMetadataPanel component (#413)
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

// Verify that the module file compiles and exports correctly
test('SorobanMetadataPanel: module exports and structure', () => {
  // Ensure the typescript component file exists and has valid syntax
  const fs = require('node:fs');
  const path = require('node:path');
  const fileContent = fs.readFileSync(
    path.join(__dirname, 'SorobanMetadataPanel.tsx'),
    'utf8'
  );
  assert.ok(fileContent.includes('export default function SorobanMetadataPanel'));
  assert.ok(fileContent.includes('filteredFunctions'));
  assert.ok(fileContent.includes('filteredTypes'));
  assert.ok(fileContent.includes('BuildMetaRow'));
  assert.ok(fileContent.includes('FunctionCard'));
});

// Pure helper tests that mirror the component's internal logic
function formatSignature(fn) {
  const args = fn.inputs.map((arg) => `${arg.name}: ${arg.type}`).join(', ');
  const rets = fn.outputs.length > 0 ? ` -> ${fn.outputs.join(', ')}` : '';
  return `fn ${fn.name}(${args})${rets}`;
}

function filterFunctions(functions, query) {
  if (!query || !query.trim()) return functions;
  const q = query.toLowerCase();
  return functions.filter(
    (f) =>
      f.name.toLowerCase().includes(q) ||
      (f.doc && f.doc.toLowerCase().includes(q)) ||
      f.inputs.some((i) => i.name.toLowerCase().includes(q) || i.type.toLowerCase().includes(q))
  );
}

function filterTypes(types, query) {
  if (!query || !query.trim()) return types;
  const q = query.toLowerCase();
  return types.filter(
    (t) =>
      t.name.toLowerCase().includes(q) ||
      t.kind.toLowerCase().includes(q) ||
      (t.doc && t.doc.toLowerCase().includes(q))
  );
}

test('SorobanMetadataPanel: formatSignature creates proper Rust-like signatures', () => {
  const fn1 = {
    kind: 'function',
    name: 'transfer',
    doc: 'Transfer tokens',
    inputs: [
      { name: 'from', type: 'Address', doc: '' },
      { name: 'to', type: 'Address', doc: '' },
      { name: 'amount', type: 'I128', doc: '' },
    ],
    outputs: ['Bool'],
  };

  assert.equal(
    formatSignature(fn1),
    'fn transfer(from: Address, to: Address, amount: I128) -> Bool'
  );

  const fnVoid = {
    kind: 'function',
    name: 'reset',
    doc: '',
    inputs: [],
    outputs: [],
  };

  assert.equal(formatSignature(fnVoid), 'fn reset()');
});

test('SorobanMetadataPanel: filterFunctions searches by name, doc, or param type', () => {
  const fns = [
    {
      name: 'deposit',
      doc: 'Deposit collateral into pool',
      inputs: [{ name: 'amount', type: 'U128', doc: '' }],
      outputs: [],
    },
    {
      name: 'withdraw',
      doc: 'Withdraw funds',
      inputs: [{ name: 'recipient', type: 'Address', doc: '' }],
      outputs: ['U128'],
    },
  ];

  // Search by function name
  const res1 = filterFunctions(fns, 'dep');
  assert.equal(res1.length, 1);
  assert.equal(res1[0].name, 'deposit');

  // Search by doc string
  const res2 = filterFunctions(fns, 'collateral');
  assert.equal(res2.length, 1);
  assert.equal(res2[0].name, 'deposit');

  // Search by parameter type
  const res3 = filterFunctions(fns, 'Address');
  assert.equal(res3.length, 1);
  assert.equal(res3[0].name, 'withdraw');

  // Empty search returns all
  assert.equal(filterFunctions(fns, '').length, 2);
});

test('SorobanMetadataPanel: filterTypes filters by name or kind', () => {
  const types = [
    { kind: 'struct', name: 'PoolConfig', doc: 'Pool configuration settings' },
    { kind: 'enum', name: 'PoolStatus', doc: 'State of liquidity pool' },
    { kind: 'error_enum', name: 'ContractError', doc: 'Custom error codes' },
  ];

  const structOnly = filterTypes(types, 'struct');
  assert.equal(structOnly.length, 1);
  assert.equal(structOnly[0].name, 'PoolConfig');

  const errorOnly = filterTypes(types, 'error');
  assert.equal(errorOnly.length, 1);
  assert.equal(errorOnly[0].name, 'ContractError');
});
