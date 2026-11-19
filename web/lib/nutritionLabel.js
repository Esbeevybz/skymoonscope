// Pure calculation helpers for the contract nutrition label.
//
// The component renders a nutritional-facts style panel, so the numbers it
// shows are "percent daily values" measured against the Soroban per-transaction
// resource limits. Keeping the maths here (rather than inline in the component)
// makes it directly unit-testable and keeps the JSX declarative.

/**
 * Soroban per-transaction resource ceilings.
 *
 * These mirror the network protocol limits the backend validates against.
 * They are exported so callers can override individual entries when a
 * deployment runs with non-default limits.
 */
const SOROBAN_LIMITS = Object.freeze({
  cpu_instructions: 100_000_000,
  ram_bytes: 40 * 1024 * 1024,
  ledger_read_bytes: 64 * 1024,
  ledger_write_bytes: 64 * 1024,
  transaction_size_bytes: 128 * 1024,
});

/**
 * Ordered metric descriptors. Order drives the render order of the label.
 * `key` matches the field names used by the `/analyze` response.
 */
const METRIC_DEFINITIONS = Object.freeze([
  { key: 'cpu_instructions', label: 'CPU Instructions', unit: 'instr', bold: true },
  { key: 'ram_bytes', label: 'Memory (RAM)', unit: 'bytes', bold: true },
  { key: 'ledger_read_bytes', label: 'Ledger Reads', unit: 'bytes', bold: false },
  { key: 'ledger_write_bytes', label: 'Ledger Writes', unit: 'bytes', bold: false },
  { key: 'transaction_size_bytes', label: 'Transaction Size', unit: 'bytes', bold: false },
]);

/** Coerce anything to a finite, non-negative number (defaults to 0). */
function toSafeNumber(value) {
  const num = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(num) || num < 0) return 0;
  return num;
}

/**
 * Percent of the per-transaction limit consumed by `value`.
 *
 * Returns an unclamped percentage so callers can distinguish "at the limit"
 * (100) from "over budget" (>100); use `clampPercent` for bar widths.
 *
 * @param {number} value
 * @param {number} limit
 * @returns {number} percentage, or 0 when the limit is not a positive number
 */
function dailyValuePercent(value, limit) {
  const safeLimit = toSafeNumber(limit);
  if (safeLimit === 0) return 0;
  return (toSafeNumber(value) / safeLimit) * 100;
}

/** Clamp a percentage into the 0-100 range for progress-bar widths. */
function clampPercent(percent) {
  const num = toSafeNumber(percent);
  return num > 100 ? 100 : num;
}

/**
 * Severity bucket for a percent-of-limit reading.
 *
 * Thresholds follow the usual traffic-light convention so the label reads at
 * a glance: comfortable under half the budget, worth noticing past 75%, and
 * critical once the transaction is at or beyond the protocol ceiling.
 */
function severityFor(percent) {
  const num = toSafeNumber(percent);
  if (num >= 100) return 'over';
  if (num >= 75) return 'high';
  if (num >= 50) return 'moderate';
  return 'low';
}

/**
 * Build the full set of rendered metric rows.
 *
 * @param {Record<string, number>} resources per-transaction resource usage
 * @param {Record<string, number>} [limits] optional limit overrides
 */
function buildMetrics(resources, limits) {
  const effectiveLimits = { ...SOROBAN_LIMITS, ...(limits || {}) };
  const source = resources || {};

  return METRIC_DEFINITIONS.map((definition) => {
    const value = toSafeNumber(source[definition.key]);
    const limit = toSafeNumber(effectiveLimits[definition.key]);
    const percent = dailyValuePercent(value, limit);

    return {
      key: definition.key,
      label: definition.label,
      unit: definition.unit,
      bold: definition.bold,
      value,
      limit,
      percent,
      barPercent: clampPercent(percent),
      severity: severityFor(percent),
    };
  });
}

/**
 * Aggregate "calorie" figure for the label header.
 *
 * The single number a reader takes away is the worst-case resource pressure,
 * because a transaction fails when any one budget is exhausted -- not when
 * the average is high.
 */
function overallPercent(metrics) {
  if (!Array.isArray(metrics) || metrics.length === 0) return 0;
  return metrics.reduce((max, metric) => Math.max(max, toSafeNumber(metric.percent)), 0);
}

/** Format a raw resource count for display, using compact notation. */
function formatValue(value) {
  return new Intl.NumberFormat('en-US', {
    notation: 'compact',
    compactDisplay: 'short',
    maximumFractionDigits: 1,
  }).format(toSafeNumber(value));
}

/** Format a percentage for display, keeping sub-1% readings meaningful. */
function formatPercent(percent) {
  const num = toSafeNumber(percent);
  if (num > 0 && num < 0.1) return '<0.1%';
  return `${num.toFixed(1)}%`;
}

/**
 * Sanitise a raw resource object received from the backend.
 *
 * On repeated simulations the backend may compute a memory-RSS *delta*
 * between two calls. If the process freed memory between runs the delta is
 * negative, which would show a misleading negative bar in the label.
 * `toSafeNumber` already clamps individual values to ≥ 0, but exposing this
 * helper makes the intent explicit and lets callers sanitise the entire
 * object in one shot before passing it anywhere else.
 *
 * @param {Partial<import('./nutritionLabel.d').NutritionResources>} resources
 * @returns {import('./nutritionLabel.d').NutritionResources}
 */
function sanitizeResources(resources) {
  const src = resources || {};
  return {
    cpu_instructions: toSafeNumber(src.cpu_instructions),
    ram_bytes: toSafeNumber(src.ram_bytes),
    ledger_read_bytes: toSafeNumber(src.ledger_read_bytes),
    ledger_write_bytes: toSafeNumber(src.ledger_write_bytes),
    transaction_size_bytes: toSafeNumber(src.transaction_size_bytes),
  };
}

/**
 * Export the nutrition label as a PDF and trigger a browser download.
 *
 * Lazy-loads `./pdfExport` so the PDF generation code stays out of the
 * initial bundle for consumers that only use the calculation helpers.
 *
 * @param {import('./nutritionLabel.d').NutritionMetric[]} metrics
 * @param {{ functionName?: string, costStroops?: number }} [options]
 * @returns {Promise<void>}
 */
async function exportPdf(metrics, options) {
  const { buildPdfBytes, downloadPdf } = require('./pdfExport');
  const bytes = buildPdfBytes(metrics, options);
  const filename = `sky-moon-scope-nutrition-${options?.functionName || 'transaction'}-${Date.now()}.pdf`;
  downloadPdf(bytes, filename);
}

module.exports = {
  SOROBAN_LIMITS,
  METRIC_DEFINITIONS,
  dailyValuePercent,
  clampPercent,
  severityFor,
  buildMetrics,
  overallPercent,
  formatValue,
  formatPercent,
  sanitizeResources,
  exportPdf,
};
