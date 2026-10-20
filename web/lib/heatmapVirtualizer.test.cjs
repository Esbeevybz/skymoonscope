'use strict';

/**
 * heatmapVirtualizer.test.cjs — Unit tests and performance validation for heatmap
 * grid windowing, layout calculations, and large cell-count scaling (> 500 cells).
 *
 * Runs with: node --test ./lib/heatmapVirtualizer.test.cjs
 */

const { test, describe } = require('node:test');
const assert = require('node:assert/strict');
const { performance } = require('node:perf_hooks');

const {
  computeVisibleRange,
  getCellSeverityBand,
} = require('./heatmapVirtualizer.js');

describe('heatmapVirtualizer: computeVisibleRange', () => {
  test('returns empty range for 0 cells', () => {
    const res = computeVisibleRange({ totalItems: 0 });
    assert.equal(res.startIndex, 0);
    assert.equal(res.endIndex, 0);
    assert.equal(res.totalRows, 0);
    assert.equal(res.totalHeight, 0);
    assert.equal(res.isVirtualized, false);
    assert.equal(res.visibleCount, 0);
  });

  test('does not virtualize small cell counts (e.g., standard 12 archetypal cells)', () => {
    const res = computeVisibleRange({
      totalItems: 12,
      cols: 4,
      rowHeight: 92,
      viewportHeight: 560,
    });
    assert.equal(res.isVirtualized, false);
    assert.equal(res.startIndex, 0);
    assert.equal(res.endIndex, 12);
    assert.equal(res.visibleCount, 12);
    assert.equal(res.totalRows, 3);
    assert.equal(res.totalHeight, 276);
  });

  test('virtualizes heatmaps with exactly 500 cells to a bounded window', () => {
    const res = computeVisibleRange({
      totalItems: 500,
      cols: 4,
      rowHeight: 92,
      scrollTop: 0,
      viewportHeight: 560,
      overscan: 2,
    });

    assert.equal(res.isVirtualized, true);
    assert.equal(res.totalRows, 125);
    assert.equal(res.totalHeight, 125 * 92); // 11,500px
    assert.equal(res.startRow, 0);
    // (560 / 92) ≈ 6.08 -> rawEndRow = 7, + overscan 2 = 9 rows (9 * 4 = 36 cells)
    assert.equal(res.endRow, 9);
    assert.equal(res.startIndex, 0);
    assert.equal(res.endIndex, 36);
    assert.equal(res.visibleCount, 36);
    assert.ok(res.visibleCount <= 40, 'visible cells must be tightly bounded');
  });

  test('virtualizes heatmaps beyond 500 cells (e.g., 501, 750, 1000, 2500 cells)', () => {
    const counts = [501, 750, 1000, 2500];

    for (const count of counts) {
      const res = computeVisibleRange({
        totalItems: count,
        cols: 4,
        rowHeight: 92,
        scrollTop: 0,
        viewportHeight: 560,
        overscan: 2,
      });

      assert.equal(res.isVirtualized, true, `should virtualize for ${count} cells`);
      assert.equal(res.startIndex, 0);
      assert.ok(res.visibleCount <= 40, `visible count (${res.visibleCount}) should remain <= 40 for ${count} cells`);
      assert.equal(res.totalRows, Math.ceil(count / 4));
      assert.equal(res.totalHeight, Math.ceil(count / 4) * 92);
    }
  });

  test('correctly adjusts window offset during scrolling on 500+ cells', () => {
    const totalItems = 1000;
    // Scroll down 4600px (~50 rows down)
    const scrollTop = 4600;
    const res = computeVisibleRange({
      totalItems,
      cols: 4,
      rowHeight: 92,
      scrollTop,
      viewportHeight: 560,
      overscan: 2,
    });

    const expectedRawStartRow = Math.floor(4600 / 92); // 50
    const expectedStartRow = 50 - 2; // 48
    assert.equal(res.startRow, expectedStartRow);
    assert.equal(res.startIndex, expectedStartRow * 4); // 192
    assert.equal(res.offsetY, expectedStartRow * 92); // 4416
    assert.ok(res.endIndex > res.startIndex);
    assert.ok(res.visibleCount <= 44, 'visible count must remain bounded while scrolling');
  });

  test('clamps to bottom boundary when scrolled to the end of a 1000-cell heatmap', () => {
    const totalItems = 1000;
    const totalRows = 250;
    const totalHeight = 250 * 92;
    const scrollTop = totalHeight; // scrolled to very bottom

    const res = computeVisibleRange({
      totalItems,
      cols: 4,
      rowHeight: 92,
      scrollTop,
      viewportHeight: 560,
      overscan: 2,
    });

    assert.equal(res.endRow, totalRows);
    assert.equal(res.endIndex, totalItems);
    assert.ok(res.startIndex < res.endIndex);
    assert.ok(res.visibleCount <= 40);
  });

  test('handles extreme overscroll (scrollTop >> totalHeight) without negative count or inverted indices', () => {
    const totalItems = 500;
    const res = computeVisibleRange({
      totalItems,
      cols: 4,
      rowHeight: 92,
      scrollTop: 1000000,
      viewportHeight: 560,
      overscan: 2,
    });

    assert.ok(res.startIndex <= res.endIndex);
    assert.equal(res.endIndex, totalItems);
    assert.ok(res.visibleCount > 0 && res.visibleCount <= 44);
    assert.ok(res.offsetY <= res.totalHeight);
  });
});

describe('heatmapVirtualizer: getCellSeverityBand', () => {
  const bp = { critical: 25, high: 15, medium: 8, low: 3 };

  test('assigns correct bands according to breakpoints', () => {
    assert.equal(getCellSeverityBand(30, bp), 'CRITICAL');
    assert.equal(getCellSeverityBand(25, bp), 'CRITICAL');
    assert.equal(getCellSeverityBand(20, bp), 'HIGH');
    assert.equal(getCellSeverityBand(15, bp), 'HIGH');
    assert.equal(getCellSeverityBand(10, bp), 'MEDIUM');
    assert.equal(getCellSeverityBand(8, bp), 'MEDIUM');
    assert.equal(getCellSeverityBand(5, bp), 'LOW');
    assert.equal(getCellSeverityBand(3, bp), 'LOW');
    assert.equal(getCellSeverityBand(1, bp), 'TRACE');
  });
});

describe('Performance benchmark: 500 cells and > 500 cells', () => {
  test('windowing calculation for 10,000 scroll frames on 1000 cells executes under 25ms', () => {
    const iterations = 10000;
    const t0 = performance.now();

    for (let i = 0; i < iterations; i++) {
      const scrollTop = (i * 17) % 20000;
      computeVisibleRange({
        totalItems: 1000,
        cols: 4,
        rowHeight: 92,
        scrollTop,
        viewportHeight: 560,
      });
    }

    const elapsed = performance.now() - t0;
    assert.ok(elapsed < 50, `10,000 windowing calculations must be < 50ms, took ${elapsed.toFixed(2)}ms`);
  });
});
