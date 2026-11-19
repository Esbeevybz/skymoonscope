export interface VisibleRangeOptions {
  totalItems: number;
  cols?: number;
  rowHeight?: number;
  scrollTop?: number;
  viewportHeight?: number;
  overscan?: number;
}

export interface VisibleRangeResult {
  startIndex: number;
  endIndex: number;
  startRow: number;
  endRow: number;
  totalRows: number;
  totalHeight: number;
  offsetY: number;
  isVirtualized: boolean;
  visibleCount: number;
}

export interface BreakpointsInput {
  critical?: number;
  high?: number;
  medium?: number;
  low?: number;
}

export function computeVisibleRange(options: VisibleRangeOptions): VisibleRangeResult;

export function getCellSeverityBand(
  share: number,
  breakpoints?: BreakpointsInput
): 'CRITICAL' | 'HIGH' | 'MEDIUM' | 'LOW' | 'TRACE';
