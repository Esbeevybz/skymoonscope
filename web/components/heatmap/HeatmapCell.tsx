import React, { useCallback, useMemo } from 'react';
import { cn } from '../../lib/utils';
import {
  type FnCategory,
  type HotspotColors,
  type ScaleBreakpoints,
  hotspotColors,
  CATEGORY_STYLE,
} from './HeatmapColorScale';

export type { ScaleBreakpoints };

export interface CpuHotspotCell {
  id: string;
  /** Full qualified name, e.g. "contract::function" */
  fnName: string;
  /** Abbreviated label that fits inside the cell */
  displayName: string;
  category: FnCategory;
  /** Share of this simulation's total CPU (0–100) */
  cpuShare: number;
  /** Absolute estimated instruction count */
  cpuInstructions: number;
  /** Call-graph depth; 0 = entry point */
  depth: number;
}

export interface HeatmapCellProps {
  cell: CpuHotspotCell;
  rank?: number;
  isHovered?: boolean;
  breakpoints?: ScaleBreakpoints;
  onHover?: (id: string) => void;
  onLeave?: () => void;
  onMouseEnter?: () => void;
  onMouseLeave?: () => void;
  onClick?: () => void;
  className?: string;
  style?: React.CSSProperties;
}

function HeatmapCellComponent({
  cell,
  rank,
  isHovered = false,
  breakpoints,
  onHover,
  onLeave,
  onMouseEnter,
  onMouseLeave,
  onClick,
  className,
  style,
}: HeatmapCellProps) {
  const clr: HotspotColors = useMemo(
    () => hotspotColors(cell.cpuShare, breakpoints),
    [cell.cpuShare, breakpoints],
  );
  const catStyle = CATEGORY_STYLE[cell.category];

  const handleMouseEnter = useCallback(() => {
    if (onHover) {
      onHover(cell.id);
    }
    if (onMouseEnter) {
      onMouseEnter();
    }
  }, [onHover, onMouseEnter, cell.id]);

  const handleMouseLeave = useCallback(() => {
    if (onLeave) {
      onLeave();
    }
    if (onMouseLeave) {
      onMouseLeave();
    }
  }, [onLeave, onMouseLeave]);

  // Pre-compose static and dynamic class names efficiently
  const baseButtonClass =
    'group relative flex flex-col justify-between rounded-lg border p-2.5 text-left transition-transform duration-200 cursor-crosshair';
  const hoverEffectClass = isHovered
    ? 'scale-[1.06] z-20 ring-2 ring-white/20 shadow-lg'
    : 'hover:scale-[1.02]';

  return (
    <button
      key={cell.id}
      type="button"
      onClick={onClick}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      className={className ? cn(baseButtonClass, clr.bg, clr.border, hoverEffectClass, className) : `${baseButtonClass} ${clr.bg} ${clr.border} ${hoverEffectClass}`}
      style={{ minHeight: '84px', ...style }}
    >
      {/* Rank + category badges */}
      <div className="flex items-start justify-between gap-1 mb-1.5">
        {typeof rank === 'number' && (
          <span className={`text-[8px] font-black font-mono rounded px-1 py-0.5 leading-none ${clr.badge}`}>
            #{rank + 1}
          </span>
        )}
        <span className={`text-[8px] font-mono rounded px-1 py-0.5 leading-none border ${catStyle.cls}`}>
          {catStyle.label}
        </span>
      </div>

      {/* Function display name */}
      <div className={`text-[10px] font-bold font-mono leading-tight ${clr.text}`}>
        {cell.displayName}
      </div>

      {/* Mini bar + share % */}
      <div className="mt-2 w-full">
        <div className="flex items-center justify-between mb-0.5">
          <span className={`text-[9px] font-mono font-bold ${clr.text}`}>
            {cell.cpuShare.toFixed(1)}%
          </span>
          <span className="text-[8px] text-slate-500 font-mono">{clr.label}</span>
        </div>
        <div className="h-1 w-full bg-black/30 rounded-full overflow-hidden">
          <div
            className="h-full rounded-full transition-[width] duration-500"
            style={{
              width: `${Math.min(cell.cpuShare * 4, 100)}%`,
              backgroundColor: clr.barHex,
              opacity: 0.9,
            }}
          />
        </div>
      </div>
    </button>
  );
}

export const HeatmapCell = React.memo(HeatmapCellComponent);
