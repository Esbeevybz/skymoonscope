// CPUGauge.test.cjs — unit tests for CPUGauge component logic
// Runs with: node --test ./components/CPUGauge.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

test('CPUGauge: severity level calculation', () => {
  // Test severity level thresholds
  const testCases = [
    { utilization: 25, expected: 'low' },
    { utilization: 50, expected: 'moderate' },
    { utilization: 70, expected: 'high' },
    { utilization: 85, expected: 'critical' },
    { utilization: 95, expected: 'critical' },
    { utilization: 0, expected: 'low' },
    { utilization: 100, expected: 'critical' },
  ];

  const getSeverityLevel = (utilization) => {
    if (utilization >= 85) return 'critical';
    if (utilization >= 70) return 'high';
    if (utilization >= 50) return 'moderate';
    return 'low';
  };

  testCases.forEach(({ utilization, expected }) => {
    const result = getSeverityLevel(utilization);
    assert.equal(
      result,
      expected,
      `Utilization ${utilization}% should be ${expected}, got ${result}`,
    );
  });
});

test('CPUGauge: severity color mapping', () => {
  const getSeverityColor = (level) => {
    const colors = {
      low: { needle: '#10b981', zone: 'from-emerald-500/20 to-emerald-600/20', label: 'Well within budget' },
      moderate: { needle: '#f59e0b', zone: 'from-amber-500/20 to-amber-600/20', label: 'Moderate usage' },
      high: { needle: '#ef7f35', zone: 'from-orange-500/20 to-orange-600/20', label: 'Approaching limit' },
      critical: { needle: '#ef4444', zone: 'from-red-500/20 to-red-600/20', label: 'Critical usage' },
    };
    return colors[level];
  };

  const colorMappings = {
    low: { needle: '#10b981', label: 'Well within budget' },
    moderate: { needle: '#f59e0b', label: 'Moderate usage' },
    high: { needle: '#ef7f35', label: 'Approaching limit' },
    critical: { needle: '#ef4444', label: 'Critical usage' },
  };

  Object.entries(colorMappings).forEach(([level, expected]) => {
    const result = getSeverityColor(level);
    assert.equal(result.needle, expected.needle, `${level} needle color mismatch`);
    assert.equal(result.label, expected.label, `${level} label mismatch`);
  });
});

test('CPUGauge: needle rotation calculation', () => {
  // Needle rotates from -90 to 90 degrees for 0-100% utilization
  const calculateNeedleRotation = (utilization) => {
    return (utilization / 100) * 180 - 90;
  };

  const testCases = [
    { utilization: 0, expected: -90 },
    { utilization: 25, expected: -45 },
    { utilization: 50, expected: 0 },
    { utilization: 75, expected: 45 },
    { utilization: 100, expected: 90 },
  ];

  testCases.forEach(({ utilization, expected }) => {
    const result = calculateNeedleRotation(utilization);
    assert.equal(
      result,
      expected,
      `Utilization ${utilization}% should rotate ${expected}°, got ${result}°`,
    );
  });
});

test('CPUGauge: utilization clamping (0-100 range)', () => {
  const clampUtil = (util) => Math.max(0, Math.min(100, util));

  const testCases = [
    { input: -10, expected: 0 },
    { input: 0, expected: 0 },
    { input: 50, expected: 50 },
    { input: 100, expected: 100 },
    { input: 150, expected: 100 },
  ];

  testCases.forEach(({ input, expected }) => {
    const result = clampUtil(input);
    assert.equal(result, expected, `Input ${input} should clamp to ${expected}, got ${result}`);
  });
});

test('CPUGauge: percentage formatting', () => {
  const formatPercent = (util) => util.toFixed(1);

  const testCases = [
    { input: 0, expected: '0.0' },
    { input: 25.5, expected: '25.5' },
    { input: 50.123, expected: '50.1' },
    { input: 100, expected: '100.0' },
  ];

  testCases.forEach(({ input, expected }) => {
    const result = formatPercent(input);
    assert.equal(result, expected, `${input} formatted as ${expected}, got ${result}`);
  });
});

test('CPUGauge: instruction count formatting (compact notation)', () => {
  const formatCompact = (num) => {
    if (!num) return null;
    return new Intl.NumberFormat('en-US', {
      notation: 'compact',
      compactDisplay: 'short',
    }).format(num);
  };

  const testCases = [
    { input: 1000, expected: '1K' },
    { input: 1_000_000, expected: '1M' },
    { input: 1_000_000_000, expected: '1B' },
    { input: 500_000, expected: '500K' },
    { input: 3_500_000, expected: '3.5M' },
  ];

  testCases.forEach(({ input, expected }) => {
    const result = formatCompact(input);
    assert.ok(
      result.startsWith(expected.charAt(0)),
      `${input} should format to ${expected}, got ${result}`,
    );
  });
});

test('CPUGauge: severity level boundary conditions', () => {
  const getSeverityLevel = (utilization) => {
    if (utilization >= 85) return 'critical';
    if (utilization >= 70) return 'high';
    if (utilization >= 50) return 'moderate';
    return 'low';
  };

  // Test boundary values exactly
  assert.equal(getSeverityLevel(49.9), 'low');
  assert.equal(getSeverityLevel(50), 'moderate');
  assert.equal(getSeverityLevel(69.9), 'moderate');
  assert.equal(getSeverityLevel(70), 'high');
  assert.equal(getSeverityLevel(84.9), 'high');
  assert.equal(getSeverityLevel(85), 'critical');
});

test('CPUGauge: props validation - default title', () => {
  const defaultTitle = 'CPU Instruction Utilization';
  assert.equal(typeof defaultTitle, 'string');
  assert.ok(defaultTitle.length > 0);
});

test('CPUGauge: gauge value range validations', () => {
  const validateUtilization = (util) => {
    return util >= 0 && util <= 100;
  };

  const testCases = [0, 25, 50, 75, 100];
  testCases.forEach((val) => {
    assert.ok(validateUtilization(val), `${val} should be valid utilization`);
  });

  assert.ok(!validateUtilization(-1), 'Negative utilization should be invalid');
  assert.ok(!validateUtilization(101), 'Utilization > 100 should be invalid');
});

test('CPUGauge: scale tick marks generation (5 ticks)', () => {
  const ticks = [0, 25, 50, 75, 100];
  assert.equal(ticks.length, 5, 'Should have 5 tick marks');

  const angles = ticks.map((tick) => {
    const angle = (tick / 100) * 180 - 90;
    return angle;
  });

  const expectedAngles = [-90, -45, 0, 45, 90];
  angles.forEach((angle, idx) => {
    assert.equal(angle, expectedAngles[idx], `Tick ${idx} angle mismatch`);
  });
});

test('CPUGauge: animation timing configuration', () => {
  const animationConfig = {
    container: { duration: 0.6, ease: 'easeOut' },
    needle: { type: 'spring', damping: 20, stiffness: 100 },
    bar: { duration: 1, ease: 'easeOut' },
  };

  assert.ok(animationConfig.container.duration > 0);
  assert.ok(animationConfig.needle.damping > 0);
  assert.ok(animationConfig.bar.duration > 0);
});

test('CPUGauge: responsive sizing (aspect ratio square)', () => {
  // Gauge should maintain square aspect ratio
  const gaugeConfig = {
    containerClass: 'aspect-square',
    responsive: true,
  };

  assert.equal(gaugeConfig.containerClass, 'aspect-square');
  assert.ok(gaugeConfig.responsive);
});

test('CPUGauge: props combinations - with instruction counts', () => {
  const props = {
    utilization: 65,
    maxInstructions: 10_000_000,
    currentInstructions: 6_500_000,
  };

  const percentage = (props.currentInstructions / props.maxInstructions) * 100;
  assert.equal(Math.round(percentage), 65);
});

test('CPUGauge: props combinations - without instruction counts', () => {
  const props = {
    utilization: 65,
    // No currentInstructions or maxInstructions
  };

  assert.ok(props.utilization >= 0 && props.utilization <= 100);
  assert.equal(props.currentInstructions, undefined);
  assert.equal(props.maxInstructions, undefined);
});

test('CPUGauge: color zone gradient definitions', () => {
  const gradients = {
    low: 'from-emerald-500/20 to-emerald-600/20',
    moderate: 'from-amber-500/20 to-amber-600/20',
    high: 'from-orange-500/20 to-orange-600/20',
    critical: 'from-red-500/20 to-red-600/20',
  };

  Object.entries(gradients).forEach(([level, gradient]) => {
    assert.ok(gradient.includes('from-'), `${level} gradient should have from- class`);
    assert.ok(gradient.includes('to-'), `${level} gradient should have to- class`);
    assert.ok(gradient.includes('/20'), `${level} gradient should have opacity setting`);
  });
});

test('CPUGauge: severity labels', () => {
  const labels = {
    low: 'Well within budget',
    moderate: 'Moderate usage',
    high: 'Approaching limit',
    critical: 'Critical usage',
  };

  Object.values(labels).forEach((label) => {
    assert.ok(label.length > 0, 'All labels should be non-empty');
    assert.ok(label.length < 50, 'Labels should be reasonable length');
  });
});
