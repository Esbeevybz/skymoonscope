import React, { useEffect, useRef, useState } from 'react';
import { cn } from '../../lib/utils';
import {
  LIMITS,
  hotspotColors,
  CATEGORY_STYLE,
} from './HeatmapColorScale';
import type { CpuHotspotCell, ScaleBreakpoints } from './HeatmapCell';

export function fmtInstrFull(n: number): string {
  return new Intl.NumberFormat('en-US').format(n);
}

export interface TooltipPosition {
  x: number;
  y: number;
}

export interface HeatmapFloatingTooltipProps {
  cell: CpuHotspotCell | null;
  position: TooltipPosition | null;
  breakpoints?: ScaleBreakpoints;
  /** Optional container ref to keep tooltip inside bounds */
  containerRef?: React.RefObject<HTMLElement | null>;
}

const TOOLTIP_WIDTH = 224; // px — matches `w-56`
const TOOLTIP_HEIGHT = 260; // px — approx max height
const CURSOR_OFFSET_X = 14;
const CURSOR_OFFSET_Y = 14;

/**
 * Floating tooltip that positions itself near the cursor when hovering a
 * heatmap cell. Renders as a fixed-position overlay so it can escape any
 * overflow:hidden container and always stays within the viewport.
 *
 * Shows exact values for:
 *  - Full qualified function name
 *  - Category
 *  - CPU share (%)
 *  - Absolute instruction count (exact integer, human-formatted)
 *  - Budget usage (% of 100M limit)
 *  - Call-graph depth
 *  - Severity level
 */
export function HeatmapFloatingTooltip({
  cell,
  position,
  breakpoints,
  containerRef,
}: HeatmapFloatingTooltipProps) {
  const tooltipRef = useRef<HTMLDivElement>(null);
  const [adjustedPos, setAdjustedPos] = useState<TooltipPosition | null>(null);

  useEffect(() => {
    if (!position) {
      setAdjustedPos(null);
      return;
    }

    let x = position.x + CURSOR_OFFSET_X;
    let y = position.y + CURSOR_OFFSET_Y;

    const vw = window.innerWidth;
    const vh = window.innerHeight;

    // Flip horizontally if tooltip would overflow viewport right edge
    if (x + TOOLTIP_WIDTH > vw - 8) {
      x = position.x - TOOLTIP_WIDTH - CURSOR_OFFSET_X;
    }

    // Flip vertically if tooltip would overflow viewport bottom edge
    if (y + TOOLTIP_HEIGHT > vh - 8) {
      y = position.y - TOOLTIP_HEIGHT - CURSOR_OFFSET_Y;
    }

    // Ensure we don't go off the top or left
    x = Math.max(8, x);
    y = Math.max(8, y);

    setAdjustedPos({ x, y });
  }, [position]);

  if (!cell || !position || !adjustedPos) return null;

  const clr = hotspotColors(cell.cpuShare, breakpoints);
  const catStyle = CATEGORY_STYLE[cell.category];
  const budgetPct = Math.min((cell.cpuInstructions / LIMITS.CPU) * 100, 100);

  return (
    <div
      ref={tooltipRef}
      role="tooltip"
      data-testid="heatmap-floating-tooltip"
      aria-label={`Tooltip for ${cell.fnName}`}
      className={cn(
        'fixed z-[9999] pointer-events-none',
        'w-56 rounded-xl border shadow-2xl',
        'bg-slate-950/95 backdrop-blur-sm',
        clr.border,
      )}
      style={{
        left: adjustedPos.x,
        top: adjustedPos.y,
      }}
    >
      {/* Severity accent bar at the top */}
      <div
        className="h-1 w-full rounded-t-xl"
        style={{ backgroundColor: clr.barHex }}
      />

      <div className="p-3 space-y-2.5">
        {/* Header: severity badge + category */}
        <div className="flex items-center justify-between gap-1.5">
          <span
            className={cn(
              'text-[8px] font-mono font-bold rounded px-1.5 py-0.5 border uppercase tracking-wider',
              clr.badge,
              clr.border,
            )}
          >
            {clr.label}
          </span>
          <span
            className={cn(
              'text-[8px] font-mono rounded px-1.5 py-0.5 border uppercase tracking-wider',
              catStyle.cls,
            )}
          >
            {catStyle.label}
          </span>
        </div>

        {/* Full function name */}
        <div>
          <span className="text-[8px] text-slate-500 font-mono uppercase tracking-wider block mb-0.5">
            FUNCTION
          </span>
          <code
            className="text-[10px] font-mono text-slate-100 break-all leading-snug block bg-slate-900/80 rounded px-1.5 py-1 border border-slate-800"
            title={cell.fnName}
          >
            {cell.fnName}
          </code>
        </div>

        {/* Exact values grid */}
        <div className="grid grid-cols-2 gap-1.5">
          {/* CPU share */}
          <div className="bg-slate-900/70 border border-slate-800 rounded p-1.5">
            <span className="text-[7px] font-mono text-slate-500 uppercase block leading-none mb-0.5">
              CPU SHARE
            </span>
            <span className={cn('text-sm font-mono font-black leading-none block', clr.text)}>
              {cell.cpuShare.toFixed(2)}%
            </span>
          </div>

          {/* Depth */}
          <div className="bg-slate-900/70 border border-slate-800 rounded p-1.5">
            <span className="text-[7px] font-mono text-slate-500 uppercase block leading-none mb-0.5">
              DEPTH
            </span>
            <span className="text-sm font-mono font-black text-slate-100 leading-none block">
              {cell.depth}
            </span>
          </div>

          {/* Instruction count — full row, exact integer */}
          <div className="col-span-2 bg-slate-900/70 border border-slate-800 rounded p-1.5">
            <span className="text-[7px] font-mono text-slate-500 uppercase block leading-none mb-0.5">
              INSTRUCTIONS (EXACT)
            </span>
            <span className={cn('text-[11px] font-mono font-black leading-none block tabular-nums', clr.text)}>
              {fmtInstrFull(cell.cpuInstructions)}
            </span>
          </div>

          {/* Budget bar — full row */}
          <div className="col-span-2 bg-slate-900/70 border border-slate-800 rounded p-1.5">
            <div className="flex items-center justify-between mb-1">
              <span className="text-[7px] font-mono text-slate-500 uppercase leading-none">
                OF 100M BUDGET
              </span>
              <span className="text-[8px] font-mono text-slate-400 tabular-nums">
                {budgetPct.toFixed(3)}%
              </span>
            </div>
            <div className="h-1.5 w-full bg-black/40 rounded-full overflow-hidden">
              <div
                className="h-full rounded-full transition-[width] duration-300"
                style={{
                  width: `${Math.max(budgetPct, 0.5)}%`,
                  backgroundColor: clr.barHex,
                }}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
