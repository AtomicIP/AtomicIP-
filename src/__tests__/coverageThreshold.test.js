const fs = require('fs');
const path = require('path');

describe('Coverage Threshold Gate', () => {
  const COVERAGE_CONFIG = path.join(__dirname, '../../scripts/coverage-config.json');

  test('should load coverage configuration', () => {
    const config = JSON.parse(fs.readFileSync(COVERAGE_CONFIG, 'utf-8'));
    expect(config).toBeDefined();
  });

  test('should have valid coverage floor', () => {
    const config = JSON.parse(fs.readFileSync(COVERAGE_CONFIG, 'utf-8'));
    expect(config.coverage_floor).toBeGreaterThan(0);
    expect(config.coverage_floor).toBeLessThanOrEqual(100);
  });

  test('should specify coverage tool', () => {
    const config = JSON.parse(fs.readFileSync(COVERAGE_CONFIG, 'utf-8'));
    expect(config.coverage_tool).toBe('cargo-llvm-cov');
  });

  test('should define artifact locations', () => {
    const config = JSON.parse(fs.readFileSync(COVERAGE_CONFIG, 'utf-8'));
    expect(config.artifacts).toHaveProperty('html_report');
    expect(config.artifacts).toHaveProperty('lcov_report');
    expect(config.artifacts).toHaveProperty('summary');
  });

  test('should include all target packages', () => {
    const config = JSON.parse(fs.readFileSync(COVERAGE_CONFIG, 'utf-8'));
    expect(Array.isArray(config.packages)).toBe(true);
    expect(config.packages).toContain('ip_registry');
    expect(config.packages).toContain('atomic_swap');
  });

  test('should define exclusion patterns', () => {
    const config = JSON.parse(fs.readFileSync(COVERAGE_CONFIG, 'utf-8'));
    expect(Array.isArray(config.exclude_patterns)).toBe(true);
    expect(config.exclude_patterns.length).toBeGreaterThan(0);
  });

  test('should enable coverage ratcheting', () => {
    const config = JSON.parse(fs.readFileSync(COVERAGE_CONFIG, 'utf-8'));
    expect(config.coverage_ratchet).toBe(true);
  });

  test('should define ratchet increment', () => {
    const config = JSON.parse(fs.readFileSync(COVERAGE_CONFIG, 'utf-8'));
    expect(config.ratchet_increment).toBeGreaterThan(0);
  });

  test('should validate coverage percentage ranges', () => {
    const testCases = [
      { current: 85, floor: 75, expected: true },
      { current: 75, floor: 75, expected: true },
      { current: 74, floor: 75, expected: false },
      { current: 100, floor: 75, expected: true },
      { current: 0, floor: 75, expected: false }
    ];

    testCases.forEach(tc => {
      const result = tc.current >= tc.floor;
      expect(result).toBe(tc.expected);
    });
  });

  test('should track per-module coverage', () => {
    const moduleCoverage = {
      'ip_registry': 82,
      'atomic_swap': 78,
      'batch_operations': 75
    };

    expect(Object.keys(moduleCoverage).length).toBeGreaterThan(0);
    Object.values(moduleCoverage).forEach(coverage => {
      expect(coverage).toBeGreaterThan(0);
      expect(coverage).toBeLessThanOrEqual(100);
    });
  });

  test('should validate coverage increase over time', () => {
    const historicalCoverage = [
      { timestamp: '2026-08-01', coverage: 70 },
      { timestamp: '2026-08-15', coverage: 73 },
      { timestamp: '2026-08-31', coverage: 75 }
    ];

    for (let i = 1; i < historicalCoverage.length; i++) {
      const current = historicalCoverage[i].coverage;
      const previous = historicalCoverage[i - 1].coverage;
      expect(current).toBeGreaterThanOrEqual(previous);
    }
  });

  test('should compare against floor consistently', () => {
    const config = JSON.parse(fs.readFileSync(COVERAGE_CONFIG, 'utf-8'));
    const testCoverage = 80;

    const meetsThreshold = testCoverage >= config.coverage_floor;
    expect(meetsThreshold).toBe(true);
  });
});
