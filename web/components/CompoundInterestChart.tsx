'use client';

import React, { useMemo } from 'react';
import {
  ResponsiveContainer,
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ReferenceLine,
} from 'recharts';
import { TrendingUp } from 'lucide-react';
import type { StakingYieldResult } from '../lib/stakingCalculator';

// ─── Types ────────────────────────────────────────────────────────────────────

export interface CompoundInterestChartProps {
  /**
   * Full result from `calculateStakingYield`.  The chart derives all data
   * points from `breakdownByMonth` so callers don't need to massage anything.
   */
  result: StakingYieldResult;
  /** Highlight the break-even month (defaults to true). */
  showBreakEven?: boolean;
  /** Card height in pixels (defaults to 260). */
  height?: number;
  /** Optional extra className on the outer wrapper. */
  className?: string;
}

/**
 * A single chart data point combining principal, cumulative rewards, and a
 * stacked-area-friendly total balance value.
 */
export interface ChartDataPoint {
  /** Month label shown on the x-axis ("M1", "M6", …). */
  label: string;
  /** Month index (1-based). */
  month: number;
  /** Original deposit — constant across all months. */
  principal: number;
  /** Compound interest earned by this month. */
  rewards: number;
  /** principal + rewards (totalBalance at month). */
  balance: number;
}

// ─── Pure data-transformation helper ─────────────────────────────────────────

/**
 * Convert `breakdownByMonth` rows into chart-ready points.
 *
 * This is a pure function exported for unit-testing — it contains all the
 * non-trivial logic that the chart component depends on.
 */
export function buildChartData(result: StakingYieldResult): ChartDataPoint[] {
  const { depositAmount, breakdownByMonth } = result;

  return breakdownByMonth.map((row) => ({
    label: row.month === 1 || row.month % 6 === 0 ? `M${row.month}` : '',
    month: row.month,
    principal: depositAmount,
    rewards: Number(row.yieldEarned.toFixed(2)),
    balance: Number(row.balance.toFixed(2)),
  }));
}

// ─── Custom tooltip ───────────────────────────────────────────────────────────

interface TooltipPayloadEntry {
  name: string;
  value: number;
  color: string;
}

interface CustomTooltipProps {
  active?: boolean;
  payload?: TooltipPayloadEntry[];
  label?: string | number;
}

function CustomTooltip({ active, payload, label }: CustomTooltipProps) {
  if (!active || !payload || payload.length === 0) return null;

  const rewards = payload.find((p) => p.name === 'rewards')?.value ?? 0;
  const principal = payload.find((p) => p.name === 'principal')?.value ?? 0;
  const total = principal + rewards;

  return (
    <div className="rounded-xl border border-slate-700 bg-slate-900/95 px-4 py-3 shadow-xl backdrop-blur-sm text-xs">
      <p className="mb-2 font-semibold text-slate-300">Month {label}</p>
      <div className="space-y-1">
        <div className="flex items-center justify-between gap-6">
          <span className="flex items-center gap-1.5 text-slate-400">
            <span className="inline-block h-2 w-2 rounded-full bg-cyan-500/60" />
            Principal
          </span>
          <span className="font-mono font-semibold text-slate-200">
            {principal.toLocaleString()}
          </span>
        </div>
        <div className="flex items-center justify-between gap-6">
          <span className="flex items-center gap-1.5 text-slate-400">
            <span className="inline-block h-2 w-2 rounded-full bg-emerald-400" />
            Rewards
          </span>
          <span className="font-mono font-semibold text-emerald-400">
            +{rewards.toLocaleString()}
          </span>
        </div>
        <div className="mt-1 flex items-center justify-between gap-6 border-t border-slate-700 pt-1">
          <span className="text-slate-300 font-medium">Total</span>
          <span className="font-mono font-bold text-cyan-300">{total.toLocaleString()}</span>
        </div>
      </div>
    </div>
  );
}

// ─── Y-axis tick formatter ────────────────────────────────────────────────────

function formatYAxis(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(0)}K`;
  return String(value);
}

// ─── Main component ───────────────────────────────────────────────────────────

/**
 * CompoundInterestChart
 *
 * Renders an animated stacked AreaChart showing how the principal (constant)
 * and cumulative compound rewards grow over the lock duration.
 *
 * Uses recharts `AreaChart` with `isAnimationActive` (the recharts animation
 * prop) so the chart draws in on first render or when props change.
 */
export function CompoundInterestChart({
  result,
  showBreakEven = true,
  height = 260,
  className = '',
}: CompoundInterestChartProps) {
  const chartData = useMemo(() => buildChartData(result), [result]);

  // Break-even threshold: the first month where rewards exceed 10% of principal
  const breakEvenMonth = useMemo(() => {
    if (!showBreakEven || result.depositAmount === 0) return null;
    const threshold = result.depositAmount * 0.1;
    const found = result.breakdownByMonth.find((r) => r.yieldEarned >= threshold);
    return found ? found.month : null;
  }, [result, showBreakEven]);

  // Maximum balance value — used to position the y-axis domain nicely
  const maxBalance = useMemo(
    () => Math.max(...chartData.map((d) => d.balance), result.depositAmount),
    [chartData, result.depositAmount],
  );

  const yDomain: [number, number] = [0, Math.ceil(maxBalance * 1.08)];

  // Gradient IDs — deterministic to avoid hydration mismatches
  const gradientPrincipal = 'ccPrincipalGrad';
  const gradientRewards = 'ccRewardsGrad';

  const isEmpty = chartData.length === 0;
  const isSinglePoint = chartData.length === 1;

  return (
    <div
      className={`rounded-2xl border border-slate-800 bg-slate-900/90 p-5 shadow-xl ${className}`}
    >
      {/* Header */}
      <div className="mb-4 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <TrendingUp className="h-4 w-4 text-cyan-400" aria-hidden="true" />
          <h3 className="text-sm font-semibold text-slate-200">Compound Growth Over Time</h3>
        </div>
        <div className="flex items-center gap-3 text-[11px] text-slate-400">
          <span className="flex items-center gap-1">
            <span className="inline-block h-2 w-6 rounded-full bg-cyan-500/50" />
            Principal
          </span>
          <span className="flex items-center gap-1">
            <span className="inline-block h-2 w-6 rounded-full bg-emerald-400/70" />
            Rewards
          </span>
        </div>
      </div>

      {/* Chart area */}
      {isEmpty ? (
        <div
          className="flex items-center justify-center text-sm text-slate-500"
          style={{ height }}
          role="img"
          aria-label="No chart data available"
        >
          No data to display
        </div>
      ) : isSinglePoint ? (
        /* 1-month edge case: recharts doesn't render areas well with a single
           data point, so we show a simple summary card instead. */
        <div
          className="flex flex-col items-center justify-center gap-2 rounded-xl border border-slate-800 bg-slate-950/50"
          style={{ height }}
          role="img"
          aria-label={`Single month projection: balance ${chartData[0].balance.toLocaleString()}`}
        >
          <p className="text-xs text-slate-400">1-Month Projection</p>
          <p className="font-mono text-2xl font-bold text-cyan-300">
            {chartData[0].balance.toLocaleString()}
          </p>
          <p className="text-xs text-emerald-400">
            +{chartData[0].rewards.toLocaleString()} rewards
          </p>
        </div>
      ) : (
        <ResponsiveContainer width="100%" height={height}>
          <AreaChart
            data={chartData}
            margin={{ top: 8, right: 8, left: 0, bottom: 0 }}
          >
            <defs>
              {/* Gradient fill for the principal area */}
              <linearGradient id={gradientPrincipal} x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="#06b6d4" stopOpacity={0.25} />
                <stop offset="95%" stopColor="#06b6d4" stopOpacity={0.04} />
              </linearGradient>
              {/* Gradient fill for the rewards area */}
              <linearGradient id={gradientRewards} x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="#34d399" stopOpacity={0.45} />
                <stop offset="95%" stopColor="#34d399" stopOpacity={0.05} />
              </linearGradient>
            </defs>

            <CartesianGrid
              strokeDasharray="3 3"
              stroke="#1e293b"
              vertical={false}
            />

            <XAxis
              dataKey="month"
              tick={{ fill: '#64748b', fontSize: 11 }}
              tickLine={false}
              axisLine={{ stroke: '#1e293b' }}
              tickFormatter={(v: number) =>
                v === 1 || v % 6 === 0 ? `M${v}` : ''
              }
            />

            <YAxis
              domain={yDomain}
              tickFormatter={formatYAxis}
              tick={{ fill: '#64748b', fontSize: 11 }}
              tickLine={false}
              axisLine={false}
              width={48}
            />

            <Tooltip
              content={<CustomTooltip />}
              cursor={{ stroke: '#334155', strokeWidth: 1 }}
            />

            {/* Break-even reference line */}
            {breakEvenMonth !== null && (
              <ReferenceLine
                x={breakEvenMonth}
                stroke="#fbbf24"
                strokeDasharray="4 3"
                strokeWidth={1.5}
                label={{
                  value: '10% return',
                  position: 'insideTopRight',
                  fill: '#fbbf24',
                  fontSize: 10,
                }}
              />
            )}

            {/* Stacked areas: principal at the bottom, rewards on top */}
            <Area
              type="monotone"
              dataKey="principal"
              name="principal"
              stackId="compound"
              stroke="#06b6d4"
              strokeWidth={1.5}
              fill={`url(#${gradientPrincipal})`}
              dot={false}
              activeDot={{ r: 4, fill: '#06b6d4', stroke: '#0e7490', strokeWidth: 2 }}
              isAnimationActive={true}
              animationDuration={900}
              animationEasing="ease-out"
            />
            <Area
              type="monotone"
              dataKey="rewards"
              name="rewards"
              stackId="compound"
              stroke="#34d399"
              strokeWidth={2}
              fill={`url(#${gradientRewards})`}
              dot={false}
              activeDot={{ r: 5, fill: '#34d399', stroke: '#059669', strokeWidth: 2 }}
              isAnimationActive={true}
              animationDuration={900}
              animationEasing="ease-out"
            />
          </AreaChart>
        </ResponsiveContainer>
      )}

      {/* Summary footer */}
      <div className="mt-4 flex items-center justify-between border-t border-slate-800 pt-3 text-[11px] text-slate-400">
        <span>
          <span className="font-semibold text-slate-300">{result.lockDurationMonths}</span> months
          &nbsp;·&nbsp;
          <span className="font-semibold text-cyan-400">{result.effectiveApyPercent}%</span> effective APY
          {result.multiplier !== 1 && (
            <span className="ml-1 text-slate-500">({result.multiplier}x multiplier)</span>
          )}
        </span>
        <span>
          Final balance:{' '}
          <span className="font-mono font-semibold text-cyan-300">
            {result.totalBalance.toLocaleString()}
          </span>
        </span>
      </div>
    </div>
  );
}
