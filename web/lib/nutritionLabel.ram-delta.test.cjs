// nutritionLabel.ram-delta.test.cjs — tests for the RAM negative-delta fix
// and PDF export utility.
//
// Addresses:
//   Issue: Fix RAM metric showing negative delta on repeated simulations
//   Issue: Implement PDF export of Contract Nutrition Label report
//
// Runs with: node --test ./lib/nutritionLabel.ram-delta.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  sanitizeResources,
  buildMetrics,
  exportPdf,
} = require('./nutritionLabel.js');

const { buildPdfBytes, buildContentStream, formatCompact } = require('./pdfExport.js');

// ─── sanitizeResources (RAM negative-delta fix) ───────────────────────────────

test('sanitizeResources clamps negative ram_bytes to 0', () => {
  const result = sanitizeResources({ ram_bytes: -1024 });
  assert.equal(result.ram_bytes, 0, 'negative ram_bytes must be clamped to 0');
});

test('sanitizeResources clamps all negative resource fields to 0', () => {
  const result = sanitizeResources({
    cpu_instructions: -500_000,
    ram_bytes: -2048,
    ledger_read_bytes: -100,
    ledger_write_bytes: -50,
    transaction_size_bytes: -10,
  });

  assert.equal(result.cpu_instructions, 0);
  assert.equal(result.ram_bytes, 0);
  assert.equal(result.ledger_read_bytes, 0);
  assert.equal(result.ledger_write_bytes, 0);
  assert.equal(result.transaction_size_bytes, 0);
});

test('sanitizeResources preserves valid positive values unchanged', () => {
  const input = {
    cpu_instructions: 5_000_000,
    ram_bytes: 1_024_000,
    ledger_read_bytes: 4_096,
    ledger_write_bytes: 2_048,
    transaction_size_bytes: 512,
  };
  const result = sanitizeResources(input);
  assert.deepEqual(result, input);
});

test('sanitizeResources coerces NaN to 0', () => {
  const result = sanitizeResources({ ram_bytes: NaN });
  assert.equal(result.ram_bytes, 0);
});

test('sanitizeResources coerces Infinity to 0', () => {
  const result = sanitizeResources({ ram_bytes: Infinity });
  assert.equal(result.ram_bytes, 0);
});

test('sanitizeResources handles null/undefined gracefully', () => {
  for (const input of [null, undefined, {}]) {
    const result = sanitizeResources(input);
    assert.equal(result.ram_bytes, 0);
    assert.equal(result.cpu_instructions, 0);
    assert.equal(result.ledger_read_bytes, 0);
    assert.equal(result.ledger_write_bytes, 0);
    assert.equal(result.transaction_size_bytes, 0);
  }
});

test('sanitizeResources output passes straight into buildMetrics without negative bars', () => {
  // Simulates the case where the backend returns a negative RAM delta
  const backendPayload = {
    cpu_instructions: 12_000_000,
    ram_bytes: -512,           // negative delta from backend
    ledger_read_bytes: 2_048,
    ledger_write_bytes: 0,
    transaction_size_bytes: 300,
  };

  const metrics = buildMetrics(sanitizeResources(backendPayload));

  for (const metric of metrics) {
    assert.ok(metric.value >= 0, `value for ${metric.key} must be >= 0`);
    assert.ok(metric.percent >= 0, `percent for ${metric.key} must be >= 0`);
    assert.ok(metric.barPercent >= 0, `barPercent for ${metric.key} must be >= 0`);
  }

  const ram = metrics.find((m) => m.key === 'ram_bytes');
  assert.equal(ram.value, 0, 'sanitised negative RAM must show as 0');
  assert.equal(ram.percent, 0, 'sanitised negative RAM must show 0% usage');
});

// ─── pdfExport ────────────────────────────────────────────────────────────────

// Build a minimal set of metrics to use in PDF tests
function sampleMetrics() {
  return buildMetrics({
    cpu_instructions: 50_000_000,
    ram_bytes: 20 * 1024 * 1024,
    ledger_read_bytes: 32 * 1024,
    ledger_write_bytes: 16 * 1024,
    transaction_size_bytes: 64 * 1024,
  });
}

test('buildPdfBytes returns a Uint8Array', () => {
  const bytes = buildPdfBytes(sampleMetrics());
  assert.ok(bytes instanceof Uint8Array, 'should return a Uint8Array');
  assert.ok(bytes.length > 0, 'PDF must be non-empty');
});

test('buildPdfBytes output starts with the PDF magic header %%PDF-1.4', () => {
  const bytes = buildPdfBytes(sampleMetrics());
  const header = new TextDecoder().decode(bytes.slice(0, 8));
  assert.equal(header, '%PDF-1.4', 'PDF must start with the version header');
});

test('buildPdfBytes output ends with %%EOF marker', () => {
  const bytes = buildPdfBytes(sampleMetrics());
  const tail = new TextDecoder().decode(bytes.slice(-10));
  assert.ok(tail.includes('%%EOF'), 'PDF must end with the %%EOF marker');
});

test('buildPdfBytes embeds the function name in the content stream', () => {
  const bytes = buildPdfBytes(sampleMetrics(), { functionName: 'my_transfer_fn' });
  const text = new TextDecoder().decode(bytes);
  assert.ok(text.includes('my_transfer_fn'), 'function name should appear in the PDF content');
});

test('buildPdfBytes embeds cost_stroops when provided', () => {
  const bytes = buildPdfBytes(sampleMetrics(), { costStroops: 12345678 });
  const text = new TextDecoder().decode(bytes);
  // 12345678 stroops = 1.2345678 XLM
  assert.ok(text.includes('1.2345678'), 'XLM fee should appear in the PDF content');
});

test('buildPdfBytes skips gas breakdown when cost_stroops is absent', () => {
  const stream = buildContentStream(sampleMetrics(), {});
  assert.ok(!stream.includes('XLM'), 'no XLM line when costStroops is not provided');
});

test('buildPdfBytes contains one object per metric label', () => {
  const metrics = sampleMetrics();
  const text = new TextDecoder().decode(buildPdfBytes(metrics));
  for (const metric of metrics) {
    // PDF literal strings escape parentheses, e.g. "Memory (RAM)" → "Memory \(RAM\)".
    // Check for the first unambiguous word in each label instead of the full string.
    const keyword = metric.label.split(' ')[0]; // e.g. "Memory", "CPU", "Ledger", "Transaction"
    assert.ok(
      text.includes(keyword),
      `keyword "${keyword}" from metric label "${metric.label}" must appear in the PDF`,
    );
  }
});

test('buildPdfBytes throws when metrics is empty or not an array', () => {
  assert.throws(() => buildPdfBytes([]), /non-empty array/);
  assert.throws(() => buildPdfBytes(null), /non-empty array/);
  assert.throws(() => buildPdfBytes(undefined), /non-empty array/);
});

test('buildPdfBytes contains required PDF cross-reference objects', () => {
  const text = new TextDecoder().decode(buildPdfBytes(sampleMetrics()));
  assert.ok(text.includes('xref'), 'PDF must have a cross-reference table');
  assert.ok(text.includes('startxref'), 'PDF must have a startxref entry');
  assert.ok(text.includes('/Catalog'), 'PDF must declare a Catalog object');
  assert.ok(text.includes('/Pages'), 'PDF must declare a Pages object');
  assert.ok(text.includes('/Helvetica'), 'PDF must reference the Helvetica font');
});

test('buildContentStream includes Nutrition Facts title', () => {
  const stream = buildContentStream(sampleMetrics(), { functionName: 'transfer' });
  assert.ok(stream.includes('Nutrition Facts'), 'stream must contain the label title');
});

test('buildContentStream includes all metric labels', () => {
  const metrics = sampleMetrics();
  const stream = buildContentStream(metrics, {});
  for (const metric of metrics) {
    // PDF literal strings escape parentheses; check the first word of the label.
    const keyword = metric.label.split(' ')[0];
    assert.ok(stream.includes(keyword), `"${keyword}" from "${metric.label}" must appear in the content stream`);
  }
});

// ─── formatCompact ────────────────────────────────────────────────────────────

test('formatCompact converts billions', () => {
  assert.equal(formatCompact(1_500_000_000), '1.5B');
});

test('formatCompact converts millions', () => {
  assert.equal(formatCompact(50_000_000), '50.0M');
});

test('formatCompact converts thousands', () => {
  assert.equal(formatCompact(1_500), '1.5K');
});

test('formatCompact handles zero', () => {
  assert.equal(formatCompact(0), '0');
});

test('formatCompact handles negative as 0', () => {
  assert.equal(formatCompact(-100), '0');
});

test('formatCompact handles NaN as 0', () => {
  assert.equal(formatCompact(NaN), '0');
});
