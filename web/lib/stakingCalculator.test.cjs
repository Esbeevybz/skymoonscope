const test = require('node:test');
const assert = require('node:assert/strict');

const {
  calculateStakingYield,
  getDurationTierMultiplier,
  COMPOUND_FREQUENCIES,
  formatMilestoneScheduleCSV,
  exportMilestoneScheduleCSV,
  compareStakingScenarios,
  formatScenarioComparisonCSV,
  DEFAULT_STAKING_SCENARIOS,
  computeCompoundYieldRate,
  calculateCompoundedApy,
  getCompoundFrequencyPeriods,
} = require('./stakingCalculator');

test('calculateStakingYield: zero deposit returns 0 interest and 0 ROI', () => {
  const result = calculateStakingYield({ depositAmount: 0 });
  assert.equal(result.totalBalance, 0);
  assert.equal(result.totalInterest, 0);
  assert.equal(result.totalRoiPercent, 0);
});

test('calculateStakingYield: 0% APY returns deposit amount as total balance', () => {
  const result = calculateStakingYield({ depositAmount: 1000, baseApyPercentage: 0 });
  assert.equal(result.totalBalance, 1000);
  assert.equal(result.totalInterest, 0);
});

test('calculateStakingYield: simple interest (none compounding)', () => {
  const result = calculateStakingYield({
    depositAmount: 1000,
    lockDurationMonths: 12,
    baseApyPercentage: 10,
    compoundFrequency: 'none',
    enableTierMultiplier: false,
  });

  // A = 1000 * (1 + 0.10 * 1) = 1100
  assert.equal(result.totalBalance, 1100);
  assert.equal(result.totalInterest, 100);
  assert.equal(result.totalRoiPercent, 10);
});

test('calculateStakingYield: compound monthly interest', () => {
  const result = calculateStakingYield({
    depositAmount: 1000,
    lockDurationMonths: 12,
    baseApyPercentage: 12,
    compoundFrequency: 'monthly',
    enableTierMultiplier: false,
  });

  // A = 1000 * (1 + 0.12 / 12) ^ 12 = 1000 * (1.01)^12 ≈ 1126.83
  assert.equal(result.totalBalance, 1126.83);
  assert.equal(result.totalInterest, 126.83);
  assert.equal(result.totalRoiPercent, 12.68);
  assert.equal(result.breakdownByMonth.length, 12);
});

test('getDurationTierMultiplier: duration scaling thresholds', () => {
  assert.equal(getDurationTierMultiplier(1), 1.0);
  assert.equal(getDurationTierMultiplier(3), 1.1);
  assert.equal(getDurationTierMultiplier(6), 1.25);
  assert.equal(getDurationTierMultiplier(12), 1.5);
  assert.equal(getDurationTierMultiplier(24), 1.75);
  assert.equal(getDurationTierMultiplier(36), 2.0);
});

test('calculateStakingYield: tier multiplier increases effective APY', () => {
  const withMultiplier = calculateStakingYield({
    depositAmount: 1000,
    lockDurationMonths: 12,
    baseApyPercentage: 10,
    enableTierMultiplier: true,
  });

  // For 12 months, tier multiplier is 1.5x, so effective APY = 15%
  assert.equal(withMultiplier.multiplier, 1.5);
  assert.equal(withMultiplier.effectiveApyPercent, 15);
});

test('calculateStakingYield: compound frequencies affect total interest appropriately', () => {
  const daily = calculateStakingYield({ depositAmount: 1000, compoundFrequency: 'daily', enableTierMultiplier: false });
  const monthly = calculateStakingYield({ depositAmount: 1000, compoundFrequency: 'monthly', enableTierMultiplier: false });
  const annually = calculateStakingYield({ depositAmount: 1000, compoundFrequency: 'annually', enableTierMultiplier: false });
  const simple = calculateStakingYield({ depositAmount: 1000, compoundFrequency: 'none', enableTierMultiplier: false });

  // Higher frequency compounding yields strictly higher or equal returns: Daily >= Monthly >= Annually >= Simple
  assert.ok(daily.totalInterest >= monthly.totalInterest);
  assert.ok(monthly.totalInterest >= annually.totalInterest);
  assert.ok(annually.totalInterest >= simple.totalInterest);
});

test('calculateStakingYield: handles negative inputs and boundary clamping', () => {
  const negativeDeposit = calculateStakingYield({ depositAmount: -500, lockDurationMonths: -5 });
  assert.equal(negativeDeposit.depositAmount, 0);
  assert.equal(negativeDeposit.lockDurationMonths, 1);
  assert.equal(negativeDeposit.totalBalance, 0);
});

test('getDurationTierMultiplier: interpolates smoothly between 24 and 36 months (slider stepping fix)', () => {
  // Regression test for: slider stepping inconsistency above 24 months.
  // Previously 25-36 all returned flat 2.0 (a +0.25 jump at 24->25, then zero
  // change per slider step). Now every 1-month step adds 0.25/12.
  const step = 0.25 / 12;
  assert.ok(Math.abs(getDurationTierMultiplier(25) - (1.75 + step)) < 1e-12);
  assert.ok(Math.abs(getDurationTierMultiplier(30) - 1.875) < 1e-12);
  assert.ok(Math.abs(getDurationTierMultiplier(36) - 2.0) < 1e-12);

  // Uniform per-month delta across the whole 24-36 slider range
  for (let m = 25; m <= 36; m++) {
    const delta = getDurationTierMultiplier(m) - getDurationTierMultiplier(m - 1);
    assert.ok(Math.abs(delta - step) < 1e-12, `month ${m - 1}->${m} delta ${delta} !== ${step}`);
  }

  // Clamps at 2.0 beyond the 36-month slider max
  assert.equal(getDurationTierMultiplier(37), 2.0);
  assert.equal(getDurationTierMultiplier(100), 2.0);
});

test('calculateStakingYield: every slider step above 24 months increases rewards', () => {
  let prevInterest = -Infinity;
  for (let m = 24; m <= 36; m++) {
    const r = calculateStakingYield({
      depositAmount: 1000,
      lockDurationMonths: m,
      baseApyPercentage: 10,
      compoundFrequency: 'monthly',
      enableTierMultiplier: true,
    });
    assert.ok(r.totalInterest > prevInterest, `month ${m} interest ${r.totalInterest} should exceed ${prevInterest}`);
    prevInterest = r.totalInterest;
  }
});

test('calculateStakingYield: monthly breakdown matches final balance', () => {
  const result = calculateStakingYield({
    depositAmount: 2000,
    lockDurationMonths: 24,
    baseApyPercentage: 8,
    compoundFrequency: 'monthly',
    enableTierMultiplier: true,
  });

  assert.equal(result.breakdownByMonth.length, 24);
  const lastMonth = result.breakdownByMonth[result.breakdownByMonth.length - 1];
  assert.equal(lastMonth.month, 24);
  assert.equal(lastMonth.balance, result.totalBalance);
  assert.equal(lastMonth.yieldEarned, result.totalInterest);
});

test('formatMilestoneScheduleCSV: formats milestone table correctly with headers and rows', () => {
  const result = calculateStakingYield({
    depositAmount: 1000,
    lockDurationMonths: 3,
    baseApyPercentage: 12,
    compoundFrequency: 'monthly',
    enableTierMultiplier: false,
  });

  const csv = formatMilestoneScheduleCSV(result);
  const lines = csv.split('\n');

  assert.equal(lines.length, 4); // 1 header + 3 month rows
  assert.equal(lines[0], '"Month","Cumulative Yield","Projected Balance"');
  assert.ok(lines[1].startsWith('"Month 1",'));
  assert.ok(lines[2].startsWith('"Month 2",'));
  assert.ok(lines[3].startsWith('"Month 3",'));

  // Third month balance should match result.totalBalance
  assert.ok(lines[3].includes(`"${result.totalBalance.toFixed(2)}"`));
  assert.ok(lines[3].includes(`"${result.totalInterest.toFixed(2)}"`));
});

test('formatMilestoneScheduleCSV: accepts breakdownByMonth array directly', () => {
  const mockBreakdown = [
    { month: 1, yieldEarned: 10.5, balance: 1010.5 },
    { month: 2, yieldEarned: 21.25, balance: 1021.25 },
  ];

  const csv = formatMilestoneScheduleCSV(mockBreakdown);
  const lines = csv.split('\n');

  assert.equal(lines.length, 3);
  assert.equal(lines[0], '"Month","Cumulative Yield","Projected Balance"');
  assert.equal(lines[1], '"Month 1","10.50","1010.50"');
  assert.equal(lines[2], '"Month 2","21.25","1021.25"');
});

test('formatMilestoneScheduleCSV: supports numeric month format option', () => {
  const mockBreakdown = [
    { month: 1, yieldEarned: 5, balance: 1005 },
    { month: 2, yieldEarned: 10, balance: 1010 },
  ];

  const csv = formatMilestoneScheduleCSV(mockBreakdown, { monthFormat: 'number' });
  const lines = csv.split('\n');

  assert.equal(lines[1], '"1","5.00","1005.00"');
  assert.equal(lines[2], '"2","10.00","1010.00"');
});

test('formatMilestoneScheduleCSV: escapes quotes and commas properly', () => {
  const customBreakdown = [
    { month: '1, "special"', yieldEarned: '10,00', balance: '1,010.00' },
  ];

  const csv = formatMilestoneScheduleCSV(customBreakdown);
  const lines = csv.split('\n');

  assert.equal(lines[1], '"1, ""special""","10,00","1,010.00"');
});

test('formatMilestoneScheduleCSV: handles empty or invalid inputs gracefully', () => {
  assert.equal(formatMilestoneScheduleCSV([]), '');
  assert.equal(formatMilestoneScheduleCSV(null), '');
  assert.equal(formatMilestoneScheduleCSV(undefined), '');
  assert.equal(formatMilestoneScheduleCSV({}), '');
  assert.equal(formatMilestoneScheduleCSV({ breakdownByMonth: [] }), '');
});

test('exportMilestoneScheduleCSV: returns formatted CSV string in non-browser environment', () => {
  const result = calculateStakingYield({ depositAmount: 1000, lockDurationMonths: 2 });
  const output = exportMilestoneScheduleCSV(result);

  assert.equal(typeof output, 'string');
  assert.ok(output.includes('"Month","Cumulative Yield","Projected Balance"'));
  assert.ok(output.includes('"Month 1"'));
  assert.ok(output.includes('"Month 2"'));
});

test('exportMilestoneScheduleCSV: returns false for empty input in non-browser environment', () => {
  const output = exportMilestoneScheduleCSV([]);
  assert.equal(output, false);
});

test('compareStakingScenarios: compares up to 3 configurations side-by-side and ranks winners', () => {
  const scenarios = [
    { id: 'sc-1', name: 'Low Lock', depositAmount: 1000, lockDurationMonths: 3, baseApyPercentage: 8 },
    { id: 'sc-2', name: 'Mid Lock', depositAmount: 1000, lockDurationMonths: 12, baseApyPercentage: 12 },
    { id: 'sc-3', name: 'Max Lock', depositAmount: 1000, lockDurationMonths: 36, baseApyPercentage: 15 },
  ];

  const comparison = compareStakingScenarios(scenarios);

  assert.equal(comparison.scenarios.length, 3);
  assert.equal(comparison.bestYieldId, 'sc-3');
  assert.equal(comparison.bestApyId, 'sc-3');
  assert.equal(comparison.bestRoiId, 'sc-3');

  // Check winner flags
  assert.equal(comparison.scenarios[0].isHighestYield, false);
  assert.equal(comparison.scenarios[1].isHighestYield, false);
  assert.equal(comparison.scenarios[2].isHighestYield, true);

  // Baseline is scenario 0 (Low Lock)
  assert.equal(comparison.scenarios[0].diffFromBaseline.interestDelta, 0);
  assert.ok(comparison.scenarios[1].diffFromBaseline.interestDelta > 0);
  assert.ok(comparison.scenarios[2].diffFromBaseline.interestDelta > comparison.scenarios[1].diffFromBaseline.interestDelta);
});

test('compareStakingScenarios: enforces maximum 3 scenarios cap', () => {
  const scenarios = [
    { id: 's1', depositAmount: 1000 },
    { id: 's2', depositAmount: 2000 },
    { id: 's3', depositAmount: 3000 },
    { id: 's4', depositAmount: 4000 },
    { id: 's5', depositAmount: 5000 },
  ];

  const comparison = compareStakingScenarios(scenarios);
  assert.equal(comparison.scenarios.length, 3);
  assert.equal(comparison.scenarios[0].id, 's1');
  assert.equal(comparison.scenarios[1].id, 's2');
  assert.equal(comparison.scenarios[2].id, 's3');
});

test('compareStakingScenarios: handles empty or invalid inputs', () => {
  const empty = compareStakingScenarios([]);
  assert.equal(empty.scenarios.length, 0);
  assert.equal(empty.bestYieldId, null);

  const nullInput = compareStakingScenarios(null);
  assert.equal(nullInput.scenarios.length, 0);
});

test('DEFAULT_STAKING_SCENARIOS: contains 3 valid preset scenarios', () => {
  assert.equal(DEFAULT_STAKING_SCENARIOS.length, 3);
  const comparison = compareStakingScenarios(DEFAULT_STAKING_SCENARIOS);
  assert.equal(comparison.scenarios.length, 3);
  assert.ok(comparison.bestYieldId !== null);
});

test('formatScenarioComparisonCSV: outputs valid CSV matrix comparing metrics across scenarios', () => {
  const comparison = compareStakingScenarios(DEFAULT_STAKING_SCENARIOS);
  const csv = formatScenarioComparisonCSV(comparison);
  const lines = csv.trim().split('\n');

  assert.ok(lines.length >= 10);
  assert.equal(
    lines[0],
    '"Metric","Conservative (3M)","Balanced (12M)","Aggressive (36M)"'
  );

  // Verify key metric rows exist
  assert.ok(lines.some((line) => line.startsWith('"Effective APY (%)"')));
  assert.ok(lines.some((line) => line.startsWith('"Projected Total Balance"')));
  assert.ok(lines.some((line) => line.startsWith('"Total Staking Rewards"')));
  assert.ok(lines.some((line) => line.startsWith('"Lock Duration"')));
});

test('calculateStakingYield: inflation adjustment discounts real balance and real APY via Fisher relation', () => {
  const deposit = 1000;
  const nominal = calculateStakingYield({
    depositAmount: deposit,
    lockDurationMonths: 12,
    baseApyPercentage: 12,
    compoundFrequency: 'monthly',
    enableTierMultiplier: true, // 1.5x -> 18% effective APY
    enableInflationAdjustment: false,
  });

  const real = calculateStakingYield({
    depositAmount: deposit,
    lockDurationMonths: 12,
    baseApyPercentage: 12,
    compoundFrequency: 'monthly',
    enableTierMultiplier: true, // 1.5x -> 18% effective APY
    enableInflationAdjustment: true,
    annualInflationRate: 3.0,
  });

  // Nominal balance should match between runs
  assert.equal(real.nominalTotalBalance, nominal.totalBalance);
  assert.equal(real.nominalEffectiveApyPercent, 18);

  // Real balance discounted by (1 + 0.03)^1 = 1.03
  const expectedRealBalance = Number((nominal.totalBalance / 1.03).toFixed(2));
  assert.equal(real.totalBalance, expectedRealBalance);
  assert.ok(real.totalBalance < nominal.totalBalance);

  // Real APY: ((1 + 0.18) / 1.03 - 1) * 100 ≈ 14.56%
  const expectedRealApy = Number((((1.18 / 1.03) - 1) * 100).toFixed(2));
  assert.equal(real.effectiveApyPercent, expectedRealApy);
  assert.equal(real.isNegativeRealReturn, false);
});

test('calculateStakingYield: negative real return flagged when inflation rate exceeds APY', () => {
  const result = calculateStakingYield({
    depositAmount: 1000,
    lockDurationMonths: 12,
    baseApyPercentage: 5,
    compoundFrequency: 'none',
    enableTierMultiplier: false, // 5% APY
    enableInflationAdjustment: true,
    annualInflationRate: 8.0, // 8% inflation > 5% APY
  });

  assert.equal(result.isNegativeRealReturn, true);
  assert.ok(result.effectiveApyPercent < 0);
  assert.ok(result.totalBalance < 1000);
  assert.ok(result.totalInterest < 0);
});

test('calculateStakingYield: 0% inflation produces identical real and nominal values', () => {
  const result = calculateStakingYield({
    depositAmount: 1000,
    lockDurationMonths: 12,
    baseApyPercentage: 10,
    enableInflationAdjustment: true,
    annualInflationRate: 0,
  });

  assert.equal(result.totalBalance, result.nominalTotalBalance);
  assert.equal(result.totalInterest, result.nominalTotalInterest);
  assert.equal(result.effectiveApyPercent, result.nominalEffectiveApyPercent);
  assert.equal(result.isNegativeRealReturn, false);
});

test('calculateStakingYield: monthly breakdown contains both real and nominal data points', () => {
  const result = calculateStakingYield({
    depositAmount: 1000,
    lockDurationMonths: 6,
    baseApyPercentage: 12,
    compoundFrequency: 'monthly',
    enableTierMultiplier: false,
    enableInflationAdjustment: true,
    annualInflationRate: 4.0,
  });

  assert.equal(result.breakdownByMonth.length, 6);
  const m6 = result.breakdownByMonth[5];

  assert.ok(m6.nominalBalance !== undefined);
  assert.ok(m6.realBalance !== undefined);
  assert.ok(m6.nominalBalance > m6.realBalance);
  assert.equal(m6.balance, m6.realBalance); // Active balance reflects real when inflation adjustment is on
});

test('computeCompoundYieldRate: strictly monotonic increase across ascending compound frequencies', () => {
  const r = 0.12; // 12% APR
  const t = 1; // 1 year

  const simple = computeCompoundYieldRate(r, 0, t);
  const annually = computeCompoundYieldRate(r, 1, t);
  const quarterly = computeCompoundYieldRate(r, 4, t);
  const monthly = computeCompoundYieldRate(r, 12, t);
  const weekly = computeCompoundYieldRate(r, 52, t);
  const daily = computeCompoundYieldRate(r, 365, t);
  const hourly = computeCompoundYieldRate(r, 8760, t);
  const minutely = computeCompoundYieldRate(r, 525600, t);
  const secondly = computeCompoundYieldRate(r, 31536000, t);
  const continuous = computeCompoundYieldRate(r, Infinity, t);

  // Exact math checks
  assert.equal(simple, 0.12);
  assert.equal(annually, 0.12);
  assert.ok(quarterly > annually);
  assert.ok(monthly > quarterly);
  assert.ok(weekly > monthly);
  assert.ok(daily > weekly);
  assert.ok(hourly > daily);
  assert.ok(minutely > hourly);
  assert.ok(secondly >= minutely);
  assert.ok(continuous >= secondly);

  // Continuous limit equals expm1(r * t)
  assert.equal(continuous, Math.expm1(r * t));

  // The difference between secondly and continuous is tiny but continuous is the upper bound
  assert.ok(Math.abs(continuous - secondly) < 1e-7);
});

test('computeCompoundYieldRate: handles micro-rates without catastrophic cancellation', () => {
  const microRate = 1e-6; // 0.0001%
  const t = 2;
  const continuous = computeCompoundYieldRate(microRate, Infinity, t);
  const hourly = computeCompoundYieldRate(microRate, 8760, t);

  assert.ok(continuous > 0);
  assert.ok(hourly > 0);
  assert.ok(continuous >= hourly);
  assert.ok(!Number.isNaN(continuous));
  assert.ok(!Number.isNaN(hourly));
});

test('calculateCompoundedApy: returns high-precision compounded APY percentages', () => {
  const apr = 12;

  // Simple / annual
  assert.equal(calculateCompoundedApy(apr, 'none'), 12);
  assert.equal(calculateCompoundedApy(apr, 'annually'), 12);

  // Monthly: (1 + 0.12/12)^12 - 1 = 12.6825%
  assert.equal(calculateCompoundedApy(apr, 'monthly'), 12.6825);

  // Daily: (1 + 0.12/365)^365 - 1 = 12.7475%
  assert.equal(calculateCompoundedApy(apr, 'daily'), 12.7475);

  // Hourly: (1 + 0.12/8760)^8760 - 1 = 12.7496%
  assert.equal(calculateCompoundedApy(apr, 'hourly'), 12.7496);

  // Continuous: exp(0.12) - 1 = 12.7497%
  assert.equal(calculateCompoundedApy(apr, 'continuous'), 12.7497);

  // Monotonic check
  assert.ok(
    calculateCompoundedApy(apr, 'continuous') >= calculateCompoundedApy(apr, 'daily')
  );
  assert.ok(
    calculateCompoundedApy(apr, 'daily') > calculateCompoundedApy(apr, 'monthly')
  );
});

test('calculateStakingYield: final milestone row strictly synchronizes with totalBalance and totalInterest for high frequency', () => {
  const deposit = 10000;
  const months = 18;
  const frequencies = ['daily', 'hourly', 'secondly', 'continuous'];

  for (const freq of frequencies) {
    const result = calculateStakingYield({
      depositAmount: deposit,
      lockDurationMonths: months,
      baseApyPercentage: 14,
      compoundFrequency: freq,
      enableTierMultiplier: true,
    });

    assert.equal(result.breakdownByMonth.length, months);
    const lastRow = result.breakdownByMonth[months - 1];

    // Final month milestone must exactly match total balance and total interest with zero 1-cent drift
    assert.equal(lastRow.nominalBalance, result.nominalTotalBalance);
    assert.equal(lastRow.nominalYieldEarned, result.nominalTotalInterest);
    assert.equal(lastRow.balance, result.totalBalance);
    assert.equal(lastRow.yieldEarned, result.totalInterest);
    assert.ok(result.compoundedApyPercent > result.effectiveApyPercent);
  }
});


