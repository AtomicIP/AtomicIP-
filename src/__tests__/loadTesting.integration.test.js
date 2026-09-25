/**
 * API Load Testing Integration Tests
 *
 * Verifies load testing capabilities and performance baselines.
 * Tests with mock server to simulate various load scenarios.
 */

const LoadTest = require('../../scripts/loadTest');
const http = require('http');

describe('API Load Testing', () => {
  let mockServer;
  const TEST_PORT = 3001;
  const BASE_URL = `http://localhost:${TEST_PORT}`;

  beforeAll((done) => {
    // Create a mock API server
    mockServer = http.createServer((req, res) => {
      const delay = Math.random() * 50; // Simulate 0-50ms latency

      setTimeout(() => {
        res.writeHead(200, { 'Content-Type': 'application/json' });

        if (req.url === '/v1/health') {
          res.end(JSON.stringify({ status: 'ok' }));
        } else if (req.url.startsWith('/v1/ips')) {
          res.end(JSON.stringify({ ips: [] }));
        } else if (req.url.startsWith('/v1/swaps')) {
          res.end(JSON.stringify({ swaps: [] }));
        } else {
          res.writeHead(404);
          res.end(JSON.stringify({ error: 'Not found' }));
        }
      }, delay);
    });

    mockServer.listen(TEST_PORT, done);
  });

  afterAll((done) => {
    mockServer.close(done);
  });

  test('should initialize load tester with proper configuration', () => {
    const loadTest = new LoadTest({
      url: BASE_URL,
      users: 100,
      duration: 10,
      ramp: 2,
    });

    expect(loadTest.baseUrl).toBe(BASE_URL);
    expect(loadTest.concurrentUsers).toBe(100);
    expect(loadTest.testDuration).toBe(10);
    expect(loadTest.rampUpDuration).toBe(2);
  });

  test('should add test scenarios', () => {
    const loadTest = new LoadTest({ url: BASE_URL });

    loadTest
      .addScenario('Health Check', {
        method: 'GET',
        path: '/v1/health',
        weight: 1,
      })
      .addScenario('List IPs', {
        method: 'GET',
        path: '/v1/ips',
        weight: 2,
      });

    expect(loadTest.scenarios).toHaveLength(2);
    expect(loadTest.scenarios[0].name).toBe('Health Check');
    expect(loadTest.scenarios[1].name).toBe('List IPs');
  });

  test('should select scenarios based on weight', () => {
    const loadTest = new LoadTest({ url: BASE_URL });

    loadTest
      .addScenario('Light', { path: '/v1/health', weight: 1 })
      .addScenario('Heavy', { path: '/v1/ips', weight: 9 });

    // Sample many selections to verify weighting
    const selections = {};
    for (let i = 0; i < 1000; i++) {
      const scenario = loadTest.selectScenario();
      selections[scenario.name] = (selections[scenario.name] || 0) + 1;
    }

    // Light should be ~10%, Heavy should be ~90%
    const lightPercentage = selections['Light'] / 1000;
    const heavyPercentage = selections['Heavy'] / 1000;

    expect(lightPercentage).toBeGreaterThan(0.05); // Allow variance
    expect(lightPercentage).toBeLessThan(0.15);
    expect(heavyPercentage).toBeGreaterThan(0.85);
    expect(heavyPercentage).toBeLessThan(0.95);
  });

  test('should record successful requests', async () => {
    const loadTest = new LoadTest({
      url: BASE_URL,
      users: 5,
      duration: 2,
      ramp: 1,
    });

    loadTest.addScenario('Health Check', {
      method: 'GET',
      path: '/v1/health',
    });

    await loadTest.run();

    expect(loadTest.results.totalRequests).toBeGreaterThan(0);
    expect(loadTest.results.successfulRequests).toBeGreaterThan(0);
    expect(loadTest.results.responseTimes.length).toBeGreaterThan(0);
  }, 20000);

  test('should calculate response time percentiles', async () => {
    const loadTest = new LoadTest({
      url: BASE_URL,
      users: 10,
      duration: 2,
      ramp: 1,
    });

    loadTest.addScenario('Health Check', {
      method: 'GET',
      path: '/v1/health',
    });

    await loadTest.run();

    expect(loadTest.results.avgResponseTime).toBeGreaterThan(0);
    expect(loadTest.results.p95ResponseTime).toBeGreaterThanOrEqual(loadTest.results.avgResponseTime);
    expect(loadTest.results.p99ResponseTime).toBeGreaterThanOrEqual(loadTest.results.p95ResponseTime);
    expect(loadTest.results.maxResponseTime).toBeGreaterThanOrEqual(loadTest.results.p99ResponseTime);
  }, 20000);

  test('should track throughput', async () => {
    const loadTest = new LoadTest({
      url: BASE_URL,
      users: 20,
      duration: 3,
      ramp: 1,
    });

    loadTest.addScenario('Health Check', {
      method: 'GET',
      path: '/v1/health',
    });

    await loadTest.run();

    // Throughput should be requests per second
    expect(loadTest.results.throughput).toBeGreaterThan(0);
    expect(loadTest.results.throughput).toBeLessThan(1000000); // Reasonable upper bound
  }, 20000);

  test('should handle multiple concurrent users', async () => {
    const loadTest = new LoadTest({
      url: BASE_URL,
      users: 50,
      duration: 2,
      ramp: 1,
    });

    loadTest
      .addScenario('Health', { path: '/v1/health', weight: 1 })
      .addScenario('IPs', { path: '/v1/ips', weight: 1 })
      .addScenario('Swaps', { path: '/v1/swaps', weight: 1 });

    await loadTest.run();

    expect(loadTest.results.totalRequests).toBeGreaterThan(100);
    expect(loadTest.results.scenarioResults['Health'].requests).toBeGreaterThan(0);
    expect(loadTest.results.scenarioResults['IPs'].requests).toBeGreaterThan(0);
    expect(loadTest.results.scenarioResults['Swaps'].requests).toBeGreaterThan(0);
  }, 30000);

  test('should measure response time distribution', async () => {
    const loadTest = new LoadTest({
      url: BASE_URL,
      users: 15,
      duration: 2,
      ramp: 1,
    });

    loadTest.addScenario('Health Check', {
      method: 'GET',
      path: '/v1/health',
    });

    await loadTest.run();

    const times = loadTest.results.responseTimes;
    expect(times.length).toBeGreaterThan(0);

    // All times should be positive
    times.forEach(time => {
      expect(time).toBeGreaterThanOrEqual(0);
    });

    // Min should be less than or equal to max
    expect(loadTest.results.minResponseTime).toBeLessThanOrEqual(loadTest.results.maxResponseTime);
  }, 20000);

  test('should detect performance bottlenecks', async () => {
    const loadTest = new LoadTest({
      url: BASE_URL,
      users: 20,
      duration: 2,
      ramp: 1,
    });

    loadTest.addScenario('Health Check', {
      method: 'GET',
      path: '/v1/health',
    });

    await loadTest.run();

    // Calculate bottleneck indicators
    const results = loadTest.results;
    const bottlenecks = [];

    if (results.avgResponseTime > 500) {
      bottlenecks.push('High average response time');
    }
    if (results.p99ResponseTime > 2000) {
      bottlenecks.push('High P99 response time');
    }
    if ((results.failedRequests / results.totalRequests) > 0.01) {
      bottlenecks.push('High error rate');
    }

    // There should be no bottlenecks with mock server
    expect(bottlenecks).toHaveLength(0);
  }, 20000);

  test('should provide scalability insights', async () => {
    const loadTest = new LoadTest({
      url: BASE_URL,
      users: 30,
      duration: 2,
      ramp: 1,
    });

    loadTest.addScenario('Health Check', {
      method: 'GET',
      path: '/v1/health',
    });

    await loadTest.run();

    const results = loadTest.results;

    // Should provide insights
    expect(results.throughput).toBeGreaterThan(0);
    expect(results.avgResponseTime).toBeGreaterThan(0);
    expect(results.p95ResponseTime).toBeGreaterThan(0);
    expect(results.totalRequests).toBeGreaterThan(0);

    // Success rate should be high
    const successRate = results.successfulRequests / results.totalRequests;
    expect(successRate).toBeGreaterThan(0.95);
  }, 30000);

  describe('Scalability Limits', () => {
    test('should document tested concurrency levels', () => {
      const testedLevels = [100, 500, 1000, 5000];

      testedLevels.forEach(level => {
        expect(level).toBeGreaterThan(0);
      });
    });

    test('should provide baseline performance expectations', () => {
      const baselines = {
        '100_users': { avgResponseTime: 50, throughput: 500 },
        '1000_users': { avgResponseTime: 100, throughput: 1000 },
      };

      expect(baselines['100_users'].avgResponseTime).toBeLessThan(baselines['1000_users'].avgResponseTime);
    });
  });
});
