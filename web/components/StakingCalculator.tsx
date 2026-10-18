import React, { useState, useMemo, useEffect, useCallback } from 'react';
import {
  calculateStakingYield,
  formatMilestoneScheduleCSV,
  exportMilestoneScheduleCSV,
  compareStakingScenarios,
  formatScenarioComparisonCSV,
  DEFAULT_STAKING_SCENARIOS,
  type StakingScenario,
} from '../lib/stakingCalculator';
import { CompoundInterestChart } from './CompoundInterestChart';
import {
  Calculator,
  TrendingUp,
  Calendar,
  Zap,
  DollarSign,
  RefreshCw,
  BarChart2,
  AlertTriangle,
  Download,
  Check,
  Scale,
  Plus,
  Trash2,
  ArrowRight,
  Trophy,
  Percent,
} from 'lucide-react';

export interface StakingCalculatorProps {
  initialDeposit?: number;
  initialDurationMonths?: number;
  initialBaseApy?: number;
  className?: string;
  stakeLockWarningThreshold?: number;
  onStakeLockWarningChange?: (showWarning: boolean) => void;
}

export function StakingCalculator({
  initialDeposit = 1000,
  initialDurationMonths = 12,
  initialBaseApy = 12,
  className = '',
  stakeLockWarningThreshold,
  onStakeLockWarningChange,
}: StakingCalculatorProps) {
  const [depositAmount, setDepositAmount] = useState<number>(initialDeposit);
  const [lockDurationMonths, setLockDurationMonths] = useState<number>(() =>
    Math.min(36, Math.max(1, Math.round(initialDurationMonths))),
  );
  const [baseApyPercentage, setBaseApyPercentage] = useState<number>(initialBaseApy);
  const [compoundFrequency, setCompoundFrequency] = useState<string>('monthly');
  const [enableTierMultiplier, setEnableTierMultiplier] = useState<boolean>(true);
  const [showTable, setShowTable] = useState<boolean>(false);
  const [exported, setExported] = useState<boolean>(false);
  const [enableInflationAdjustment, setEnableInflationAdjustment] = useState<boolean>(false);
  const [annualInflationRate, setAnnualInflationRate] = useState<number>(3.0);
  const [scenarios, setScenarios] = useState<StakingScenario[]>(() => DEFAULT_STAKING_SCENARIOS);
  const [showComparison, setShowComparison] = useState<boolean>(true);
  const [baselineIndex, setBaselineIndex] = useState<number>(0);
  const [exportedComparison, setExportedComparison] = useState<boolean>(false);
  const [customScenarioName, setCustomScenarioName] = useState<string>('');

  const results = useMemo(() => {
    return calculateStakingYield({
      depositAmount,
      lockDurationMonths,
      compoundFrequency,
      baseApyPercentage,
      enableTierMultiplier,
      stakeLockWarningThreshold,
      enableInflationAdjustment,
      annualInflationRate,
    });
  }, [
    depositAmount,
    lockDurationMonths,
    compoundFrequency,
    baseApyPercentage,
    enableTierMultiplier,
    stakeLockWarningThreshold,
    enableInflationAdjustment,
    annualInflationRate,
  ]);

  useEffect(() => {
    if (onStakeLockWarningChange) {
      onStakeLockWarningChange(results.showStakeLockWarning);
    }
  }, [results.showStakeLockWarning, onStakeLockWarningChange]);

  const exportToCSV = useCallback(() => {
    if (!results.breakdownByMonth || results.breakdownByMonth.length === 0) return;
    exportMilestoneScheduleCSV(results);
    setExported(true);
    setTimeout(() => setExported(false), 2000);
  }, [results]);

  const comparison = useMemo(() => {
    return compareStakingScenarios(scenarios, {
      baselineIndex,
      enableInflationAdjustment,
      annualInflationRate,
    });
  }, [scenarios, baselineIndex, enableInflationAdjustment, annualInflationRate]);

  const handleApplyScenario = useCallback((scenario: StakingScenario) => {
    setDepositAmount(scenario.depositAmount);
    setLockDurationMonths(scenario.lockDurationMonths);
    setBaseApyPercentage(scenario.baseApyPercentage);
    setCompoundFrequency(scenario.compoundFrequency);
    setEnableTierMultiplier(scenario.enableTierMultiplier);
  }, []);

  const handleSaveCurrentAsScenario = useCallback(() => {
    const name = customScenarioName.trim() || `Config (${lockDurationMonths}M, ${baseApyPercentage}%)`;
    const newScenario: StakingScenario = {
      id: `scenario-${Date.now()}`,
      name,
      depositAmount,
      lockDurationMonths,
      baseApyPercentage,
      compoundFrequency,
      enableTierMultiplier,
    };

    setScenarios((prev) => {
      if (prev.length < 3) {
        return [...prev, newScenario];
      }
      return [prev[0], prev[1], newScenario];
    });
    setCustomScenarioName('');
  }, [customScenarioName, depositAmount, lockDurationMonths, baseApyPercentage, compoundFrequency, enableTierMultiplier]);

  const handleRemoveScenario = useCallback((id: string) => {
    setScenarios((prev) => {
      if (prev.length <= 1) return prev;
      return prev.filter((s) => s.id !== id);
    });
  }, []);

  const handleResetScenarios = useCallback(() => {
    setScenarios(DEFAULT_STAKING_SCENARIOS);
    setBaselineIndex(0);
  }, []);

  const handleExportComparisonCSV = useCallback(() => {
    const csvContent = formatScenarioComparisonCSV(comparison);
    if (!csvContent) return;
    const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.setAttribute('href', url);
    link.setAttribute('download', `staking_scenario_comparison_${Date.now()}.csv`);
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    setExportedComparison(true);
    setTimeout(() => setExportedComparison(false), 2000);
  }, [comparison]);

  const presetAmounts = [500, 1000, 5000, 10000, 50000];
  const presetDurations = [
    { label: '1M', months: 1 },
    { label: '3M', months: 3 },
    { label: '6M', months: 6 },
    { label: '12M (1Y)', months: 12 },
    { label: '24M (2Y)', months: 24 },
    { label: '36M (3Y)', months: 36 },
  ];

  return (
    <div className={`rounded-2xl border border-slate-800 bg-slate-900/90 p-6 shadow-2xl backdrop-blur ${className}`}>
      {/* Widget Header */}
      <div className="mb-6 flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between border-b border-slate-800 pb-4">
        <div>
          <div className="flex items-center gap-2">
            <Calculator className="h-6 w-6 text-cyan-400" />
            <h2 className="text-xl font-bold text-slate-100">Token Yield & Staking Calculator</h2>
          </div>
          <p className="mt-1 text-sm text-slate-400">
            Estimate projected APY, compounding returns, and rewards prior to locking your tokens.
          </p>
        </div>
        <div className="flex flex-col items-end gap-2 sm:flex-row sm:items-center">
          {results.showStakeLockWarning && (
            <div className="inline-flex items-center gap-2 rounded-full border border-amber-500/30 bg-amber-950/40 px-3 py-1 text-xs font-semibold text-amber-400">
              <AlertTriangle className="h-3.5 w-3.5" />
              <span>Stake Lock Warning: Estimated returns below threshold</span>
            </div>
          )}
          <div className="inline-flex items-center gap-2 rounded-full border border-cyan-500/30 bg-cyan-950/40 px-3 py-1 text-xs font-semibold text-cyan-400">
            <Zap className="h-3.5 w-3.5" />
            <span>Soroban Staking</span>
          </div>
        </div>
      </div>

      <div className="grid gap-8 lg:grid-cols-12">
        {/* Left Column: Interactive Input Controls */}
        <div className="space-y-6 lg:col-span-7">
          {/* Deposit Amount Input */}
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm font-medium">
              <label htmlFor="deposit-amount-input" className="text-slate-300">
                Deposit Amount
              </label>
              <div className="flex items-center gap-1 font-mono text-cyan-400">
                <DollarSign className="h-4 w-4" />
                <input
                  id="deposit-amount-input"
                  type="number"
                  min="0"
                  max="1000000"
                  value={depositAmount}
                  onChange={(e) => setDepositAmount(Math.max(0, Number(e.target.value)))}
                  className="w-28 rounded border border-slate-700 bg-slate-950 px-2 py-0.5 text-right font-mono text-slate-100 focus:border-cyan-400 focus:outline-none"
                />
              </div>
            </div>
            <input
              type="range"
              min="10"
              max="100000"
              step="50"
              value={depositAmount}
              onChange={(e) => setDepositAmount(Number(e.target.value))}
              className="h-2 w-full cursor-pointer appearance-none rounded-lg bg-slate-800 accent-cyan-400"
            />
            {/* Quick Amount Presets */}
            <div className="flex flex-wrap gap-1.5 pt-1">
              {presetAmounts.map((amt) => (
                <button
                  key={amt}
                  type="button"
                  onClick={() => setDepositAmount(amt)}
                  className={`rounded px-2.5 py-1 text-xs font-medium transition ${
                    depositAmount === amt
                      ? 'bg-cyan-500 text-slate-950 font-semibold'
                      : 'bg-slate-800 text-slate-300 hover:bg-slate-700'
                  }`}
                >
                  {amt.toLocaleString()}
                </button>
              ))}
            </div>
          </div>

          {/* Lock Duration Slider & Presets */}
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm font-medium">
              <label htmlFor="lock-duration-slider" className="text-slate-300">
                Lock Duration: <span className="font-mono text-cyan-400">{lockDurationMonths} Months</span>
              </label>
              {enableTierMultiplier && (
                <span className="text-xs font-medium text-emerald-400">
                  {results.multiplier}x Duration Multiplier
                </span>
              )}
            </div>
            <input
              id="lock-duration-slider"
              type="range"
              min="1"
              max="36"
              step="1"
              value={lockDurationMonths}
              onChange={(e) => {
                // Keep stepping uniform (1 month) across the full 1-36 range,
                // including above 24 months: round + clamp so the slider can
                // never land on fractional or out-of-range values.
                const next = Math.round(Number(e.target.value));
                setLockDurationMonths(Math.min(36, Math.max(1, next)));
              }}
              aria-valuemin={1}
              aria-valuemax={36}
              aria-valuenow={lockDurationMonths}
              aria-valuetext={`${lockDurationMonths} months`}
              className="h-2 w-full cursor-pointer appearance-none rounded-lg bg-slate-800 accent-cyan-400"
            />
            {/* Duration Presets */}
            <div className="flex flex-wrap gap-1.5 pt-1">
              {presetDurations.map((item) => (
                <button
                  key={item.months}
                  type="button"
                  onClick={() => setLockDurationMonths(item.months)}
                  className={`rounded px-2.5 py-1 text-xs font-medium transition ${
                    lockDurationMonths === item.months
                      ? 'bg-cyan-500 text-slate-950 font-semibold'
                      : 'bg-slate-800 text-slate-300 hover:bg-slate-700'
                  }`}
                >
                  {item.label}
                </button>
              ))}
            </div>
          </div>

          {/* Base APY (%) Slider */}
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm font-medium">
              <label htmlFor="base-apy-input" className="text-slate-300">
                Base APY (%)
              </label>
              <div className="flex items-center gap-1 font-mono text-cyan-400">
                <input
                  id="base-apy-input"
                  type="number"
                  min="0"
                  max="100"
                  step="0.1"
                  value={baseApyPercentage}
                  onChange={(e) => setBaseApyPercentage(Math.max(0, Number(e.target.value)))}
                  className="w-20 rounded border border-slate-700 bg-slate-950 px-2 py-0.5 text-right font-mono text-slate-100 focus:border-cyan-400 focus:outline-none"
                />
                <span>%</span>
              </div>
            </div>
            <input
              type="range"
              min="1"
              max="50"
              step="0.5"
              value={baseApyPercentage}
              onChange={(e) => setBaseApyPercentage(Number(e.target.value))}
              className="h-2 w-full cursor-pointer appearance-none rounded-lg bg-slate-800 accent-cyan-400"
            />
          </div>

          {/* Compound Frequency Select Pills */}
          <div className="space-y-2">
            <label className="block text-sm font-medium text-slate-300">Compound Frequency</label>
            <div className="grid grid-cols-3 gap-2 sm:grid-cols-4 md:grid-cols-7">
              {[
                { id: 'continuous', label: 'Continuous' },
                { id: 'daily', label: 'Daily' },
                { id: 'weekly', label: 'Weekly' },
                { id: 'monthly', label: 'Monthly' },
                { id: 'quarterly', label: 'Quarterly' },
                { id: 'annually', label: 'Annual' },
                { id: 'none', label: 'Simple' },
              ].map((freq) => (
                <button
                  key={freq.id}
                  type="button"
                  onClick={() => setCompoundFrequency(freq.id)}
                  className={`rounded-lg border px-2 py-2 text-center text-xs font-medium transition ${
                    compoundFrequency === freq.id
                      ? 'border-cyan-500 bg-cyan-950/60 text-cyan-300'
                      : 'border-slate-800 bg-slate-950/40 text-slate-400 hover:border-slate-700 hover:text-slate-200'
                  }`}
                >
                  {freq.label}
                </button>
              ))}
            </div>
          </div>

          {/* Lock Duration Tier Multiplier Toggle */}
          <div className="flex items-center justify-between rounded-xl border border-slate-800 bg-slate-950/50 p-3">
            <div className="flex items-center gap-2">
              <TrendingUp className="h-4 w-4 text-emerald-400" />
              <div>
                <span className="text-xs font-semibold text-slate-200">Lock Duration Tier Multiplier</span>
                <p className="text-[11px] text-slate-400">Bonus APY for locking tokens for longer periods</p>
              </div>
            </div>
            <button
              type="button"
              onClick={() => setEnableTierMultiplier(!enableTierMultiplier)}
              className={`relative inline-flex h-5 w-10 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none ${
                enableTierMultiplier ? 'bg-cyan-500' : 'bg-slate-700'
              }`}
            >
              <span
                className={`pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
                  enableTierMultiplier ? 'translate-x-5' : 'translate-x-0'
                }`}
              />
            </button>
          </div>

          {/* Inflation Adjustment (Real vs Nominal) Toggle & Controls */}
          <div className="space-y-3 rounded-xl border border-slate-800 bg-slate-950/50 p-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Percent className="h-4 w-4 text-amber-400" />
                <div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-semibold text-slate-200">Inflation Adjustment</span>
                    <span
                      className={`rounded px-1.5 py-0.5 text-[10px] font-semibold ${
                        enableInflationAdjustment
                          ? 'bg-amber-950/80 text-amber-400 border border-amber-800/50'
                          : 'bg-slate-800 text-slate-400'
                      }`}
                    >
                      {enableInflationAdjustment ? 'Real Returns' : 'Nominal Returns'}
                    </span>
                  </div>
                  <p className="text-[11px] text-slate-400">
                    Discount future yields by expected inflation to measure real purchasing power
                  </p>
                </div>
              </div>
              <button
                type="button"
                onClick={() => setEnableInflationAdjustment(!enableInflationAdjustment)}
                id="toggle-inflation-btn"
                className={`relative inline-flex h-5 w-10 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none ${
                  enableInflationAdjustment ? 'bg-amber-500' : 'bg-slate-700'
                }`}
              >
                <span
                  className={`pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
                    enableInflationAdjustment ? 'translate-x-5' : 'translate-x-0'
                  }`}
                />
              </button>
            </div>

            {enableInflationAdjustment && (
              <div className="border-t border-slate-800/80 pt-3 space-y-2">
                <div className="flex items-center justify-between text-xs">
                  <label htmlFor="annual-inflation-input" className="text-slate-300 font-medium">
                    Annual Inflation Rate (%)
                  </label>
                  <div className="flex items-center gap-1 font-mono text-amber-400">
                    <input
                      id="annual-inflation-input"
                      type="number"
                      min="0"
                      max="30"
                      step="0.1"
                      value={annualInflationRate}
                      onChange={(e) => setAnnualInflationRate(Math.max(0, Number(e.target.value)))}
                      className="w-16 rounded border border-slate-700 bg-slate-950 px-2 py-0.5 text-right font-mono text-slate-100 focus:border-amber-400 focus:outline-none"
                    />
                    <span>%</span>
                  </div>
                </div>
                <input
                  type="range"
                  min="0"
                  max="15"
                  step="0.25"
                  value={annualInflationRate}
                  onChange={(e) => setAnnualInflationRate(Number(e.target.value))}
                  className="h-2 w-full cursor-pointer appearance-none rounded-lg bg-slate-800 accent-amber-400"
                />
                <div className="flex flex-wrap gap-1 pt-1">
                  {[
                    { label: '2.0% (Target)', rate: 2.0 },
                    { label: '3.0% (Standard)', rate: 3.0 },
                    { label: '5.0% (High)', rate: 5.0 },
                    { label: '8.0% (Elevated)', rate: 8.0 },
                  ].map((preset) => (
                    <button
                      key={preset.rate}
                      type="button"
                      onClick={() => setAnnualInflationRate(preset.rate)}
                      className={`rounded px-2 py-0.5 text-[11px] font-medium transition ${
                        annualInflationRate === preset.rate
                          ? 'bg-amber-500/20 text-amber-300 border border-amber-500/40'
                          : 'bg-slate-900 text-slate-400 hover:text-slate-200 border border-slate-800'
                      }`}
                    >
                      {preset.label}
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Right Column: Projected Results & Summary Cards */}
        <div className="flex flex-col justify-between space-y-4 lg:col-span-5">
          {/* Main Projection Cards */}
          <div className="grid gap-3">
            {enableInflationAdjustment && results.isNegativeRealReturn && (
              <div className="rounded-xl border border-rose-500/30 bg-rose-950/40 p-3 text-xs text-rose-300 flex items-start gap-2">
                <AlertTriangle className="h-4 w-4 shrink-0 text-rose-400 mt-0.5" />
                <div>
                  <span className="font-semibold text-rose-200">Negative Real Return Warning: </span>
                  Annual inflation ({annualInflationRate}%) exceeds effective staking APY ({results.nominalEffectiveApyPercent}%).
                  Your real purchasing power will erode by {Math.abs(results.effectiveApyPercent)}% per year.
                </div>
              </div>
            )}

            {enableInflationAdjustment && !results.isNegativeRealReturn && (
              <div className="rounded-xl border border-amber-500/30 bg-amber-950/40 p-2.5 text-xs text-amber-300 flex items-center justify-between">
                <span>Displaying <strong>Real Returns</strong> (Purchasing Power)</span>
                <span className="font-mono text-amber-400">Inflation: {annualInflationRate}%/yr</span>
              </div>
            )}

            {/* Projected Total Balance */}
            <div className="rounded-xl border border-slate-800 bg-gradient-to-br from-slate-900 to-slate-950 p-4">
              <div className="flex items-center justify-between text-xs font-medium text-slate-400">
                <span>{enableInflationAdjustment ? 'Projected Real Balance (Purchasing Power)' : 'Projected Total Balance'}</span>
                <Calendar className="h-4 w-4 text-cyan-400" />
              </div>
              <div className="mt-2 flex items-baseline gap-2">
                <span className="text-3xl font-bold font-mono text-cyan-300">
                  {results.totalBalance.toLocaleString()}
                </span>
                <span className="text-xs font-semibold text-slate-400">Tokens</span>
              </div>
              <div className="mt-2 text-xs text-slate-400 flex items-center justify-between">
                <span>Initial: <span className="font-mono text-slate-300">{results.depositAmount.toLocaleString()}</span></span>
                {enableInflationAdjustment && (
                  <span className="text-slate-400 text-[11px]">
                    Nominal: <span className="font-mono text-slate-300">{results.nominalTotalBalance.toLocaleString()}</span>
                  </span>
                )}
              </div>
            </div>

            {/* Total Staking Rewards */}
            <div className="rounded-xl border border-slate-800 bg-gradient-to-br from-slate-900 to-slate-950 p-4">
              <div className="flex items-center justify-between text-xs font-medium text-slate-400">
                <span>{enableInflationAdjustment ? 'Real Staking Rewards' : 'Total Staking Rewards'}</span>
                <Zap className="h-4 w-4 text-emerald-400" />
              </div>
              <div className="mt-2 flex items-baseline gap-2">
                <span
                  className={`text-2xl font-bold font-mono ${
                    results.totalInterest >= 0 ? 'text-emerald-400' : 'text-rose-400'
                  }`}
                >
                  {results.totalInterest >= 0 ? '+' : ''}
                  {results.totalInterest.toLocaleString()}
                </span>
                <span className="text-xs font-semibold text-slate-400">Tokens</span>
              </div>
              <div className="mt-2 flex items-center justify-between text-xs text-slate-400">
                <span>Total ROI:</span>
                <span
                  className={`font-mono font-semibold ${
                    results.totalRoiPercent >= 0 ? 'text-emerald-400' : 'text-rose-400'
                  }`}
                >
                  {results.totalRoiPercent >= 0 ? '+' : ''}
                  {results.totalRoiPercent}%
                </span>
              </div>
              {enableInflationAdjustment && (
                <div className="mt-1 flex items-center justify-between text-[11px] text-slate-500 border-t border-slate-800/60 pt-1">
                  <span>Nominal Rewards:</span>
                  <span className="font-mono text-slate-400">
                    +{results.nominalTotalInterest.toLocaleString()} (+{results.nominalTotalRoiPercent}%)
                  </span>
                </div>
              )}
            </div>

            {/* Effective APY Card */}
            <div className="grid grid-cols-2 gap-3">
              <div className="rounded-xl border border-slate-800 bg-slate-950/60 p-3">
                <span className="text-[11px] font-medium text-slate-400">
                  {enableInflationAdjustment ? 'Real APY' : 'Effective APY'}
                </span>
                <div
                  className={`mt-1 text-lg font-bold font-mono ${
                    results.effectiveApyPercent >= 0 ? 'text-cyan-400' : 'text-rose-400'
                  }`}
                >
                  {results.effectiveApyPercent}%
                </div>
                <div className="flex flex-col text-[10px] text-slate-500">
                  <span>
                    {enableInflationAdjustment
                      ? `Nominal: ${results.nominalEffectiveApyPercent}%`
                      : `Base: ${results.baseApyPercentage}%`}
                  </span>
                  {compoundFrequency !== 'none' && results.compoundedApyPercent !== undefined && (
                    <span className="text-cyan-400/80 font-mono">
                      Compounded: {results.compoundedApyPercent}%
                    </span>
                  )}
                </div>
              </div>
              <div className="rounded-xl border border-slate-800 bg-slate-950/60 p-3">
                <span className="text-[11px] font-medium text-slate-400">
                  {enableInflationAdjustment ? 'Real Monthly Yield' : 'Monthly Reward'}
                </span>
                <div
                  className={`mt-1 text-lg font-bold font-mono ${
                    results.estimatedMonthlyYield >= 0 ? 'text-emerald-400' : 'text-rose-400'
                  }`}
                >
                  {results.estimatedMonthlyYield >= 0 ? '+' : ''}
                  {results.estimatedMonthlyYield.toLocaleString()}
                </div>
                <span className="text-[10px] text-slate-500">~{results.estimatedDailyYield} / day</span>
              </div>
            </div>
          </div>

          {/* Toggle Projection Schedule Table & Quick Export */}
          <div className="space-y-2 pt-2">
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => setShowTable(!showTable)}
                id="toggle-milestone-schedule-btn"
                className="flex flex-1 items-center justify-center gap-2 rounded-xl border border-slate-800 bg-slate-950 py-2.5 text-xs font-semibold text-slate-300 hover:bg-slate-800 transition"
              >
                <BarChart2 className="h-4 w-4 text-cyan-400" />
                <span>{showTable ? 'Hide Monthly Milestone Schedule' : 'View Monthly Milestone Schedule'}</span>
              </button>
              <button
                type="button"
                onClick={exportToCSV}
                id="export-milestone-schedule-btn"
                title="Export full milestone schedule as CSV"
                className="flex items-center justify-center gap-1.5 rounded-xl border border-slate-800 bg-slate-950 px-3 py-2.5 text-xs font-semibold text-slate-300 hover:border-slate-700 hover:text-cyan-400 hover:bg-slate-800 transition"
              >
                {exported ? (
                  <>
                    <Check className="h-4 w-4 text-emerald-400" />
                    <span className="hidden sm:inline text-emerald-400">Exported!</span>
                  </>
                ) : (
                  <>
                    <Download className="h-4 w-4 text-cyan-400" />
                    <span className="hidden sm:inline">Export CSV</span>
                  </>
                )}
              </button>
            </div>

            <button
              type="button"
              onClick={() => setShowComparison(!showComparison)}
              id="toggle-scenario-comparison-btn"
              className={`flex w-full items-center justify-center gap-2 rounded-xl border py-2.5 text-xs font-semibold transition ${
                showComparison
                  ? 'border-cyan-500/40 bg-cyan-950/30 text-cyan-300'
                  : 'border-slate-800 bg-slate-950 text-slate-300 hover:bg-slate-800'
              }`}
            >
              <Scale className="h-4 w-4 text-cyan-400" />
              <span>{showComparison ? 'Hide Scenario Comparison' : 'Compare Staking Scenarios (Up to 3)'}</span>
              <span className="rounded-full bg-slate-800 px-2 py-0.5 text-[10px] text-cyan-400 font-mono">
                {scenarios.length}/3
              </span>
            </button>
          </div>
        </div>
      </div>

      {/* Compound Interest Growth Chart */}
      <div className="mt-6 border-t border-slate-800 pt-5">
        <CompoundInterestChart result={results} />
      </div>

      {/* Monthly Milestone Table */}
      {showTable && (
        <div className="mt-6 border-t border-slate-800 pt-4">
          <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
            <div>
              <h3 className="text-sm font-semibold text-slate-200">
                Monthly Compounding Schedule ({results.lockDurationMonths} Months)
                {enableInflationAdjustment && (
                  <span className="ml-2 rounded bg-amber-950/70 px-2 py-0.5 text-[11px] font-mono text-amber-300 border border-amber-800/40">
                    Real ({annualInflationRate}% Inflation)
                  </span>
                )}
              </h3>
              {enableInflationAdjustment && (
                <p className="text-[11px] text-slate-400 mt-0.5">
                  Values reflect discounted purchasing power eroded by expected inflation over time.
                </p>
              )}
            </div>
            <button
              type="button"
              onClick={exportToCSV}
              id="export-milestone-csv-btn"
              title="Export milestone schedule as CSV"
              className="flex items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-1 text-xs font-semibold text-slate-300 hover:border-slate-700 hover:text-cyan-400 hover:bg-slate-900 transition"
            >
              {exported ? (
                <>
                  <Check className="h-3.5 w-3.5 text-emerald-400" />
                  <span className="text-emerald-400">Exported!</span>
                </>
              ) : (
                <>
                  <Download className="h-3.5 w-3.5 text-cyan-400" />
                  <span>Export CSV</span>
                </>
              )}
            </button>
          </div>
          <div className="max-h-60 overflow-y-auto rounded-xl border border-slate-800 bg-slate-950">
            <table className="w-full text-left text-xs font-mono">
              <thead className="sticky top-0 bg-slate-900 text-slate-400">
                <tr>
                  <th className="px-4 py-2">Month</th>
                  <th className="px-4 py-2">
                    {enableInflationAdjustment ? 'Real Cumulative Yield' : 'Cumulative Yield'}
                  </th>
                  <th className="px-4 py-2 text-right">
                    {enableInflationAdjustment ? 'Projected Real Balance' : 'Projected Balance'}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/50 text-slate-300">
                {results.breakdownByMonth.map((row) => (
                  <tr key={row.month} className="hover:bg-slate-900/50">
                    <td className="px-4 py-2">Month {row.month}</td>
                    <td
                      className={`px-4 py-2 ${
                        row.yieldEarned >= 0 ? 'text-emerald-400' : 'text-rose-400'
                      }`}
                    >
                      {row.yieldEarned >= 0 ? '+' : ''}
                      {row.yieldEarned.toLocaleString()}
                    </td>
                    <td className="px-4 py-2 text-right font-bold text-slate-100">
                      {row.balance.toLocaleString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Scenario Comparison Section */}
      <div className="mt-8 border-t border-slate-800 pt-6">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-center gap-2">
            <Scale className="h-5 w-5 text-cyan-400" />
            <div>
              <h3 className="text-base font-bold text-slate-100 flex items-center gap-2">
                <span>Staking Scenario Comparison</span>
                <span className="rounded-full border border-cyan-500/30 bg-cyan-950/60 px-2 py-0.5 text-[11px] font-mono font-medium text-cyan-300">
                  {scenarios.length}/3 Configurations
                </span>
              </h3>
              <p className="text-xs text-slate-400">
                Compare up to 3 lock durations, deposit tiers, and compounding frequencies side-by-side.
              </p>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={handleExportComparisonCSV}
              id="export-scenarios-csv-btn"
              title="Export scenario comparison matrix as CSV"
              className="flex items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-1.5 text-xs font-semibold text-slate-300 hover:border-slate-700 hover:text-cyan-400 hover:bg-slate-900 transition"
            >
              {exportedComparison ? (
                <>
                  <Check className="h-3.5 w-3.5 text-emerald-400" />
                  <span className="text-emerald-400">Exported!</span>
                </>
              ) : (
                <>
                  <Download className="h-3.5 w-3.5 text-cyan-400" />
                  <span>Export CSV</span>
                </>
              )}
            </button>
            <button
              type="button"
              onClick={handleResetScenarios}
              id="reset-scenarios-btn"
              title="Reset to default scenarios"
              className="flex items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-1.5 text-xs font-semibold text-slate-400 hover:text-slate-200 hover:bg-slate-900 transition"
            >
              <RefreshCw className="h-3.5 w-3.5" />
              <span>Reset</span>
            </button>
            <button
              type="button"
              onClick={() => setShowComparison(!showComparison)}
              id="toggle-scenarios-panel-btn"
              className="flex items-center gap-1.5 rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-1.5 text-xs font-semibold text-slate-300 hover:bg-slate-900 transition"
            >
              <span>{showComparison ? 'Hide' : 'Show'}</span>
            </button>
          </div>
        </div>

        {/* Add Current Config as Scenario Bar */}
        {showComparison && (
          <div className="mt-4 flex flex-col sm:flex-row items-stretch sm:items-center gap-2 rounded-xl border border-slate-800/80 bg-slate-950/60 p-3">
            <input
              type="text"
              placeholder="Custom label for current setup (e.g. My High Yield Plan)..."
              value={customScenarioName}
              onChange={(e) => setCustomScenarioName(e.target.value)}
              className="flex-1 rounded-lg border border-slate-800 bg-slate-900 px-3 py-1.5 text-xs text-slate-100 placeholder:text-slate-500 focus:border-cyan-400 focus:outline-none"
            />
            <button
              type="button"
              onClick={handleSaveCurrentAsScenario}
              id="save-scenario-btn"
              className="flex items-center justify-center gap-1.5 rounded-lg bg-cyan-500 hover:bg-cyan-400 px-3.5 py-1.5 text-xs font-semibold text-slate-950 transition"
            >
              <Plus className="h-4 w-4" />
              <span>{scenarios.length >= 3 ? 'Replace 3rd with Current' : 'Add Current as Scenario'}</span>
            </button>
          </div>
        )}

        {/* Comparison Cards Grid */}
        {showComparison && (
          <div className="mt-4 grid gap-4 grid-cols-1 md:grid-cols-3">
            {comparison.scenarios.map((item, idx) => {
              const isBaseline = idx === baselineIndex;
              return (
                <div
                  key={item.id}
                  className={`relative flex flex-col justify-between rounded-xl border p-4 transition ${
                    isBaseline
                      ? 'border-cyan-500/50 bg-slate-900/90 shadow-lg shadow-cyan-950/20'
                      : 'border-slate-800 bg-slate-950/70 hover:border-slate-700'
                  }`}
                >
                  <div>
                    {/* Header: Title + Winner Badges */}
                    <div className="flex items-start justify-between gap-2 border-b border-slate-800/70 pb-3">
                      <div>
                        <div className="flex items-center gap-1.5">
                          <span className="font-mono text-xs text-slate-500">#{idx + 1}</span>
                          <h4 className="text-sm font-bold text-slate-100">{item.name}</h4>
                        </div>
                        <div className="mt-1 flex flex-wrap gap-1">
                          {isBaseline && (
                            <span className="rounded bg-cyan-950 px-1.5 py-0.5 text-[10px] font-semibold text-cyan-400 border border-cyan-800/50">
                              Baseline
                            </span>
                          )}
                          {item.isHighestYield && (
                            <span className="inline-flex items-center gap-1 rounded bg-emerald-950/80 px-1.5 py-0.5 text-[10px] font-semibold text-emerald-400 border border-emerald-800/50">
                              <Trophy className="h-3 w-3" />
                              Top Yield
                            </span>
                          )}
                          {item.isHighestApy && (
                            <span className="inline-flex items-center gap-1 rounded bg-amber-950/80 px-1.5 py-0.5 text-[10px] font-semibold text-amber-400 border border-amber-800/50">
                              <Zap className="h-3 w-3" />
                              Top APY
                            </span>
                          )}
                          {item.isHighestRoi && (
                            <span className="inline-flex items-center gap-1 rounded bg-purple-950/80 px-1.5 py-0.5 text-[10px] font-semibold text-purple-300 border border-purple-800/50">
                              <TrendingUp className="h-3 w-3" />
                              Top ROI
                            </span>
                          )}
                        </div>
                      </div>

                      {scenarios.length > 1 && (
                        <button
                          type="button"
                          onClick={() => handleRemoveScenario(item.id)}
                          title="Remove scenario"
                          id={`remove-scenario-${idx}-btn`}
                          className="rounded p-1 text-slate-500 hover:bg-slate-800 hover:text-red-400 transition"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      )}
                    </div>

                    {/* Inputs Specification */}
                    <div className="my-3 space-y-1.5 rounded-lg border border-slate-800/60 bg-slate-900/50 p-2.5 text-xs">
                      <div className="flex justify-between text-slate-400">
                        <span>Deposit:</span>
                        <span className="font-mono font-medium text-slate-200">
                          ${item.config.depositAmount.toLocaleString()}
                        </span>
                      </div>
                      <div className="flex justify-between text-slate-400">
                        <span>Duration:</span>
                        <span className="font-mono font-medium text-cyan-400">
                          {item.config.lockDurationMonths} Months ({item.result.multiplier}x)
                        </span>
                      </div>
                      <div className="flex justify-between text-slate-400">
                        <span>Base APY:</span>
                        <span className="font-mono font-medium text-slate-200">
                          {item.config.baseApyPercentage}%
                        </span>
                      </div>
                      <div className="flex justify-between text-slate-400">
                        <span>Compounding:</span>
                        <span className="capitalize font-mono font-medium text-slate-200">
                          {item.config.compoundFrequency}
                        </span>
                      </div>
                    </div>

                    {/* Projected Outcomes */}
                    <div className="space-y-2 border-t border-slate-800/70 pt-3 text-xs">
                      <div className="flex items-baseline justify-between">
                        <span className="text-slate-400">Total Balance:</span>
                        <span className="text-base font-bold font-mono text-cyan-300">
                          {item.result.totalBalance.toLocaleString()}
                        </span>
                      </div>

                      <div className="flex items-baseline justify-between">
                        <span className="text-slate-400">Total Rewards:</span>
                        <span className="font-mono font-bold text-emerald-400">
                          +{item.result.totalInterest.toLocaleString()}
                        </span>
                      </div>

                      <div className="flex items-center justify-between">
                        <span className="text-slate-400">Effective APY:</span>
                        <span className="font-mono font-semibold text-amber-400">
                          {item.result.effectiveApyPercent}%
                        </span>
                      </div>

                      <div className="flex items-center justify-between">
                        <span className="text-slate-400">Total ROI:</span>
                        <span className="font-mono font-semibold text-purple-300">
                          +{item.result.totalRoiPercent}%
                        </span>
                      </div>

                      {/* Variance vs Baseline */}
                      {!isBaseline && (
                        <div className="mt-2 rounded-lg border border-slate-800 bg-slate-900/60 p-2 text-[11px]">
                          <span className="text-slate-500">vs #{baselineIndex + 1} Baseline:</span>
                          <div className="mt-1 flex items-center justify-between font-mono">
                            <span className="text-slate-400">Rewards Delta:</span>
                            <span
                              className={`font-semibold ${
                                item.diffFromBaseline.interestDelta >= 0
                                  ? 'text-emerald-400'
                                  : 'text-rose-400'
                              }`}
                            >
                              {item.diffFromBaseline.interestDelta >= 0 ? '+' : ''}
                              {item.diffFromBaseline.interestDelta.toLocaleString()}
                            </span>
                          </div>
                          <div className="flex items-center justify-between font-mono">
                            <span className="text-slate-400">APY Delta:</span>
                            <span
                              className={`font-semibold ${
                                item.diffFromBaseline.apyDelta >= 0
                                  ? 'text-cyan-400'
                                  : 'text-rose-400'
                              }`}
                            >
                              {item.diffFromBaseline.apyDelta >= 0 ? '+' : ''}
                              {item.diffFromBaseline.apyDelta}%
                            </span>
                          </div>
                        </div>
                      )}
                    </div>
                  </div>

                  {/* Card Actions */}
                  <div className="mt-4 flex items-center gap-2 border-t border-slate-800/70 pt-3">
                    <button
                      type="button"
                      onClick={() => handleApplyScenario(item.config)}
                      id={`apply-scenario-${idx}-btn`}
                      className="flex flex-1 items-center justify-center gap-1.5 rounded-lg border border-slate-700 bg-slate-900 py-1.5 text-xs font-semibold text-slate-200 hover:border-cyan-500 hover:text-cyan-300 transition"
                    >
                      <ArrowRight className="h-3.5 w-3.5" />
                      <span>Apply</span>
                    </button>
                    {!isBaseline && (
                      <button
                        type="button"
                        onClick={() => setBaselineIndex(idx)}
                        id={`set-baseline-${idx}-btn`}
                        className="rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-400 hover:text-slate-200 hover:bg-slate-900 transition"
                      >
                        Set Baseline
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
