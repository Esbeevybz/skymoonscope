'use strict';

/**
 * heatmapVirtualizer.js — High-performance grid windowing and layout calculations
 * for resource heatmaps containing hundreds or thousands of cells.
 */

/**
 * Computes the visible window of cells for a grid container.
 *
 * @param {Object} options
 * @param {number} options.totalItems - Total number of cells.
 * @param {number} [options.cols=4] - Number of columns in the grid.
 * @param {number} [options.rowHeight=92] - Height of each row including gap (px).
 * @param {number} [options.scrollTop=0] - Current scroll offset of the container (px).
 * @param {number} [options.viewportHeight=560] - Height of the visible scroll container (px).
 * @param {number} [options.overscan=2] - Extra rows to render above and below visible area.
 * @returns {Object} Range calculation and layout metrics.
 */
function computeVisibleRange(options) {
  const totalItems = Math.max(0, options?.totalItems ?? 0);
  const cols = Math.max(1, options?.cols ?? 4);
  const rowHeight = Math.max(1, options?.rowHeight ?? 92);
  const scrollTop = Math.max(0, options?.scrollTop ?? 0);
  const viewportHeight = Math.max(0, options?.viewportHeight ?? 560);
  const overscan = Math.max(0, options?.overscan ?? 2);

  if (totalItems === 0) {
    return {
      startIndex: 0,
      endIndex: 0,
      startRow: 0,
      endRow: 0,
      totalRows: 0,
      totalHeight: 0,
      offsetY: 0,
      isVirtualized: false,
      visibleCount: 0,
    };
  }

  const totalRows = Math.ceil(totalItems / cols);
  const totalHeight = totalRows * rowHeight;

  // When cell count is small (<= 24) or fits comfortably in viewport, no virtualization needed
  const shouldVirtualize = totalItems > 24 && totalHeight > viewportHeight;

  if (!shouldVirtualize) {
    return {
      startIndex: 0,
      endIndex: totalItems,
      startRow: 0,
      endRow: totalRows,
      totalRows,
      totalHeight,
      offsetY: 0,
      isVirtualized: false,
      visibleCount: totalItems,
    };
  }

  const maxScroll = Math.max(0, totalHeight - viewportHeight);
  const clampedScrollTop = Math.min(scrollTop, maxScroll);

  const rawStartRow = Math.floor(clampedScrollTop / rowHeight);
  const rawEndRow = Math.ceil((clampedScrollTop + viewportHeight) / rowHeight);

  const startRow = Math.max(0, rawStartRow - overscan);
  const endRow = Math.min(totalRows, rawEndRow + overscan);

  const startIndex = Math.min(totalItems, startRow * cols);
  const endIndex = Math.min(totalItems, Math.max(startIndex, endRow * cols));
  const offsetY = startRow * rowHeight;

  return {
    startIndex,
    endIndex,
    startRow,
    endRow,
    totalRows,
    totalHeight,
    offsetY,
    isVirtualized: true,
    visibleCount: endIndex - startIndex,
  };
}

/**
 * Fast lookup map for cell severity threshold and color properties.
 * Avoids repeated object allocations across 500+ cells.
 */
function getCellSeverityBand(share, breakpoints) {
  const critical = breakpoints?.critical ?? 20;
  const high = breakpoints?.high ?? 10;
  const medium = breakpoints?.medium ?? 5;
  const low = breakpoints?.low ?? 2;

  if (share >= critical) return 'CRITICAL';
  if (share >= high) return 'HIGH';
  if (share >= medium) return 'MEDIUM';
  if (share >= low) return 'LOW';
  return 'TRACE';
}

module.exports = {
  computeVisibleRange,
  getCellSeverityBand,
};
