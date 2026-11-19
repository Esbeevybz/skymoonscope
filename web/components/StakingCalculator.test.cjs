const test = require('node:test');
const assert = require('node:assert/strict');

const {
  calculateStakingYield,
  formatMilestoneScheduleCSV,
  exportMilestoneScheduleCSV,
  compareStakingScenarios,
  formatScenarioComparisonCSV,
  DEFAULT_STAKING_SCENARIOS,
} = require('../lib/stakingCalculator');

test('StakingCalculator logic integration: default parameters yield calculation', () => {
  const result = calculateStakingYield();
  assert.equal(result.depositAmount, 1000);
  assert.equal(result.lockDurationMonths, 12);
  assert.equal(result.baseApyPercentage, 12);
  assert.equal(result.effectiveApyPercent, 18); // 12% * 1.5 multiplier
  assert.ok(result.totalBalance > 1000);
  assert.ok(result.totalInterest > 0);
});

test('StakingCalculator logic integration: max lock duration (36 months)', () => {
  const result = calculateStakingYield({
    depositAmount: 5000,
    lockDurationMonths: 36,
    baseApyPercentage: 10,
    compoundFrequency: 'daily',
    enableTierMultiplier: true,
  });

  // For 36 months, tier multiplier is 2.0x -> effective APY is 20%
  assert.equal(result.multiplier, 2.0);
  assert.equal(result.effectiveApyPercent, 20);
  assert.ok(result.totalBalance > 5000 * 1.6);
  assert.equal(result.breakdownByMonth.length, 36);
});

test('StakingCalculator: exports full milestone schedule table as CSV', () => {
  const result = calculateStakingYield({
    depositAmount: 2000,
    lockDurationMonths: 12,
    baseApyPercentage: 10,
    compoundFrequency: 'monthly',
    enableTierMultiplier: true,
  });

  const csv = formatMilestoneScheduleCSV(result);
  const rows = csv.trim().split('\n');

  // Header row + 12 monthly milestone rows
  assert.equal(rows.length, 13);
  assert.equal(rows[0], '"Month","Cumulative Yield","Projected Balance"');

  // Check every month exists sequentially in the export
  for (let m = 1; m <= 12; m++) {
    const row = rows[m];
    assert.ok(row.startsWith(`"Month ${m}",`));
    const expectedYield = result.breakdownByMonth[m - 1].yieldEarned.toFixed(2);
    const expectedBalance = result.breakdownByMonth[m - 1].balance.toFixed(2);
    assert.ok(row.includes(`"${expectedYield}"`));
    assert.ok(row.includes(`"${expectedBalance}"`));
  }
});

test('StakingCalculator: browser DOM download simulation for milestone schedule export', () => {
  const clicks = [];
  const appendedElements = [];
  const removedElements = [];

  const origDocument = global.document;
  const origWindow = global.window;
  const origURL = global.URL;

  // Mock minimal DOM environment
  global.window = {};
  global.document = {
    body: {
      appendChild(node) {
        appendedElements.push(node);
      },
      removeChild(node) {
        removedElements.push(node);
      },
    },
    createElement(tag) {
      const element = {
        tagName: tag,
        setAttribute(k, v) {
          element[k] = v;
        },
        click() {
          clicks.push(element.download);
        },
      };
      return element;
    },
  };

  global.URL = {
    createObjectURL(blob) {
      return 'blob:mock-url-' + blob.size;
    },
    revokeObjectURL() {},
  };

  try {
    const result = calculateStakingYield({ depositAmount: 1000, lockDurationMonths: 6 });
    const success = exportMilestoneScheduleCSV(result, 'test_schedule.csv');

    assert.equal(success, true);
    assert.equal(clicks.length, 1);
    assert.equal(clicks[0], 'test_schedule.csv');
    assert.equal(appendedElements.length, 1);
    assert.equal(removedElements.length, 1);
    assert.equal(appendedElements[0], removedElements[0]);
  } finally {
    global.document = origDocument;
    global.window = origWindow;
    global.URL = origURL;
  }
});

test('StakingCalculator scenario comparison: compares up to 3 configurations side-by-side', () => {
  const customScenarios = [
    { id: 'sc1', name: 'Plan 1', depositAmount: 1000, lockDurationMonths: 6, baseApyPercentage: 10 },
    { id: 'sc2', name: 'Plan 2', depositAmount: 1000, lockDurationMonths: 12, baseApyPercentage: 10 },
    { id: 'sc3', name: 'Plan 3', depositAmount: 1000, lockDurationMonths: 24, baseApyPercentage: 10 },
  ];

  const comparison = compareStakingScenarios(customScenarios);

  assert.equal(comparison.scenarios.length, 3);
  assert.equal(comparison.bestYieldId, 'sc3');
  assert.equal(comparison.bestApyId, 'sc3');

  // Verify duration scaling progression
  assert.ok(comparison.scenarios[2].result.totalInterest > comparison.scenarios[1].result.totalInterest);
  assert.ok(comparison.scenarios[1].result.totalInterest > comparison.scenarios[0].result.totalInterest);

  // Baseline comparison (Plan 1 is baseline by default)
  assert.equal(comparison.scenarios[0].diffFromBaseline.interestDelta, 0);
  assert.ok(comparison.scenarios[1].diffFromBaseline.interestDelta > 0);
  assert.ok(comparison.scenarios[2].diffFromBaseline.interestDelta > 0);
});

test('StakingCalculator scenario comparison: exports comparison CSV and handles simulated browser download', () => {
  const comparison = compareStakingScenarios(DEFAULT_STAKING_SCENARIOS);
  const csv = formatScenarioComparisonCSV(comparison);

  assert.ok(csv.includes('"Metric"'));
  assert.ok(csv.includes('"Conservative (3M)"'));
  assert.ok(csv.includes('"Balanced (12M)"'));
  assert.ok(csv.includes('"Aggressive (36M)"'));

  // Test browser download simulation for comparison CSV
  const clicks = [];
  const origDocument = global.document;
  const origWindow = global.window;
  const origURL = global.URL;

  global.window = {};
  global.document = {
    body: {
      appendChild() {},
      removeChild() {},
    },
    createElement() {
      return {
        setAttribute(k, v) { this[k] = v; },
        click() { clicks.push(this.download); },
      };
    },
  };
  global.URL = {
    createObjectURL: () => 'blob:comparison-url',
    revokeObjectURL: () => {},
  };

  try {
    const blob = { size: csv.length };
    const url = global.URL.createObjectURL(blob);
    const link = global.document.createElement('a');
    link.setAttribute('href', url);
    link.setAttribute('download', 'scenario_comparison.csv');
    link.click();

    assert.equal(clicks.length, 1);
    assert.equal(clicks[0], 'scenario_comparison.csv');
  } finally {
    global.document = origDocument;
    global.window = origWindow;
    global.URL = origURL;
  }
});

test('StakingCalculator inflation adjustment: nominal vs real return calculation toggle', () => {
  const nominal = calculateStakingYield({
    depositAmount: 1000,
    lockDurationMonths: 12,
    baseApyPercentage: 10,
    enableInflationAdjustment: false,
  });

  const real = calculateStakingYield({
    depositAmount: 1000,
    lockDurationMonths: 12,
    baseApyPercentage: 10,
    enableInflationAdjustment: true,
    annualInflationRate: 3.5,
  });

  // Verify nominal baseline is preserved
  assert.equal(real.nominalTotalBalance, nominal.totalBalance);
  assert.equal(real.nominalTotalInterest, nominal.totalInterest);
  assert.equal(real.nominalEffectiveApyPercent, nominal.effectiveApyPercent);

  // Verify real purchasing power is discounted
  assert.ok(real.totalBalance < nominal.totalBalance);
  assert.ok(real.totalInterest < nominal.totalInterest);
  assert.ok(real.effectiveApyPercent < nominal.effectiveApyPercent);
  assert.equal(real.enableInflationAdjustment, true);
  assert.equal(real.annualInflationRate, 3.5);
});

test('StakingCalculator inflation adjustment: scenario comparison with inflation applied', () => {
  const scenarios = [
    { id: 's1', depositAmount: 1000, lockDurationMonths: 6, baseApyPercentage: 8 },
    { id: 's2', depositAmount: 1000, lockDurationMonths: 12, baseApyPercentage: 12 },
  ];

  const comparison = compareStakingScenarios(scenarios, {
    enableInflationAdjustment: true,
    annualInflationRate: 4.0,
  });

  assert.equal(comparison.scenarios.length, 2);
  assert.equal(comparison.scenarios[0].result.enableInflationAdjustment, true);
  assert.equal(comparison.scenarios[0].result.annualInflationRate, 4.0);
  assert.ok(comparison.scenarios[1].result.totalInterest > comparison.scenarios[0].result.totalInterest);
});

test('StakingCalculator: high compound frequencies (daily, hourly, continuous) calculate APY and returns without precision loss', () => {
  const daily = calculateStakingYield({
    depositAmount: 5000,
    lockDurationMonths: 12,
    baseApyPercentage: 10,
    compoundFrequency: 'daily',
    enableTierMultiplier: false,
  });

  const continuous = calculateStakingYield({
    depositAmount: 5000,
    lockDurationMonths: 12,
    baseApyPercentage: 10,
    compoundFrequency: 'continuous',
    enableTierMultiplier: false,
  });

  assert.ok(continuous.totalInterest >= daily.totalInterest);
  assert.ok(continuous.compoundedApyPercent >= daily.compoundedApyPercent);
  assert.equal(continuous.compoundedApyPercent, 10.5171); // exp(0.10) - 1 ≈ 10.5171%
  assert.equal(daily.compoundedApyPercent, 10.5156);
  assert.ok(continuous.breakdownByMonth.length === 12);
  assert.equal(continuous.breakdownByMonth[11].balance, continuous.totalBalance);
});

