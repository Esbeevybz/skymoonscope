/**
 * Staking & Yield Calculator Logic
 */

const COMPOUND_FREQUENCIES = {
  continuous: Infinity,
  secondly: 31536000,
  minutely: 525600,
  hourly: 8760,
  daily: 365,
  weekly: 52,
  monthly: 12,
  quarterly: 4,
  annually: 1,
  none: 0,
};

const DURATION_TIER_MULTIPLIERS = {
  1: 1.0,
  3: 1.1,
  6: 1.25,
  12: 1.5,
  24: 1.75,
  36: 2.0,
};

/**
 * Resolve compound frequency to periods per year.
 * Handles named string frequencies and direct numerical inputs.
 */
function getCompoundFrequencyPeriods(frequency) {
  if (typeof frequency === 'number') {
    return Math.max(0, frequency);
  }
  if (typeof frequency === 'string') {
    const key = frequency.toLowerCase().trim();
    if (key in COMPOUND_FREQUENCIES) {
      return COMPOUND_FREQUENCIES[key];
    }
  }
  return 12;
}

/**
 * High-precision compound interest growth rate minus 1: (1 + r / n)^(n * t) - 1.
 *
 * Uses Math.expm1 and Math.log1p to eliminate IEEE 754 floating-point rounding
 * errors and catastrophic cancellation that occur at high compounding frequencies
 * (e.g. daily, hourly, by-second, and continuous compounding).
 *
 * Traditional Math.pow(1 + r / n, n * t) loses precision when r / n << 1 because
 * adding 1 truncates the lower mantissa bits. Math.expm1(n * t * Math.log1p(r / n))
 * preserves full 53-bit floating-point accuracy across all compounding frequencies.
 *
 * @param {number} r - Annual rate as decimal (e.g. 0.12)
 * @param {number} n - Compound periods per year (0 for simple, Infinity for continuous)
 * @param {number} t - Time in years
 * @returns {number} Growth rate minus 1
 */
function computeCompoundYieldRate(r, n, t) {
  if (r === 0 || t === 0) return 0;
  if (n === 0) return r * t; // Simple interest
  if (!isFinite(n) || n >= 1e7) {
    // Continuous compounding limit: lim n->inf (1 + r/n)^(nt) - 1 = expm1(r * t)
    return Math.expm1(r * t);
  }
  // High-precision discrete compounding: expm1(n * t * log1p(r / n))
  return Math.expm1(n * t * Math.log1p(r / n));
}

/**
 * Calculate the Effective Annual Percentage Yield (compounded APY) from nominal APR
 * with high precision even at high compound frequencies.
 *
 * APY = ((1 + r / n)^n - 1) * 100
 *
 * @param {number} nominalAprPercent - Nominal rate percentage (e.g. 12 for 12%)
 * @param {string | number} compoundFrequency - Compound frequency key or periods per year
 * @returns {number} Compounded APY percentage
 */
function calculateCompoundedApy(nominalAprPercent, compoundFrequency = 'annually') {
  const rate = Math.max(0, Number(nominalAprPercent) || 0) / 100;
  if (rate === 0) return 0;
  const n = getCompoundFrequencyPeriods(compoundFrequency);
  const apyRate = computeCompoundYieldRate(rate, n, 1);
  return Number((apyRate * 100).toFixed(4));
}

/**
 * Get tier multiplier based on lock duration in months.
 * Interpolates or clamps appropriately.
 */
function getDurationTierMultiplier(months) {
  if (months <= 1) return 1.0;
  if (months <= 3) return 1.0 + (months - 1) * (0.1 / 2);
  if (months <= 6) return 1.1 + (months - 3) * (0.15 / 3);
  if (months <= 12) return 1.25 + (months - 6) * (0.25 / 6);
  if (months <= 24) return 1.5 + (months - 12) * (0.25 / 12);
  if (months <= 36) return 1.75 + (months - 24) * (0.25 / 12);
  return 2.0;
}

/**
 * Calculate Staking Yield & APY
 */
function calculateStakingYield({
  depositAmount = 1000,
  lockDurationMonths = 12,
  compoundFrequency = 'monthly',
  baseApyPercentage = 12,
  enableTierMultiplier = true,
  stakeLockWarningThreshold = null,
  enableInflationAdjustment = false,
  annualInflationRate = 3.0,
} = {}) {
  const P = Math.max(0, Number(depositAmount) || 0);
  const months = Math.max(1, Number(lockDurationMonths) || 1);
  const t = months / 12; // Time in years
  const baseApy = Math.max(0, Number(baseApyPercentage) || 0);

  const multiplier = enableTierMultiplier ? getDurationTierMultiplier(months) : 1.0;
  const effectiveApyPercent = baseApy * multiplier;
  const r = effectiveApyPercent / 100; // annual rate as decimal

  const n = getCompoundFrequencyPeriods(compoundFrequency);

  let totalBalance = P;
  let totalInterest = 0;
  if (P === 0 || r === 0) {
    totalBalance = P;
    totalInterest = 0;
  } else {
    const yieldRate = computeCompoundYieldRate(r, n, t);
    totalInterest = P * yieldRate;
    totalBalance = P + totalInterest;
  }
  const totalRoiPercent = P > 0 ? (totalInterest / P) * 100 : 0;

  const totalDays = t * 365;
  const estimatedDailyYield = totalDays > 0 ? totalInterest / totalDays : 0;
  const estimatedMonthlyYield = months > 0 ? totalInterest / months : 0;

  // Inflation adjustment (Fisher relation & real purchasing power discounting)
  const inflationRate = Math.max(0, Number(annualInflationRate) || 0);
  const inflationDecimal = inflationRate / 100;

  // Real purchasing power: Real Balance = Balance / (1 + i)^t
  // Compute using rounded nominal balance to avoid sub-cent drift between displayed nominal and real balances
  const roundedNominalBalance = Number(totalBalance.toFixed(2));
  const inflationDiscountFactor = Math.pow(1 + inflationDecimal, t);
  const realTotalBalance = inflationDiscountFactor > 0 ? roundedNominalBalance / inflationDiscountFactor : roundedNominalBalance;
  const realTotalInterest = realTotalBalance - P;
  const realTotalRoiPercent = P > 0 ? (realTotalInterest / P) * 100 : 0;

  // Real APY: ((1 + r_nominal) / (1 + i) - 1) * 100
  const realEffectiveApyPercent = inflationDecimal > -1
    ? (((1 + effectiveApyPercent / 100) / (1 + inflationDecimal)) - 1) * 100
    : effectiveApyPercent;

  const isNegativeRealReturn = realEffectiveApyPercent < 0;

  // Compounded APY calculation with numerical stability at all frequencies
  const nominalCompoundedApy = calculateCompoundedApy(effectiveApyPercent, compoundFrequency);
  const realCompoundedApy = inflationDecimal > -1
    ? (((1 + nominalCompoundedApy / 100) / (1 + inflationDecimal)) - 1) * 100
    : nominalCompoundedApy;
  const activeCompoundedApy = enableInflationAdjustment ? realCompoundedApy : nominalCompoundedApy;

  // Monthly milestone projections
  const breakdownByMonth = [];
  for (let m = 1; m <= months; m++) {
    const elapsedYears = m / 12;
    let balanceAtM = P;
    let yieldAtM = 0;
    if (P > 0 && r > 0) {
      if (m === months) {
        // Guarantee final milestone precisely synchronizes with overall totals
        yieldAtM = totalInterest;
        balanceAtM = totalBalance;
      } else {
        yieldAtM = P * computeCompoundYieldRate(r, n, elapsedYears);
        balanceAtM = P + yieldAtM;
      }
    }

    const roundedNominalBalanceAtM = Number(balanceAtM.toFixed(2));
    const roundedNominalYieldAtM = Number(yieldAtM.toFixed(2));

    const discountFactorAtM = Math.pow(1 + inflationDecimal, elapsedYears);
    const realBalanceAtM = discountFactorAtM > 0 ? roundedNominalBalanceAtM / discountFactorAtM : roundedNominalBalanceAtM;
    const realYieldAtM = realBalanceAtM - P;

    const activeBalanceAtM = enableInflationAdjustment ? realBalanceAtM : roundedNominalBalanceAtM;
    const activeYieldAtM = enableInflationAdjustment ? realYieldAtM : roundedNominalYieldAtM;

    breakdownByMonth.push({
      month: m,
      balance: Number(activeBalanceAtM.toFixed(2)),
      yieldEarned: Number(activeYieldAtM.toFixed(2)),
      nominalBalance: roundedNominalBalanceAtM,
      nominalYieldEarned: roundedNominalYieldAtM,
      realBalance: Number(realBalanceAtM.toFixed(2)),
      realYieldEarned: Number(realYieldAtM.toFixed(2)),
    });
  }

  const threshold = stakeLockWarningThreshold !== null && stakeLockWarningThreshold !== undefined
    ? Number(stakeLockWarningThreshold)
    : null;
  const showStakeLockWarning = threshold !== null && !Number.isNaN(threshold) && effectiveApyPercent < threshold;

  const activeBalance = enableInflationAdjustment ? realTotalBalance : totalBalance;
  const activeInterest = enableInflationAdjustment ? realTotalInterest : totalInterest;
  const activeRoi = enableInflationAdjustment ? realTotalRoiPercent : totalRoiPercent;
  const activeApy = enableInflationAdjustment ? realEffectiveApyPercent : effectiveApyPercent;
  const activeDailyYield = totalDays > 0 ? activeInterest / totalDays : 0;
  const activeMonthlyYield = months > 0 ? activeInterest / months : 0;

  return {
    depositAmount: P,
    lockDurationMonths: months,
    durationYears: Number(t.toFixed(4)),
    baseApyPercentage: baseApy,
    multiplier: Number(multiplier.toFixed(2)),
    effectiveApyPercent: Number(activeApy.toFixed(2)),
    compoundedApyPercent: Number(activeCompoundedApy.toFixed(4)),
    compoundFrequency,
    totalBalance: Number(activeBalance.toFixed(2)),
    totalInterest: Number(activeInterest.toFixed(2)),
    totalRoiPercent: Number(activeRoi.toFixed(2)),
    estimatedDailyYield: Number(activeDailyYield.toFixed(4)),
    estimatedMonthlyYield: Number(activeMonthlyYield.toFixed(2)),
    breakdownByMonth,
    stakeLockWarningThreshold: threshold,
    showStakeLockWarning,
    // Inflation-related fields
    enableInflationAdjustment: Boolean(enableInflationAdjustment),
    annualInflationRate: Number(inflationRate.toFixed(2)),
    nominalTotalBalance: Number(totalBalance.toFixed(2)),
    nominalTotalInterest: Number(totalInterest.toFixed(2)),
    nominalTotalRoiPercent: Number(totalRoiPercent.toFixed(2)),
    nominalEffectiveApyPercent: Number(effectiveApyPercent.toFixed(2)),
    nominalCompoundedApyPercent: Number(nominalCompoundedApy.toFixed(4)),
    realTotalBalance: Number(realTotalBalance.toFixed(2)),
    realTotalInterest: Number(realTotalInterest.toFixed(2)),
    realTotalRoiPercent: Number(realTotalRoiPercent.toFixed(2)),
    realEffectiveApyPercent: Number(realEffectiveApyPercent.toFixed(2)),
    realCompoundedApyPercent: Number(realCompoundedApy.toFixed(4)),
    isNegativeRealReturn,
  };
}

/**
 * Format milestone schedule projections as a standard RFC 4180 CSV string.
 *
 * @param {Array<{month: number, yieldEarned: number, balance: number}> | {breakdownByMonth: Array}} milestonesOrResult
 * @param {Object} [options]
 * @param {'label' | 'number'} [options.monthFormat]
 * @returns {string} CSV formatted text
 */
function formatMilestoneScheduleCSV(milestonesOrResult, options = {}) {
  const list = Array.isArray(milestonesOrResult)
    ? milestonesOrResult
    : (milestonesOrResult && milestonesOrResult.breakdownByMonth) || [];

  if (!list.length) return '';

  const headers = ['Month', 'Cumulative Yield', 'Projected Balance'];
  const escape = (val) => {
    const clean = String(val ?? '').replace(/"/g, '""');
    return `"${clean}"`;
  };

  const rows = list.map((row) => {
    let monthLabel;
    if (options.monthFormat === 'number') {
      monthLabel = typeof row.month === 'number'
        ? String(row.month)
        : String(row.month ?? '').replace(/^Month\s*/i, '');
    } else {
      monthLabel = typeof row.month === 'number' && !String(row.month).startsWith('Month')
        ? `Month ${row.month}`
        : String(row.month ?? '');
    }

    const yieldVal = typeof row.yieldEarned === 'number'
      ? row.yieldEarned.toFixed(2)
      : String(row.yieldEarned ?? '0');
    const balanceVal = typeof row.balance === 'number'
      ? row.balance.toFixed(2)
      : String(row.balance ?? '0');

    return [
      escape(monthLabel),
      escape(yieldVal),
      escape(balanceVal),
    ];
  });

  return [headers.map(escape).join(','), ...rows.map((row) => row.join(','))].join('\n');
}

/**
 * Trigger download of milestone schedule projections as CSV in browser environment.
 * In non-browser environments, returns the CSV string.
 *
 * @param {Array | Object} milestonesOrResult
 * @param {string} [filename]
 * @param {Object} [options]
 * @returns {boolean | string}
 */
function exportMilestoneScheduleCSV(milestonesOrResult, filename, options) {
  const csvContent = formatMilestoneScheduleCSV(milestonesOrResult, options);
  if (!csvContent) return false;

  if (typeof window === 'undefined' || typeof document === 'undefined') {
    return csvContent;
  }

  const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.setAttribute('href', url);
  link.setAttribute('download', filename || `milestone_schedule_${Date.now()}.csv`);
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
  return true;
}

/**
 * Default preset scenarios for 3-way staking comparison.
 */
const DEFAULT_STAKING_SCENARIOS = [
  {
    id: 'scenario-1',
    name: 'Conservative (3M)',
    depositAmount: 1000,
    lockDurationMonths: 3,
    baseApyPercentage: 8,
    compoundFrequency: 'monthly',
    enableTierMultiplier: true,
  },
  {
    id: 'scenario-2',
    name: 'Balanced (12M)',
    depositAmount: 1000,
    lockDurationMonths: 12,
    baseApyPercentage: 12,
    compoundFrequency: 'monthly',
    enableTierMultiplier: true,
  },
  {
    id: 'scenario-3',
    name: 'Aggressive (36M)',
    depositAmount: 1000,
    lockDurationMonths: 36,
    baseApyPercentage: 15,
    compoundFrequency: 'daily',
    enableTierMultiplier: true,
  },
];

/**
 * Compare up to 3 staking configurations side-by-side.
 *
 * @param {Array<Object>} scenarios - Array of up to 3 scenario configs
 * @param {Object} [options]
 * @param {number} [options.baselineIndex=0] - Index of scenario to treat as baseline for deltas
 * @returns {Object} Comparison result with results, rankings, and deltas
 */
function compareStakingScenarios(scenarios, options = {}) {
  if (!Array.isArray(scenarios) || scenarios.length === 0) {
    return {
      scenarios: [],
      bestYieldId: null,
      bestApyId: null,
      bestRoiId: null,
      bestBalanceId: null,
    };
  }

  // Enforce maximum 3 scenarios
  const cappedScenarios = scenarios.slice(0, 3);
  const baselineIndex = Math.min(
    cappedScenarios.length - 1,
    Math.max(0, Number(options.baselineIndex) || 0)
  );

  const evaluated = cappedScenarios.map((sc, idx) => {
    const id = sc.id || `scenario-${idx + 1}`;
    const name = sc.name || `Scenario ${idx + 1}`;
    const depositAmount = sc.depositAmount !== undefined ? sc.depositAmount : 1000;
    const lockDurationMonths = sc.lockDurationMonths !== undefined ? sc.lockDurationMonths : 12;
    const baseApyPercentage = sc.baseApyPercentage !== undefined ? sc.baseApyPercentage : 12;
    const compoundFrequency = sc.compoundFrequency || 'monthly';
    const enableTierMultiplier = sc.enableTierMultiplier !== undefined ? Boolean(sc.enableTierMultiplier) : true;
    const enableInflationAdjustment = sc.enableInflationAdjustment !== undefined
      ? Boolean(sc.enableInflationAdjustment)
      : Boolean(options.enableInflationAdjustment);
    const annualInflationRate = sc.annualInflationRate !== undefined
      ? sc.annualInflationRate
      : (options.annualInflationRate !== undefined ? options.annualInflationRate : 3.0);

    const result = calculateStakingYield({
      depositAmount,
      lockDurationMonths,
      baseApyPercentage,
      compoundFrequency,
      enableTierMultiplier,
      enableInflationAdjustment,
      annualInflationRate,
    });

    return {
      id,
      name,
      config: {
        id,
        name,
        depositAmount: result.depositAmount,
        lockDurationMonths: result.lockDurationMonths,
        baseApyPercentage: result.baseApyPercentage,
        compoundFrequency: result.compoundFrequency,
        enableTierMultiplier,
        enableInflationAdjustment,
        annualInflationRate: result.annualInflationRate,
      },
      result,
    };
  });

  // Determine top performers
  let maxYield = -Infinity;
  let maxApy = -Infinity;
  let maxRoi = -Infinity;
  let maxBalance = -Infinity;
  let bestYieldId = null;
  let bestApyId = null;
  let bestRoiId = null;
  let bestBalanceId = null;

  for (const item of evaluated) {
    if (item.result.totalInterest > maxYield) {
      maxYield = item.result.totalInterest;
      bestYieldId = item.id;
    }
    if (item.result.effectiveApyPercent > maxApy) {
      maxApy = item.result.effectiveApyPercent;
      bestApyId = item.id;
    }
    if (item.result.totalRoiPercent > maxRoi) {
      maxRoi = item.result.totalRoiPercent;
      bestRoiId = item.id;
    }
    if (item.result.totalBalance > maxBalance) {
      maxBalance = item.result.totalBalance;
      bestBalanceId = item.id;
    }
  }

  const baselineResult = evaluated[baselineIndex].result;

  const enrichedScenarios = evaluated.map((item) => {
    const isHighestYield = item.id === bestYieldId;
    const isHighestApy = item.id === bestApyId;
    const isHighestRoi = item.id === bestRoiId;
    const isHighestBalance = item.id === bestBalanceId;

    const interestDelta = Number((item.result.totalInterest - baselineResult.totalInterest).toFixed(2));
    const balanceDelta = Number((item.result.totalBalance - baselineResult.totalBalance).toFixed(2));
    const apyDelta = Number((item.result.effectiveApyPercent - baselineResult.effectiveApyPercent).toFixed(2));
    const roiDelta = Number((item.result.totalRoiPercent - baselineResult.totalRoiPercent).toFixed(2));

    return {
      id: item.id,
      name: item.name,
      config: item.config,
      result: item.result,
      isHighestYield,
      isHighestApy,
      isHighestRoi,
      isHighestBalance,
      diffFromBaseline: {
        interestDelta,
        balanceDelta,
        apyDelta,
        roiDelta,
      },
    };
  });

  return {
    scenarios: enrichedScenarios,
    bestYieldId,
    bestApyId,
    bestRoiId,
    bestBalanceId,
  };
}

/**
 * Format scenario comparison matrix as CSV.
 *
 * @param {Object | Array} comparisonOrScenarios
 * @returns {string} CSV formatted comparison table
 */
function formatScenarioComparisonCSV(comparisonOrScenarios) {
  const comparison = comparisonOrScenarios && comparisonOrScenarios.scenarios
    ? comparisonOrScenarios
    : compareStakingScenarios(comparisonOrScenarios || []);

  if (!comparison.scenarios.length) return '';

  const headers = [
    'Metric',
    ...comparison.scenarios.map((s) => s.name),
  ];

  const escape = (val) => {
    const clean = String(val ?? '').replace(/"/g, '""');
    return `"${clean}"`;
  };

  const rows = [
    ['Deposit Amount', ...comparison.scenarios.map((s) => String(s.config.depositAmount))],
    ['Lock Duration', ...comparison.scenarios.map((s) => `${s.config.lockDurationMonths} Months`)],
    ['Base APY (%)', ...comparison.scenarios.map((s) => `${s.config.baseApyPercentage}%`)],
    ['Tier Multiplier', ...comparison.scenarios.map((s) => `${s.result.multiplier}x`)],
    ['Compound Frequency', ...comparison.scenarios.map((s) => s.config.compoundFrequency)],
    ['Effective APY (%)', ...comparison.scenarios.map((s) => `${s.result.effectiveApyPercent}%`)],
    ['Projected Total Balance', ...comparison.scenarios.map((s) => s.result.totalBalance.toFixed(2))],
    ['Total Staking Rewards', ...comparison.scenarios.map((s) => s.result.totalInterest.toFixed(2))],
    ['Total ROI (%)', ...comparison.scenarios.map((s) => `${s.result.totalRoiPercent}%`)],
    ['Monthly Yield', ...comparison.scenarios.map((s) => s.result.estimatedMonthlyYield.toFixed(2))],
    ['Daily Yield', ...comparison.scenarios.map((s) => s.result.estimatedDailyYield.toFixed(4))],
  ];

  return [headers.map(escape).join(','), ...rows.map((row) => row.map(escape).join(','))].join('\n');
}

module.exports = {
  calculateStakingYield,
  getDurationTierMultiplier,
  COMPOUND_FREQUENCIES,
  DURATION_TIER_MULTIPLIERS,
  formatMilestoneScheduleCSV,
  exportMilestoneScheduleCSV,
  compareStakingScenarios,
  formatScenarioComparisonCSV,
  DEFAULT_STAKING_SCENARIOS,
  computeCompoundYieldRate,
  calculateCompoundedApy,
  getCompoundFrequencyPeriods,
};
