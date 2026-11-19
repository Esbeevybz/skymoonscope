/**
 * TypeScript type declarations for lib/stakingCalculator.js
 */

export interface StakingYieldInput {
  depositAmount?: number;
  lockDurationMonths?: number;
  compoundFrequency?: string;
  baseApyPercentage?: number;
  enableTierMultiplier?: boolean;
  stakeLockWarningThreshold?: number | null;
  enableInflationAdjustment?: boolean;
  annualInflationRate?: number;
}

export interface MonthlyBreakdownRow {
  /** 1-based month index. */
  month: number;
  /** Active balance at this month (real if inflation adjustment enabled, otherwise nominal). */
  balance: number;
  /** Active interest earned by this month (real if inflation adjustment enabled, otherwise nominal). */
  yieldEarned: number;
  /** Nominal balance at this month. */
  nominalBalance?: number;
  /** Nominal cumulative yield earned by this month. */
  nominalYieldEarned?: number;
  /** Inflation-discounted purchasing power balance at this month. */
  realBalance?: number;
  /** Inflation-discounted cumulative real yield at this month. */
  realYieldEarned?: number;
}

export interface StakingYieldResult {
  depositAmount: number;
  lockDurationMonths: number;
  durationYears: number;
  baseApyPercentage: number;
  /** Duration tier multiplier applied to base APY (1.0 when disabled). */
  multiplier: number;
  /** Effective APY (real if inflation adjusted, otherwise nominal baseApyPercentage × multiplier). */
  effectiveApyPercent: number;
  /** Effective Compounded APY accounting for compounding frequency with high numerical precision. */
  compoundedApyPercent: number;
  compoundFrequency: string;
  totalBalance: number;
  totalInterest: number;
  totalRoiPercent: number;
  estimatedDailyYield: number;
  estimatedMonthlyYield: number;
  breakdownByMonth: MonthlyBreakdownRow[];
  stakeLockWarningThreshold?: number | null;
  showStakeLockWarning?: boolean;
  // Inflation fields
  enableInflationAdjustment: boolean;
  annualInflationRate: number;
  nominalTotalBalance: number;
  nominalTotalInterest: number;
  nominalTotalRoiPercent: number;
  nominalEffectiveApyPercent: number;
  nominalCompoundedApyPercent?: number;
  realTotalBalance: number;
  realTotalInterest: number;
  realTotalRoiPercent: number;
  realEffectiveApyPercent: number;
  realCompoundedApyPercent?: number;
  isNegativeRealReturn: boolean;
}

export declare function calculateStakingYield(input?: StakingYieldInput): StakingYieldResult;
export declare function getDurationTierMultiplier(months: number): number;
export declare function getCompoundFrequencyPeriods(frequency: string | number): number;
export declare function computeCompoundYieldRate(r: number, n: number, t: number): number;
export declare function calculateCompoundedApy(nominalAprPercent: number, compoundFrequency?: string | number): number;
export declare const COMPOUND_FREQUENCIES: Record<string, number>;
export declare const DURATION_TIER_MULTIPLIERS: Record<number, number>;

export interface MilestoneScheduleCSVOptions {
  monthFormat?: 'label' | 'number';
}

export declare function formatMilestoneScheduleCSV(
  milestonesOrResult: MonthlyBreakdownRow[] | StakingYieldResult | { breakdownByMonth: MonthlyBreakdownRow[] },
  options?: MilestoneScheduleCSVOptions
): string;

export declare function exportMilestoneScheduleCSV(
  milestonesOrResult: MonthlyBreakdownRow[] | StakingYieldResult | { breakdownByMonth: MonthlyBreakdownRow[] },
  filename?: string,
  options?: MilestoneScheduleCSVOptions
): boolean | string;

export interface StakingScenario {
  id: string;
  name: string;
  depositAmount: number;
  lockDurationMonths: number;
  baseApyPercentage: number;
  compoundFrequency: string;
  enableTierMultiplier: boolean;
}

export interface ScenarioComparisonItem {
  id: string;
  name: string;
  config: StakingScenario;
  result: StakingYieldResult;
  isHighestYield: boolean;
  isHighestApy: boolean;
  isHighestRoi: boolean;
  isHighestBalance: boolean;
  diffFromBaseline: {
    interestDelta: number;
    balanceDelta: number;
    apyDelta: number;
    roiDelta: number;
  };
}

export interface ScenarioComparisonResult {
  scenarios: ScenarioComparisonItem[];
  bestYieldId: string | null;
  bestApyId: string | null;
  bestRoiId: string | null;
  bestBalanceId: string | null;
}

export interface ScenarioComparisonOptions {
  baselineIndex?: number;
}

export declare const DEFAULT_STAKING_SCENARIOS: StakingScenario[];

export declare function compareStakingScenarios(
  scenarios: Array<Partial<StakingScenario>>,
  options?: ScenarioComparisonOptions
): ScenarioComparisonResult;

export declare function formatScenarioComparisonCSV(
  comparisonOrScenarios: ScenarioComparisonResult | Array<Partial<StakingScenario>>
): string;
