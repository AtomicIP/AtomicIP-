#!/usr/bin/env node

/**
 * API Load Testing Script
 *
 * Tests the API server with configurable concurrent users and requests.
 * Identifies bottlenecks and measures response times under load.
 *
 * Usage: node scripts/loadTest.js [options]
 *   --users <n>          Number of concurrent users (default: 1000)
 *   --duration <s>       Test duration in seconds (default: 60)
 *   --ramp <s>           Ramp-up duration in seconds (default: 10)
 *   --url <url>          API base URL (default: http://localhost:3000)
 */

const http = require('http');
const https = require('https');
const crypto = require('crypto');

class LoadTest {
  constructor(options = {}) {
    this.baseUrl = options.url || process.env.API_URL || 'http://localhost:3000';
    this.concurrentUsers = options.users || 1000;
    this.testDuration = options.duration || 60; // seconds
    this.rampUpDuration = options.ramp || 10; // seconds
    this.startTime = null;
    this.endTime = null;

    this.results = {
      totalRequests: 0,
      successfulRequests: 0,
      failedRequests: 0,
      responseTimes: [],
      errors: {},
      scenarioResults: {},
      throughput: 0,
      avgResponseTime: 0,
      p95ResponseTime: 0,
      p99ResponseTime: 0,
      maxResponseTime: 0,
      minResponseTime: Infinity,
    };

    this.scenarios = [];
    this.activeUsers = 0;
  }

  /**
   * Add a test scenario
   */
  addScenario(name, config) {
    this.scenarios.push({
      name,
      method: config.method || 'GET',
      path: config.path,
      body: config.body,
      weight: config.weight || 1,
      signature: config.signature || false,
    });
    this.results.scenarioResults[name] = {
      requests: 0,
      success: 0,
      failed: 0,
      avgResponseTime: 0,
    };
    return this;
  }

  /**
   * Run the load test
   */
  async run() {
    console.log('\n╔════════════════════════════════════════════╗');
    console.log('║     API Load Testing - Starting           ║');
    console.log('╚════════════════════════════════════════════╝\n');

    console.log('Configuration:');
    console.log(`  Base URL: ${this.baseUrl}`);
    console.log(`  Concurrent Users: ${this.concurrentUsers}`);
    console.log(`  Test Duration: ${this.testDuration}s`);
    console.log(`  Ramp-up Duration: ${this.rampUpDuration}s`);
    console.log(`  Total Scenarios: ${this.scenarios.length}\n`);

    this.startTime = Date.now();
    this.endTime = this.startTime + (this.testDuration * 1000);

    // Start ramping up users
    const rampInterval = this.rampUpDuration * 1000 / this.concurrentUsers;
    const userPromises = [];

    for (let i = 0; i < this.concurrentUsers; i++) {
      const delay = i * rampInterval;
      const userPromise = new Promise(resolve => {
        setTimeout(() => {
          this.simulateUser().finally(resolve);
        }, delay);
      });
      userPromises.push(userPromise);
    }

    await Promise.all(userPromises);

    this.printResults();
    return this.results;
  }

  /**
   * Simulate a single user making requests
   */
  async simulateUser() {
    const userId = crypto.randomBytes(4).toString('hex');

    while (Date.now() < this.endTime) {
      const scenario = this.selectScenario();
      await this.executeRequest(scenario, userId);

      // Small delay between requests
      await this.delay(Math.random() * 100);
    }
  }

  /**
   * Select a scenario based on weights
   */
  selectScenario() {
    const totalWeight = this.scenarios.reduce((sum, s) => sum + s.weight, 0);
    let random = Math.random() * totalWeight;

    for (const scenario of this.scenarios) {
      random -= scenario.weight;
      if (random <= 0) {
        return scenario;
      }
    }

    return this.scenarios[0];
  }

  /**
   * Execute a single request
   */
  async executeRequest(scenario, userId) {
    return new Promise((resolve) => {
      const startTime = Date.now();
      const url = new URL(scenario.path, this.baseUrl);
      const isHttps = url.protocol === 'https:';
      const client = isHttps ? https : http;

      const options = {
        hostname: url.hostname,
        port: url.port,
        path: url.pathname + url.search,
        method: scenario.method,
        timeout: 30000,
        headers: {
          'Content-Type': 'application/json',
          'User-Agent': `LoadTester/${userId}`,
          'X-Load-Test': 'true',
        },
      };

      if (scenario.signature) {
        options.headers['X-Address'] = 'GCZXWVG5FGTWTJWY5M3DMX3S2Z4XYFABXJZLOWMVTQKJMHFJGDQGSVRQ';
        options.headers['X-Timestamp'] = Math.floor(Date.now() / 1000);
        options.headers['X-Signature'] = crypto.randomBytes(64).toString('hex');
      }

      const req = client.request(options, (res) => {
        let responseData = '';

        res.on('data', (chunk) => {
          responseData += chunk;
        });

        res.on('end', () => {
          const responseTime = Date.now() - startTime;
          this.recordResult(scenario.name, res.statusCode, responseTime);
          resolve();
        });
      });

      req.on('error', (error) => {
        const responseTime = Date.now() - startTime;
        this.recordError(scenario.name, error.message, responseTime);
        resolve();
      });

      req.on('timeout', () => {
        req.destroy();
        const responseTime = Date.now() - startTime;
        this.recordError(scenario.name, 'Timeout', responseTime);
        resolve();
      });

      if (scenario.body) {
        req.write(JSON.stringify(scenario.body));
      }

      req.end();
    });
  }

  /**
   * Record a successful request
   */
  recordResult(scenarioName, statusCode, responseTime) {
    this.results.totalRequests++;
    this.results.responseTimes.push(responseTime);
    this.results.maxResponseTime = Math.max(this.results.maxResponseTime, responseTime);
    this.results.minResponseTime = Math.min(this.results.minResponseTime, responseTime);

    const scenario = this.results.scenarioResults[scenarioName];
    scenario.requests++;

    if (statusCode >= 200 && statusCode < 300) {
      this.results.successfulRequests++;
      scenario.success++;
    } else {
      this.results.failedRequests++;
      scenario.failed++;
      this.recordError(scenarioName, `HTTP ${statusCode}`, responseTime);
    }
  }

  /**
   * Record an error
   */
  recordError(scenarioName, error, responseTime) {
    if (!this.results.errors[error]) {
      this.results.errors[error] = 0;
    }
    this.results.errors[error]++;
    this.results.failedRequests++;

    const scenario = this.results.scenarioResults[scenarioName];
    scenario.failed++;
  }

  /**
   * Calculate percentiles
   */
  calculatePercentile(percentile) {
    const sorted = this.results.responseTimes.sort((a, b) => a - b);
    const index = Math.ceil((percentile / 100) * sorted.length) - 1;
    return sorted[Math.max(0, index)] || 0;
  }

  /**
   * Print results
   */
  printResults() {
    const testDuration = (Date.now() - this.startTime) / 1000;
    this.results.throughput = this.results.totalRequests / testDuration;
    this.results.avgResponseTime = Math.round(
      this.results.responseTimes.reduce((a, b) => a + b, 0) / this.results.responseTimes.length || 0
    );
    this.results.p95ResponseTime = this.calculatePercentile(95);
    this.results.p99ResponseTime = this.calculatePercentile(99);

    console.log('╔════════════════════════════════════════════╗');
    console.log('║      Load Test Results                    ║');
    console.log('╚════════════════════════════════════════════╝\n');

    console.log('Overall Results:');
    console.log(`  Total Requests: ${this.results.totalRequests.toLocaleString()}`);
    console.log(`  Successful: ${this.results.successfulRequests.toLocaleString()} (${((this.results.successfulRequests / this.results.totalRequests) * 100).toFixed(2)}%)`);
    console.log(`  Failed: ${this.results.failedRequests.toLocaleString()} (${((this.results.failedRequests / this.results.totalRequests) * 100).toFixed(2)}%)`);
    console.log(`  Test Duration: ${testDuration.toFixed(2)}s\n`);

    console.log('Performance Metrics:');
    console.log(`  Throughput: ${this.results.throughput.toFixed(2)} req/s`);
    console.log(`  Avg Response Time: ${this.results.avgResponseTime}ms`);
    console.log(`  Min Response Time: ${this.results.minResponseTime}ms`);
    console.log(`  Max Response Time: ${this.results.maxResponseTime}ms`);
    console.log(`  P95 Response Time: ${this.results.p95ResponseTime}ms`);
    console.log(`  P99 Response Time: ${this.results.p99ResponseTime}ms\n`);

    if (Object.keys(this.results.errors).length > 0) {
      console.log('Errors:');
      for (const [error, count] of Object.entries(this.results.errors)) {
        console.log(`  ${error}: ${count}`);
      }
      console.log('');
    }

    console.log('Scenario Results:');
    for (const [name, scenario] of Object.entries(this.results.scenarioResults)) {
      const avgTime = scenario.requests > 0
        ? Math.round(
            this.results.responseTimes
              .slice(0, Math.floor(scenario.requests / this.results.totalRequests * this.results.responseTimes.length))
              .reduce((a, b) => a + b, 0) / scenario.requests
          )
        : 0;

      console.log(`  ${name}:`);
      console.log(`    Requests: ${scenario.requests}`);
      console.log(`    Success: ${scenario.success} (${((scenario.success / (scenario.success + scenario.failed)) * 100).toFixed(2)}%)`);
      console.log(`    Failed: ${scenario.failed}`);
    }

    console.log('\nBottlenecks Detected:');
    if (this.results.avgResponseTime > 500) {
      console.log(`  ⚠️  High average response time: ${this.results.avgResponseTime}ms`);
    }
    if (this.results.p99ResponseTime > 2000) {
      console.log(`  ⚠️  High P99 response time: ${this.results.p99ResponseTime}ms`);
    }
    if ((this.results.failedRequests / this.results.totalRequests) > 0.01) {
      console.log(`  ⚠️  Error rate: ${((this.results.failedRequests / this.results.totalRequests) * 100).toFixed(2)}%`);
    }
    if (this.results.throughput < 100) {
      console.log(`  ⚠️  Low throughput: ${this.results.throughput.toFixed(2)} req/s`);
    }

    console.log('\nRecommendations:');
    console.log('  1. Monitor database query performance');
    console.log('  2. Implement caching for frequently accessed data');
    console.log('  3. Add connection pooling');
    console.log('  4. Consider rate limiting to manage load');
    console.log('  5. Optimize hot paths in critical operations\n');
  }

  /**
   * Helper: delay function
   */
  delay(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// Parse command line arguments
function parseArgs() {
  const args = process.argv.slice(2);
  const options = {};

  for (let i = 0; i < args.length; i += 2) {
    const key = args[i].replace('--', '');
    const value = args[i + 1];

    if (key === 'users') options.users = parseInt(value);
    else if (key === 'duration') options.duration = parseInt(value);
    else if (key === 'ramp') options.ramp = parseInt(value);
    else if (key === 'url') options.url = value;
  }

  return options;
}

// Main execution
async function main() {
  const options = parseArgs();

  const loadTest = new LoadTest(options);

  // Define test scenarios
  loadTest
    .addScenario('GET /v1/ips', {
      method: 'GET',
      path: '/v1/ips',
      weight: 2,
    })
    .addScenario('POST /v1/ips (commit)', {
      method: 'POST',
      path: '/v1/ips',
      weight: 1,
      signature: true,
      body: {
        ip_hash: crypto.randomBytes(32).toString('hex'),
        blinding_factor: crypto.randomBytes(32).toString('hex'),
      },
    })
    .addScenario('GET /v1/swaps', {
      method: 'GET',
      path: '/v1/swaps',
      weight: 2,
    })
    .addScenario('POST /v1/swaps', {
      method: 'POST',
      path: '/v1/swaps',
      weight: 1,
      signature: true,
      body: {
        initiator: 'GCZXWVG5FGTWTJWY5M3DMX3S2Z4XYFABXJZLOWMVTQKJMHFJGDQGSVRQ',
        amount: '1000',
      },
    })
    .addScenario('GET /v1/health', {
      method: 'GET',
      path: '/v1/health',
      weight: 1,
    });

  try {
    await loadTest.run();
    process.exit(0);
  } catch (error) {
    console.error('Load test failed:', error);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}

module.exports = LoadTest;
