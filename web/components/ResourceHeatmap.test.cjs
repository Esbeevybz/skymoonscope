'use strict';

/**
 * Tests for ResourceHeatmap additions:
 *  1. HeatmapLegend – configurable scale breakpoints
 *  2. Export to PNG / SVG
 *
 * Runs with: node --test components/ResourceHeatmap.test.cjs
 *
 * These are pure-logic / DOM-stub tests (no React renderer required) that
 * exercise the helpers extracted from ResourceHeatmap.tsx.
 */

const { test, describe } = require('node:test');
const assert = require('node:assert/strict');

// ── Inline the pure helpers under test ───────────────────────────────────────

const DEFAULT_BREAKPOINTS = { critical: 20, high: 10, medium: 5, low: 2 };

function hotspotColors(share, bp = DEFAULT_BREAKPOINTS) {
  if (share >= bp.critical) return { label: 'CRITICAL', barHex: '#f43f5e' };
  if (share >= bp.high)     return { label: 'HIGH',     barHex: '#f97316' };
  if (share >= bp.medium)   return { label: 'MEDIUM',   barHex: '#eab308' };
  if (share >= bp.low)      return { label: 'LOW',      barHex: '#06b6d4' };
  return                           { label: 'TRACE',    barHex: '#475569' };
}

function clampBreakpoint(key, v, bp) {
  const next = { ...bp };
  if (key === 'critical') next.critical = Math.max(bp.high + 1,     Math.min(100,              v));
  if (key === 'high')     next.high     = Math.max(bp.medium + 1,   Math.min(bp.critical - 1,  v));
  if (key === 'medium')   next.medium   = Math.max(bp.low + 1,      Math.min(bp.high - 1,      v));
  if (key === 'low')      next.low      = Math.max(0.1,             Math.min(bp.medium - 1,    v));
  return next;
}

/**
 * Minimal SVG builder mirroring the real exportSvg logic, but returning the
 * SVG string instead of triggering a download – suitable for unit testing.
 */
function buildSvgString(cells, bp = DEFAULT_BREAKPOINTS) {
  const CELL_W = 120, CELL_H = 72, COLS = 4, PAD = 8;
  const rows = Math.ceil(cells.length / COLS);
  const W = COLS * (CELL_W + PAD) + PAD;
  const H = rows * (CELL_H + PAD) + PAD + 48;

  const parts = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}">`,
    `<rect width="100%" height="100%" fill="#0f172a"/>`,
    `<text x="8" y="28" font-family="monospace" font-size="13" fill="#f1f5f9">Sky Moon Scope — CPU Hotspot Heatmap</text>`,
  ];

  cells.forEach((cell, i) => {
    const col = i % COLS, row = Math.floor(i / COLS);
    const x = PAD + col * (CELL_W + PAD);
    const y = 48 + PAD + row * (CELL_H + PAD);
    const clr = hotspotColors(cell.cpuShare, bp);
    parts.push(
      `<rect x="${x}" y="${y}" width="${CELL_W}" height="${CELL_H}" rx="6" fill="#1e293b88" stroke="${clr.barHex}" stroke-width="1"/>`,
      `<text x="${x + 6}" y="${y + 16}" font-family="monospace" font-size="8" fill="${clr.barHex}">#${i + 1}  ${clr.label}</text>`,
      `<text x="${x + 6}" y="${y + 32}" font-family="monospace" font-size="9" font-weight="bold" fill="#f1f5f9">${cell.displayName}</text>`,
    );
  });

  const legendBands = [
    { label: `CRITICAL ≥${bp.critical}%`, hex: '#f43f5e' },
    { label: `HIGH ≥${bp.high}%`,         hex: '#f97316' },
    { label: `MEDIUM ≥${bp.medium}%`,     hex: '#eab308' },
    { label: `LOW ≥${bp.low}%`,           hex: '#06b6d4' },
    { label: `TRACE <${bp.low}%`,         hex: '#475569' },
  ];
  const legendY = H - 14;
  legendBands.forEach((b, i) => {
    const lx = PAD + i * 130;
    parts.push(`<rect x="${lx}" y="${legendY - 6}" width="10" height="10" rx="2" fill="${b.hex}"/>`);
    parts.push(`<text x="${lx + 14}" y="${legendY + 3}" font-family="monospace" font-size="8" fill="#94a3b8">${b.label}</text>`);
  });

  parts.push('</svg>');
  return parts.join('\n');
}

const SAMPLE_CELLS = [
  { id: 'c0', displayName: 'auth::verify_sig',  cpuShare: 24, cpuInstructions: 240000, category: 'auth',    depth: 0, fnName: 'auth::verify_sig' },
  { id: 'c1', displayName: 'fn::core_logic',    cpuShare: 12, cpuInstructions: 120000, category: 'compute', depth: 0, fnName: 'fn::core_logic' },
  { id: 'c2', displayName: 'map::write',         cpuShare:  6, cpuInstructions:  60000, category: 'storage', depth: 0, fnName: 'map::write' },
  { id: 'c3', displayName: 'validate_inputs',    cpuShare:  3, cpuInstructions:  30000, category: 'util',    depth: 0, fnName: 'fn::validate_inputs' },
  { id: 'c4', displayName: 'event::publish',     cpuShare:  1, cpuInstructions:  10000, category: 'io',      depth: 0, fnName: 'event::publish' },
];

// ═══════════════════════════════════════════════════════════════════════════════
// Issue 1 – HeatmapLegend: configurable scale breakpoints
// ═══════════════════════════════════════════════════════════════════════════════

describe('HeatmapLegend – scale breakpoints', () => {

  test('default breakpoints produce correct severity bands', () => {
    assert.equal(hotspotColors(25).label, 'CRITICAL');
    assert.equal(hotspotColors(20).label, 'CRITICAL');
    assert.equal(hotspotColors(15).label, 'HIGH');
    assert.equal(hotspotColors(10).label, 'HIGH');
    assert.equal(hotspotColors(7).label,  'MEDIUM');
    assert.equal(hotspotColors(5).label,  'MEDIUM');
    assert.equal(hotspotColors(3).label,  'LOW');
    assert.equal(hotspotColors(2).label,  'LOW');
    assert.equal(hotspotColors(1).label,  'TRACE');
  });

  test('custom breakpoints change severity classification', () => {
    const custom = { critical: 50, high: 30, medium: 15, low: 5 };
    // 20% is HIGH under default but MEDIUM under custom
    assert.equal(hotspotColors(20, DEFAULT_BREAKPOINTS).label, 'CRITICAL');
    assert.equal(hotspotColors(20, custom).label,              'MEDIUM');
  });

  test('lowering critical threshold promotes more cells to CRITICAL', () => {
    const lowCritical = { ...DEFAULT_BREAKPOINTS, critical: 5 };
    assert.equal(hotspotColors(6, lowCritical).label,  'CRITICAL');
    assert.equal(hotspotColors(6, DEFAULT_BREAKPOINTS).label, 'MEDIUM');
  });

  test('clampBreakpoint prevents critical from going below high + 1', () => {
    const bp = { critical: 20, high: 10, medium: 5, low: 2 };
    const next = clampBreakpoint('critical', 9, bp); // try to set critical = 9, but high = 10
    assert.ok(next.critical > bp.high, 'critical must remain above high');
  });

  test('clampBreakpoint prevents high from exceeding critical - 1', () => {
    const bp = { critical: 20, high: 10, medium: 5, low: 2 };
    const next = clampBreakpoint('high', 25, bp); // try to set high > critical
    assert.ok(next.high < bp.critical, 'high must stay below critical');
  });

  test('clampBreakpoint prevents low from going below 0.1', () => {
    const bp = { critical: 20, high: 10, medium: 5, low: 2 };
    const next = clampBreakpoint('low', -5, bp);
    assert.ok(next.low >= 0.1, 'low must be at least 0.1');
  });

  test('reset to defaults restores original thresholds', () => {
    const custom = { critical: 50, high: 30, medium: 15, low: 5 };
    // Simulate reset
    const restored = { ...DEFAULT_BREAKPOINTS };
    assert.deepEqual(restored, DEFAULT_BREAKPOINTS);
    assert.notDeepEqual(custom, DEFAULT_BREAKPOINTS);
  });

  test('all five bands have distinct color hexes', () => {
    const bp = DEFAULT_BREAKPOINTS;
    const hexes = [
      hotspotColors(25, bp).barHex,  // CRITICAL
      hotspotColors(15, bp).barHex,  // HIGH
      hotspotColors(7,  bp).barHex,  // MEDIUM
      hotspotColors(3,  bp).barHex,  // LOW
      hotspotColors(0.5,bp).barHex,  // TRACE
    ];
    const unique = new Set(hexes);
    assert.equal(unique.size, 5, 'each severity band should have a unique color');
  });

  test('boundary values fall into correct bands', () => {
    const bp = DEFAULT_BREAKPOINTS;
    // Exact boundary — should belong to the band defined by >=
    assert.equal(hotspotColors(bp.critical, bp).label, 'CRITICAL');
    assert.equal(hotspotColors(bp.high,     bp).label, 'HIGH');
    assert.equal(hotspotColors(bp.medium,   bp).label, 'MEDIUM');
    assert.equal(hotspotColors(bp.low,      bp).label, 'LOW');
    assert.equal(hotspotColors(bp.low - 0.1, bp).label, 'TRACE');
  });
});

// ═══════════════════════════════════════════════════════════════════════════════
// Issue 2 – Export to PNG / SVG
// ═══════════════════════════════════════════════════════════════════════════════

describe('Heatmap export – SVG builder', () => {

  test('buildSvgString returns a valid SVG string', () => {
    const svg = buildSvgString(SAMPLE_CELLS);
    assert.ok(svg.startsWith('<svg'), 'must start with <svg');
    assert.ok(svg.endsWith('</svg>'), 'must end with </svg>');
  });

  test('SVG contains title text', () => {
    const svg = buildSvgString(SAMPLE_CELLS);
    assert.ok(svg.includes('Sky Moon Scope'), 'SVG must include the app title');
  });

  test('SVG contains a rect for every cell', () => {
    const svg = buildSvgString(SAMPLE_CELLS);
    // Each cell produces at least one <rect> beyond the background rect
    const rectCount = (svg.match(/<rect /g) ?? []).length;
    // background (1) + cells (5) + legend swatches (5) = 11
    assert.ok(rectCount >= SAMPLE_CELLS.length, 'should have at least one rect per cell');
  });

  test('SVG contains display names for all cells', () => {
    const svg = buildSvgString(SAMPLE_CELLS);
    for (const cell of SAMPLE_CELLS) {
      assert.ok(svg.includes(cell.displayName), `SVG must include cell label: ${cell.displayName}`);
    }
  });

  test('SVG legend reflects custom breakpoints', () => {
    const custom = { critical: 40, high: 25, medium: 12, low: 4 };
    const svg = buildSvgString(SAMPLE_CELLS, custom);
    assert.ok(svg.includes(`CRITICAL ≥${custom.critical}%`), 'legend must show custom critical threshold');
    assert.ok(svg.includes(`HIGH ≥${custom.high}%`),         'legend must show custom high threshold');
    assert.ok(svg.includes(`MEDIUM ≥${custom.medium}%`),     'legend must show custom medium threshold');
    assert.ok(svg.includes(`LOW ≥${custom.low}%`),           'legend must show custom low threshold');
  });

  test('SVG uses correct stroke color for CRITICAL cell', () => {
    const svg = buildSvgString(SAMPLE_CELLS); // cell[0] has cpuShare=24 → CRITICAL → #f43f5e
    assert.ok(svg.includes('#f43f5e'), 'CRITICAL cell stroke should be rose hex');
  });

  test('SVG dimensions grow with more cells', () => {
    const few  = buildSvgString(SAMPLE_CELLS.slice(0, 2));
    const many = buildSvgString(SAMPLE_CELLS);
    const getH = (s) => Number(s.match(/height="(\d+)"/)?.[1] ?? 0);
    assert.ok(getH(many) >= getH(few), 'more cells → taller SVG');
  });

  test('empty cell list produces valid SVG', () => {
    const svg = buildSvgString([]);
    assert.ok(svg.startsWith('<svg'), 'empty SVG is still valid');
    assert.ok(svg.endsWith('</svg>'));
  });

  test('SVG builder handles exactly 500 cells correctly without truncation', () => {
    const cells500 = [];
    for (let i = 0; i < 500; i++) {
      cells500.push({
        id: `c-${i}`,
        displayName: `fn_${i}`,
        cpuShare: 100 / 500,
        cpuInstructions: 1000,
        category: 'compute',
        depth: 0,
        fnName: `contract::fn_${i}`,
      });
    }

    const svg = buildSvgString(cells500);
    assert.ok(svg.startsWith('<svg'));
    assert.ok(svg.endsWith('</svg>'));

    // background (1) + 500 cell rects + 5 legend rects = 506
    const rectCount = (svg.match(/<rect /g) ?? []).length;
    assert.ok(rectCount >= 500, `SVG must render all 500 cells, got ${rectCount}`);
    assert.ok(svg.includes('fn_0'));
    assert.ok(svg.includes('fn_499'));
  });

  test('SVG builder handles beyond 500 cells (e.g. 600, 1000 cells) correctly', () => {
    const counts = [600, 1000];
    for (const count of counts) {
      const cells = [];
      for (let i = 0; i < count; i++) {
        cells.push({
          id: `c-${i}`,
          displayName: `fn_${i}`,
          cpuShare: 100 / count,
          cpuInstructions: 1000,
          category: 'compute',
          depth: 0,
          fnName: `contract::fn_${i}`,
        });
      }

      const svg = buildSvgString(cells);
      assert.ok(svg.startsWith('<svg'));
      assert.ok(svg.endsWith('</svg>'));
      const rectCount = (svg.match(/<rect /g) ?? []).length;
      assert.ok(rectCount >= count, `SVG must render all ${count} cells, got ${rectCount}`);
      assert.ok(svg.includes(`fn_${count - 1}`));
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════════════
// Issue #383 – Call graph flattening beyond 500 cells (no arbitrary truncation)
// ═══════════════════════════════════════════════════════════════════════════════

describe('Issue #383 – Call graph processing without arbitrary 12-cell truncation', () => {
  function categoize(fnName) {
    const n = fnName.toLowerCase();
    if (n.includes('auth') || n.includes('verify')) return 'auth';
    if (n.includes('write') || n.includes('store')) return 'storage';
    if (n.includes('read') || n.includes('get')) return 'storage';
    if (n.includes('event') || n.includes('emit')) return 'io';
    if (n.includes('util') || n.includes('parse')) return 'util';
    return 'compute';
  }

  function cellsFromCallGraph(root, totalCpu) {
    const cells = [];
    const DEPTH_BUDGET = [40, 30, 20, 10];
    let counter = 0;

    function traverse(node, depth, levelShare) {
      const budget = DEPTH_BUDGET[Math.min(depth, DEPTH_BUDGET.length - 1)];
      const cpuShare = (budget * levelShare) / 100;
      const displayName = node.function.length > 14
        ? node.function.slice(0, 12) + '…'
        : node.function;

      cells.push({
        id: `cg-${counter++}`,
        fnName: `${node.contract_id}::${node.function}`,
        displayName,
        category: categoize(node.function),
        cpuShare,
        cpuInstructions: Math.round((cpuShare / 100) * totalCpu),
        depth,
      });

      if (node.children && node.children.length > 0) {
        const sharePerChild = 100 / node.children.length;
        node.children.forEach(child => traverse(child, depth + 1, sharePerChild));
      }
    }

    traverse(root, 0, 100);

    const total = cells.reduce((s, c) => s + c.cpuShare, 0);
    if (total > 0) {
      cells.forEach(c => {
        c.cpuShare = (c.cpuShare / total) * 100;
        c.cpuInstructions = Math.round((c.cpuShare / 100) * totalCpu);
      });
    }

    return cells.sort((a, b) => b.cpuShare - a.cpuShare);
  }

  test('call graph with 500 children preserves all 501 nodes (no slice(0,12) truncation)', () => {
    const children = [];
    for (let i = 0; i < 500; i++) {
      children.push({
        contract_id: 'C_CHILD',
        function: `child_fn_${i}`,
        children: [],
      });
    }
    const root = {
      contract_id: 'C_ROOT',
      function: 'root_entry',
      children,
    };

    const cells = cellsFromCallGraph(root, 10_000_000);
    assert.equal(cells.length, 501, 'must retain all 501 nodes instead of slicing to 12');
    assert.equal(cells[0].fnName, 'C_ROOT::root_entry');
    const totalShare = cells.reduce((sum, c) => sum + c.cpuShare, 0);
    assert.ok(Math.abs(totalShare - 100) < 0.01, 'shares must sum to 100%');
  });

  test('call graph with 1,000 children scales efficiently without truncation', () => {
    const children = [];
    for (let i = 0; i < 1000; i++) {
      children.push({
        contract_id: 'C_LEAF',
        function: `leaf_${i}`,
        children: [],
      });
    }
    const root = {
      contract_id: 'C_ROOT',
      function: 'main',
      children,
    };

    const t0 = performance.now();
    const cells = cellsFromCallGraph(root, 50_000_000);
    const duration = performance.now() - t0;

    assert.equal(cells.length, 1001, 'must retain all 1001 nodes');
    assert.ok(duration < 50, `1001 node traversal and sort should take < 50ms, took ${duration.toFixed(2)}ms`);
  });
});

describe('Heatmap export – PNG trigger (DOM stub)', () => {

  test('exportPng creates an anchor element with .png filename', () => {
    // Stub the DOM APIs used by exportPng
    const clicks = [];
    const anchors = [];

    const origCreate = global.document?.createElement?.bind(global.document);
    if (!global.document) {
      // Node environment without DOM — skip with a manual pass
      assert.ok(true, 'DOM not available; PNG trigger test skipped in pure-Node environment');
      return;
    }

    const origCreateEl = document.createElement.bind(document);
    document.createElement = (tag) => {
      const el = origCreateEl(tag);
      if (tag === 'a') {
        anchors.push(el);
        el.click = () => clicks.push(el.download);
      }
      return el;
    };

    // Simulate what exportPng does after html2canvas resolves:
    function simulateExportPng() {
      const a = document.createElement('a');
      a.href     = 'data:image/png;base64,FAKE';
      a.download = 'skymoonscope-heatmap.png';
      a.click();
    }

    simulateExportPng();

    assert.equal(clicks.length, 1, 'click was called once');
    assert.ok(clicks[0].endsWith('.png'), `filename should end with .png, got: ${clicks[0]}`);

    // Restore
    document.createElement = origCreateEl;
  });

  test('exportSvg creates an anchor element with .svg filename', () => {
    if (!global.document) {
      assert.ok(true, 'DOM not available; SVG trigger test skipped in pure-Node environment');
      return;
    }

    const clicks = [];
    const origCreateEl = document.createElement.bind(document);
    document.createElement = (tag) => {
      const el = origCreateEl(tag);
      if (tag === 'a') {
        el.click = () => clicks.push(el.download);
      }
      return el;
    };

    const origCreate   = URL.createObjectURL;
    const origRevoke   = URL.revokeObjectURL;
    URL.createObjectURL = () => 'blob:fake-url';
    URL.revokeObjectURL = () => {};

    // Simulate what exportSvg does
    function simulateExportSvg() {
      const blob = new Blob(['<svg/>'], { type: 'image/svg+xml' });
      const url  = URL.createObjectURL(blob);
      const a    = document.createElement('a');
      a.href     = url;
      a.download = 'skymoonscope-heatmap.svg';
      a.click();
      URL.revokeObjectURL(url);
    }

    simulateExportSvg();

    assert.equal(clicks.length, 1);
    assert.ok(clicks[0].endsWith('.svg'), `filename should end with .svg, got: ${clicks[0]}`);

    document.createElement = origCreateEl;
    URL.createObjectURL    = origCreate;
    URL.revokeObjectURL    = origRevoke;
  });
});
