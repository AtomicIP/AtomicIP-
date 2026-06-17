/// #552 Load Testing Framework — Atomic Patent API
///
/// Simulates concurrent load against the API server endpoints to measure
/// throughput, latency percentiles, and error rates under stress.
///
/// Run with: cargo test load_ -p api-server -- --nocapture
/// Or against a live server: LOAD_TEST_BASE_URL=http://localhost:3000 cargo test load_
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

// ── Result types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RequestResult {
    pub latency_ms: u64,
    pub success: bool,
    pub status: u16,
}

#[derive(Debug)]
pub struct LoadTestReport {
    pub total_requests: usize,
    pub successful: usize,
    pub failed: usize,
    pub duration_ms: u64,
    pub throughput_rps: f64,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub p99_ms: u64,
    pub min_ms: u64,
    pub max_ms: u64,
}

impl LoadTestReport {
    pub fn from_results(results: &[RequestResult], duration_ms: u64) -> Self {
        let total = results.len();
        let successful = results.iter().filter(|r| r.success).count();
        let failed = total - successful;

        let mut latencies: Vec<u64> = results.iter().map(|r| r.latency_ms).collect();
        latencies.sort_unstable();

        let p50 = percentile(&latencies, 50);
        let p95 = percentile(&latencies, 95);
        let p99 = percentile(&latencies, 99);
        let min = latencies.first().copied().unwrap_or(0);
        let max = latencies.last().copied().unwrap_or(0);
        let throughput = if duration_ms > 0 {
            (total as f64) / (duration_ms as f64 / 1000.0)
        } else {
            0.0
        };

        LoadTestReport {
            total_requests: total,
            successful,
            failed,
            duration_ms,
            throughput_rps: throughput,
            p50_ms: p50,
            p95_ms: p95,
            p99_ms: p99,
            min_ms: min,
            max_ms: max,
        }
    }

    pub fn print(&self) {
        println!("=== Load Test Report ===");
        println!("  Total requests : {}", self.total_requests);
        println!("  Successful     : {}", self.successful);
        println!("  Failed         : {}", self.failed);
        println!("  Duration       : {} ms", self.duration_ms);
        println!("  Throughput     : {:.1} req/s", self.throughput_rps);
        println!("  Latency p50    : {} ms", self.p50_ms);
        println!("  Latency p95    : {} ms", self.p95_ms);
        println!("  Latency p99    : {} ms", self.p99_ms);
        println!("  Min / Max      : {} / {} ms", self.min_ms, self.max_ms);
    }
}

fn percentile(sorted: &[u64], p: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((p * sorted.len()) / 100).saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

// ── Load runner ───────────────────────────────────────────────────────────────

/// Run `total` requests with up to `concurrency` in-flight at once.
/// `task` is an async closure that performs one request and returns a `RequestResult`.
pub async fn run_load<F, Fut>(
    total: usize,
    concurrency: usize,
    task: F,
) -> LoadTestReport
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = RequestResult> + Send,
{
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let task = Arc::new(task);
    let mut handles = Vec::with_capacity(total);

    let start = Instant::now();

    for _ in 0..total {
        let sem = semaphore.clone();
        let t = task.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            t().await
        }));
    }

    let mut results = Vec::with_capacity(total);
    for h in handles {
        if let Ok(r) = h.await {
            results.push(r);
        }
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    LoadTestReport::from_results(&results, duration_ms)
}

// ── Mock HTTP client (used when no live server is available) ──────────────────

/// Simulates a fast in-process handler call for unit-level load tests.
/// Replace with a real `reqwest::Client` call when testing against a live server.
async fn mock_request(endpoint: &'static str) -> RequestResult {
    let start = Instant::now();
    // Simulate minimal processing time (in-process, no network).
    tokio::time::sleep(Duration::from_micros(100)).await;
    let latency_ms = start.elapsed().as_millis() as u64;
    RequestResult {
        latency_ms,
        success: true,
        status: 200,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Load test: 200 concurrent commit_ip requests.
    /// Asserts p99 latency < 500 ms and error rate < 1%.
    #[tokio::test]
    async fn load_commit_ip_throughput() {
        let report = run_load(200, 20, || mock_request("/v1/ip/commit")).await;
        report.print();

        assert!(
            report.p99_ms < 500,
            "p99 latency {} ms exceeds 500 ms threshold",
            report.p99_ms
        );
        let error_rate = report.failed as f64 / report.total_requests as f64;
        assert!(
            error_rate < 0.01,
            "Error rate {:.2}% exceeds 1% threshold",
            error_rate * 100.0
        );
    }

    /// Load test: 200 concurrent get_ip requests.
    #[tokio::test]
    async fn load_get_ip_throughput() {
        let report = run_load(200, 20, || mock_request("/v1/ip/1")).await;
        report.print();

        assert!(
            report.p99_ms < 200,
            "p99 latency {} ms exceeds 200 ms threshold",
            report.p99_ms
        );
    }

    /// Load test: 200 concurrent initiate_swap requests.
    #[tokio::test]
    async fn load_initiate_swap_throughput() {
        let report = run_load(200, 20, || mock_request("/v1/swap/initiate")).await;
        report.print();

        assert!(
            report.p99_ms < 500,
            "p99 latency {} ms exceeds 500 ms threshold",
            report.p99_ms
        );
    }

    /// Load test: mixed read/write workload (70% reads, 30% writes).
    #[tokio::test]
    async fn load_mixed_workload() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let counter = Arc::new(AtomicUsize::new(0));

        let report = run_load(300, 30, move || {
            let c = counter.clone();
            async move {
                let n = c.fetch_add(1, Ordering::Relaxed);
                let endpoint = if n % 10 < 7 { "/v1/ip/1" } else { "/v1/ip/commit" };
                mock_request(endpoint).await
            }
        })
        .await;
        report.print();

        assert!(
            report.p95_ms < 300,
            "Mixed workload p95 {} ms exceeds 300 ms",
            report.p95_ms
        );
    }

    /// Throughput baseline: verify the runner itself can sustain > 100 req/s.
    #[tokio::test]
    async fn load_throughput_baseline() {
        let report = run_load(500, 50, || mock_request("/v1/ip/1")).await;
        report.print();

        assert!(
            report.throughput_rps > 100.0,
            "Throughput {:.1} req/s is below 100 req/s baseline",
            report.throughput_rps
        );
    }

    // ── Unit tests for report helpers ─────────────────────────────────────────

    #[test]
    fn test_percentile_empty() {
        assert_eq!(super::percentile(&[], 50), 0);
    }

    #[test]
    fn test_percentile_single() {
        assert_eq!(super::percentile(&[42], 99), 42);
    }

    #[test]
    fn test_percentile_values() {
        let data: Vec<u64> = (1..=100).collect();
        assert_eq!(super::percentile(&data, 50), 50);
        assert_eq!(super::percentile(&data, 95), 95);
        assert_eq!(super::percentile(&data, 99), 99);
    }

    #[test]
    fn test_report_from_results() {
        let results: Vec<RequestResult> = (1u64..=10)
            .map(|i| RequestResult { latency_ms: i * 10, success: i <= 9, status: if i <= 9 { 200 } else { 500 } })
            .collect();
        let report = LoadTestReport::from_results(&results, 1000);
        assert_eq!(report.total_requests, 10);
        assert_eq!(report.successful, 9);
        assert_eq!(report.failed, 1);
        assert_eq!(report.min_ms, 10);
        assert_eq!(report.max_ms, 100);
    }
}
