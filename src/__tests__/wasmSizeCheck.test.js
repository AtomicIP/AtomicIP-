const fs = require('fs');
const path = require('path');

describe('WASM Size Regression Check', () => {
  const BASELINE_FILE = path.join(__dirname, '../../scripts/wasm-size-baseline.json');
  const REGRESSION_THRESHOLD = 10; // percent

  beforeAll(() => {
    if (!fs.existsSync(BASELINE_FILE)) {
      fs.writeFileSync(
        BASELINE_FILE,
        JSON.stringify({
          ip_registry: 0,
          atomic_swap: 0,
          threshold_percent: REGRESSION_THRESHOLD,
          last_recorded: new Date().toISOString().split('T')[0]
        }, null, 2)
      );
    }
  });

  test('should load baseline WASM sizes', () => {
    const baseline = JSON.parse(fs.readFileSync(BASELINE_FILE, 'utf-8'));
    expect(baseline).toHaveProperty('ip_registry');
    expect(baseline).toHaveProperty('atomic_swap');
    expect(baseline).toHaveProperty('threshold_percent');
  });

  test('should calculate size regression correctly', () => {
    const baseline = 100000;
    const current = 110000;
    const threshold = 10;

    const percentChange = ((current - baseline) * 100) / baseline;
    expect(percentChange).toBe(10);
    expect(Math.abs(percentChange) <= threshold).toBe(true);
  });

  test('should detect size regression beyond threshold', () => {
    const baseline = 100000;
    const current = 115000;
    const threshold = 10;

    const percentChange = ((current - baseline) * 100) / baseline;
    expect(Math.abs(percentChange) > threshold).toBe(true);
  });

  test('should allow minor size variations within threshold', () => {
    const baseline = 100000;
    const current = 104000;
    const threshold = 10;

    const percentChange = ((current - baseline) * 100) / baseline;
    expect(Math.abs(percentChange) <= threshold).toBe(true);
  });

  test('should detect size improvements', () => {
    const baseline = 100000;
    const current = 90000;
    const threshold = 10;

    const percentChange = ((current - baseline) * 100) / baseline;
    expect(percentChange).toBe(-10);
    expect(Math.abs(percentChange) <= threshold).toBe(true);
  });

  test('should track multiple contract sizes', () => {
    const sizes = {
      ip_registry: 250000,
      atomic_swap: 180000
    };

    expect(Object.keys(sizes).length).toBe(2);
    expect(sizes.ip_registry).toBeGreaterThan(0);
    expect(sizes.atomic_swap).toBeGreaterThan(0);
  });

  test('should validate baseline threshold configuration', () => {
    const baseline = JSON.parse(fs.readFileSync(BASELINE_FILE, 'utf-8'));
    expect(baseline.threshold_percent).toBeGreaterThan(0);
    expect(baseline.threshold_percent).toBeLessThan(100);
  });

  test('should track last recorded date', () => {
    const baseline = JSON.parse(fs.readFileSync(BASELINE_FILE, 'utf-8'));
    expect(baseline.last_recorded).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});
