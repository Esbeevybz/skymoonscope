// heatmap.test.cjs — unit tests for extracted heatmap components & logic
// Runs with: node --test ./components/heatmap.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

// Test data limits matching Soroban budgets
const LIMITS = {
  CPU: 100_000_000,
  RAM: 40 * 1024 * 1024,
  LEDGER_READ: 150 * 1024,
  LEDGER_WRITE: 100 * 1024,
  TX_SIZE: 70 * 1024,
};

function hotspotColors(share) {
  if (share >= 20) return { label: 'CRITICAL', barHex: '#f43f5e' };
  if (share >= 10) return { label: 'HIGH', barHex: '#f97316' };
  if (share >= 5)  return { label: 'MEDIUM', barHex: '#eab308' };
  if (share >= 2)  return { label: 'LOW', barHex: '#06b6d4' };
  return { label: 'TRACE', barHex: '#475569' };
}

function statusColor(pct) {
  if (pct > 80) return { ring: '#f43f5e' };
  if (pct > 50) return { ring: '#eab308' };
  return { ring: '#06b6d4' };
}

function fmtInstr(n) {
  return new Intl.NumberFormat('en-US', { notation: 'compact', compactDisplay: 'short' }).format(n);
}

function createZoomState(initial = 1, min = 0.5, max = 2, step = 0.1) {
  let zoom = initial;
  return {
    get zoom() { return zoom; },
    zoomIn() {
      zoom = Math.min(max, Math.round((zoom + step) * 100) / 100);
      return zoom;
    },
    zoomOut() {
      zoom = Math.max(min, Math.round((zoom - step) * 100) / 100);
      return zoom;
    },
    reset() {
      zoom = 1;
      return zoom;
    },
  };
}

// ── Tests ───────────────────────────────────────────────────────────────────

test('HeatmapColorScale: assigns correct severity threshold labels', () => {
  assert.equal(hotspotColors(25).label, 'CRITICAL');
  assert.equal(hotspotColors(20).label, 'CRITICAL');
  assert.equal(hotspotColors(15).label, 'HIGH');
  assert.equal(hotspotColors(10).label, 'HIGH');
  assert.equal(hotspotColors(7).label, 'MEDIUM');
  assert.equal(hotspotColors(5).label, 'MEDIUM');
  assert.equal(hotspotColors(3).label, 'LOW');
  assert.equal(hotspotColors(2).label, 'LOW');
  assert.equal(hotspotColors(1).label, 'TRACE');
});

test('HeatmapColorScale: statusColor accurately categorizes percentages', () => {
  assert.equal(statusColor(85).ring, '#f43f5e');
  assert.equal(statusColor(60).ring, '#eab308');
  assert.equal(statusColor(30).ring, '#06b6d4');
});

test('HeatmapZoomControls: zoomIn, zoomOut, and reset adjust levels within bounds', () => {
  const controls = createZoomState(1, 0.5, 1.5, 0.1);
  assert.equal(controls.zoom, 1);

  controls.zoomIn();
  assert.equal(controls.zoom, 1.1);

  controls.zoomIn();
  assert.equal(controls.zoom, 1.2);

  controls.zoomOut();
  assert.equal(controls.zoom, 1.1);

  controls.reset();
  assert.equal(controls.zoom, 1);
});

test('HeatmapZoomControls: clamps to min and max boundaries', () => {
  const controls = createZoomState(1.4, 0.5, 1.5, 0.1);

  controls.zoomIn();
  assert.equal(controls.zoom, 1.5);

  // Exceeds max, should clamp at 1.5
  controls.zoomIn();
  assert.equal(controls.zoom, 1.5);

  // Zoom all the way down
  for (let i = 0; i < 20; i++) {
    controls.zoomOut();
  }
  assert.equal(controls.zoom, 0.5);
});

test('HeatmapTooltip: instruction formatting works correctly', () => {
  assert.equal(fmtInstr(1000), '1K');
  assert.equal(fmtInstr(50000000), '50M');
});

// ── HeatmapFloatingTooltip logic tests ──────────────────────────────────────

/**
 * Mirror of the position-adjustment logic in HeatmapFloatingTooltip.tsx.
 * Tests the viewport-aware clamping behaviour without needing a DOM renderer.
 */
const TOOLTIP_WIDTH = 224;
const TOOLTIP_HEIGHT = 260;
const CURSOR_OFFSET_X = 14;
const CURSOR_OFFSET_Y = 14;

function adjustTooltipPosition(mouseX, mouseY, viewportWidth, viewportHeight) {
  let x = mouseX + CURSOR_OFFSET_X;
  let y = mouseY + CURSOR_OFFSET_Y;

  if (x + TOOLTIP_WIDTH > viewportWidth - 8) {
    x = mouseX - TOOLTIP_WIDTH - CURSOR_OFFSET_X;
  }
  if (y + TOOLTIP_HEIGHT > viewportHeight - 8) {
    y = mouseY - TOOLTIP_HEIGHT - CURSOR_OFFSET_Y;
  }

  x = Math.max(8, x);
  y = Math.max(8, y);

  return { x, y };
}

/**
 * Mirror of the exact-value formatters used in HeatmapFloatingTooltip.
 */
function fmtInstrFull(n) {
  return new Intl.NumberFormat('en-US').format(n);
}

function budgetPct(instructions, cpuLimit) {
  return Math.min((instructions / cpuLimit) * 100, 100);
}

// ── Sample cell data ─────────────────────────────────────────────────────────

const SAMPLE_CELL = {
  id: 'ft-0',
  fnName: 'auth::verify_signature',
  displayName: 'auth::verify_sig',
  category: 'auth',
  cpuShare: 24.5,
  cpuInstructions: 24_500_000,
  depth: 0,
};

const SAMPLE_CELL_DEEP = {
  id: 'ft-1',
  fnName: 'math::u128_arithmetic',
  displayName: 'math::u128_ops',
  category: 'compute',
  cpuShare: 3.2,
  cpuInstructions: 3_200_000,
  depth: 2,
};

// ── Tests ────────────────────────────────────────────────────────────────────

const { describe } = require('node:test');

describe('HeatmapFloatingTooltip – position adjustment', () => {
  test('tooltip appears to bottom-right of cursor by default', () => {
    const pos = adjustTooltipPosition(400, 300, 1280, 800);
    assert.equal(pos.x, 400 + CURSOR_OFFSET_X);
    assert.equal(pos.y, 300 + CURSOR_OFFSET_Y);
  });

  test('flips horizontally when near the right viewport edge', () => {
    // cursor at x=1200 in 1280px wide viewport
    const pos = adjustTooltipPosition(1200, 300, 1280, 800);
    // should flip: x = mouseX - TOOLTIP_WIDTH - CURSOR_OFFSET_X
    assert.equal(pos.x, 1200 - TOOLTIP_WIDTH - CURSOR_OFFSET_X);
  });

  test('flips vertically when near the bottom viewport edge', () => {
    // cursor at y=700 in 800px tall viewport
    const pos = adjustTooltipPosition(400, 700, 1280, 800);
    // should flip: y = mouseY - TOOLTIP_HEIGHT - CURSOR_OFFSET_Y
    assert.equal(pos.y, 700 - TOOLTIP_HEIGHT - CURSOR_OFFSET_Y);
  });

  test('flips both axes when near bottom-right corner', () => {
    const pos = adjustTooltipPosition(1200, 700, 1280, 800);
    assert.equal(pos.x, 1200 - TOOLTIP_WIDTH - CURSOR_OFFSET_X);
    assert.equal(pos.y, 700 - TOOLTIP_HEIGHT - CURSOR_OFFSET_Y);
  });

  test('clamps x to minimum 8px from left edge', () => {
    // cursor so far left that flipping would go negative
    const pos = adjustTooltipPosition(0, 300, 1280, 800);
    // default path: x = 0 + 14 = 14 (no flip needed), y fine
    assert.ok(pos.x >= 8);
  });

  test('clamps y to minimum 8px from top edge', () => {
    const pos = adjustTooltipPosition(400, 0, 1280, 800);
    assert.ok(pos.y >= 8);
  });

  test('works correctly at a central cursor position', () => {
    const pos = adjustTooltipPosition(640, 400, 1280, 800);
    assert.equal(pos.x, 640 + CURSOR_OFFSET_X);
    assert.equal(pos.y, 400 + CURSOR_OFFSET_Y);
  });
});

describe('HeatmapFloatingTooltip – exact value formatting', () => {
  test('fmtInstrFull formats integers with comma separators', () => {
    assert.equal(fmtInstrFull(1000), '1,000');
    assert.equal(fmtInstrFull(1_000_000), '1,000,000');
    assert.equal(fmtInstrFull(24_500_000), '24,500,000');
    assert.equal(fmtInstrFull(100_000_000), '100,000,000');
  });

  test('fmtInstrFull handles zero correctly', () => {
    assert.equal(fmtInstrFull(0), '0');
  });

  test('budgetPct computes correct percentage of 100M limit', () => {
    assert.equal(budgetPct(50_000_000, LIMITS.CPU), 50);
    assert.equal(budgetPct(100_000_000, LIMITS.CPU), 100);
    assert.equal(budgetPct(24_500_000, LIMITS.CPU), 24.5);
  });

  test('budgetPct clamps to 100 when instructions exceed limit', () => {
    assert.equal(budgetPct(200_000_000, LIMITS.CPU), 100);
  });

  test('cpuShare rounds to 2 decimal places for display', () => {
    const share = SAMPLE_CELL.cpuShare;
    assert.equal(share.toFixed(2), '24.50');
  });

  test('depth field is exposed correctly on cell data', () => {
    assert.equal(SAMPLE_CELL.depth, 0);
    assert.equal(SAMPLE_CELL_DEEP.depth, 2);
  });
});

describe('HeatmapFloatingTooltip – cell data integrity', () => {
  test('fnName and displayName are distinct when truncated', () => {
    assert.notEqual(SAMPLE_CELL.fnName, SAMPLE_CELL.displayName);
  });

  test('cpuInstructions is consistent with cpuShare at 100M budget', () => {
    const expectedInstr = Math.round((SAMPLE_CELL.cpuShare / 100) * LIMITS.CPU);
    assert.equal(SAMPLE_CELL.cpuInstructions, expectedInstr);
  });

  test('severity label matches expected band for cpuShare', () => {
    const clr = hotspotColors(SAMPLE_CELL.cpuShare);   // 24.5 → CRITICAL
    assert.equal(clr.label, 'CRITICAL');

    const clr2 = hotspotColors(SAMPLE_CELL_DEEP.cpuShare); // 3.2 → LOW
    assert.equal(clr2.label, 'LOW');
  });

  test('cell with zero instructions still produces valid budget pct', () => {
    const pct = budgetPct(0, LIMITS.CPU);
    assert.equal(pct, 0);
  });

  test('all category values are valid FnCategory strings', () => {
    const validCategories = new Set(['auth', 'storage', 'compute', 'io', 'util']);
    assert.ok(validCategories.has(SAMPLE_CELL.category));
    assert.ok(validCategories.has(SAMPLE_CELL_DEEP.category));
  });
});
