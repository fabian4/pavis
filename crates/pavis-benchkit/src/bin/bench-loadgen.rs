#!/usr/bin/env rust
//! bench-loadgen: Minimal open-loop HTTP load generator for Pavis benchmarks
//!
//! This is a purpose-built tool designed ONLY for latency benchmarking.
//! It is NOT a general-purpose load tester.
//!
//! Key characteristics:
//! - Open-loop: Request issuance follows a fixed schedule independent of responses
//! - HTTP/1.1 keepalive only
//! - No TLS, no scripting, no dynamic payloads
//! - Deterministic, reproducible output
//!
//! Design philosophy: Replace wrk2 with model equivalence, not feature parity.
//!
//! CRITICAL DESIGN INVARIANT:
//! The scheduler NEVER blocks waiting for workers. If the system is saturated
//! (all concurrency slots full), the scheduler drops the request and continues.
//! This preserves true open-loop semantics and makes saturation observable.

use clap::Parser;
use hyper::{Body, Client, Method, Request, Uri, body::HttpBody, client::HttpConnector};
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

// ============================================================================
// CLI Configuration
// ============================================================================

#[derive(Parser, Debug)]
#[clap(
    name = "bench-loadgen",
    about = "Minimal open-loop HTTP load generator"
)]
struct Args {
    /// Target URL (e.g., http://bench-pavis:8080/fixed)
    #[clap(long)]
    url: String,

    /// Target request rate (requests per second)
    #[clap(long)]
    rate: u64,

    /// Test duration in seconds
    #[clap(long)]
    duration: u64,

    /// Maximum number of concurrent in-flight requests (concurrency cap)
    #[clap(long)]
    connections: usize,

    /// Request timeout in seconds
    #[clap(long, default_value = "2")]
    timeout: u64,

    /// Output file path (default: stdout)
    #[clap(long)]
    output: Option<String>,
}

impl Args {
    fn validate(&self) -> Result<(), String> {
        if self.rate == 0 {
            return Err("rate must be > 0".to_string());
        }
        if self.duration == 0 {
            return Err("duration must be > 0".to_string());
        }
        if self.connections == 0 {
            return Err("connections must be > 0".to_string());
        }
        if self.timeout == 0 {
            return Err("timeout must be > 0".to_string());
        }
        // Validate URL
        self.url
            .parse::<Uri>()
            .map_err(|e| format!("invalid URL: {}", e))?;
        Ok(())
    }
}

// ============================================================================
// Statistics & Measurement
// ============================================================================

/// Latency sample (microseconds)
type LatencyMicros = u64;

/// Number of shards for latency collection to avoid single-mutex bottleneck
const LATENCY_SHARDS: usize = 16;

/// Sharded latency storage to minimize lock contention at high RPS.
///
/// Why sharding?
/// - A single global Vec<u64> with mutex becomes a bottleneck at 10k+ RPS.
/// - Sharding spreads lock contention across multiple independent vectors.
/// - Round-robin shard selection provides good distribution without coordination.
struct ShardedLatencies {
    shards: Vec<parking_lot::Mutex<Vec<LatencyMicros>>>,
    next_shard: AtomicUsize,
}

impl ShardedLatencies {
    fn new() -> Self {
        let mut shards = Vec::with_capacity(LATENCY_SHARDS);
        for _ in 0..LATENCY_SHARDS {
            shards.push(parking_lot::Mutex::new(Vec::with_capacity(100_000)));
        }
        Self {
            shards,
            next_shard: AtomicUsize::new(0),
        }
    }

    fn record(&self, latency_micros: u64) {
        // Round-robin shard selection using atomic counter
        let shard_idx = self.next_shard.fetch_add(1, Ordering::Relaxed) % LATENCY_SHARDS;
        self.shards[shard_idx].lock().push(latency_micros);
    }

    fn collect_all(&self) -> Vec<LatencyMicros> {
        let mut all = Vec::new();
        for shard in &self.shards {
            all.extend_from_slice(&shard.lock());
        }
        all
    }
}

/// Shared statistics collector
struct Stats {
    /// Number of requests scheduled by the scheduler (absolute time axis)
    requests_scheduled: AtomicU64,
    /// Number of requests dropped due to saturation (all concurrency slots full)
    dropped: AtomicU64,
    /// Number of requests actually sent to the network
    requests_sent: AtomicU64,
    /// Number of successful responses (2xx/3xx status)
    requests_ok: AtomicU64,
    /// Number of errors (timeout, connection failure, HTTP error status)
    errors: AtomicU64,
    /// Sharded latency samples (microseconds)
    latencies: ShardedLatencies,
}

impl Stats {
    fn new() -> Self {
        Self {
            requests_scheduled: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            requests_sent: AtomicU64::new(0),
            requests_ok: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            latencies: ShardedLatencies::new(),
        }
    }

    fn record_scheduled(&self) {
        self.requests_scheduled.fetch_add(1, Ordering::Relaxed);
    }

    fn record_dropped(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }

    fn record_sent(&self) {
        self.requests_sent.fetch_add(1, Ordering::Relaxed);
    }

    fn record_success(&self, latency_micros: u64) {
        self.requests_ok.fetch_add(1, Ordering::Relaxed);
        self.latencies.record(latency_micros);
    }

    fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    fn compute_percentiles(&self) -> LatencyStats {
        let mut latencies = self.latencies.collect_all();
        if latencies.is_empty() {
            return LatencyStats {
                p50: 0.0,
                p90: 0.0,
                p99: 0.0,
            };
        }

        latencies.sort_unstable();
        let len = latencies.len();

        let p50_idx = (len as f64 * 0.50) as usize;
        let p90_idx = (len as f64 * 0.90) as usize;
        let p99_idx = (len as f64 * 0.99) as usize;

        LatencyStats {
            p50: latencies[p50_idx.min(len - 1)] as f64 / 1000.0,
            p90: latencies[p90_idx.min(len - 1)] as f64 / 1000.0,
            p99: latencies[p99_idx.min(len - 1)] as f64 / 1000.0,
        }
    }
}

#[derive(Serialize)]
struct LatencyStats {
    p50: f64,
    p90: f64,
    p99: f64,
}

#[derive(Serialize)]
struct Summary {
    loadgen: String,
    load_type: String,
    target_rps: u64,
    duration_s: u64,
    connections: usize,
    requests_scheduled: u64,
    dropped: u64,
    requests_sent: u64,
    requests_ok: u64,
    errors: u64,
    achieved_rps: f64,
    latency_ms: LatencyStats,
}

// ============================================================================
// Open-Loop Scheduler (CRITICAL COMPONENT)
// ============================================================================

/// Scheduler issues request tokens at a fixed rate using drift-free deadline-based scheduling.
///
/// This implements TRUE open-loop semantics with the following guarantees:
///
/// 1. Request issuance follows an absolute time axis (monotonic clock).
/// 2. The scheduler NEVER blocks waiting for workers or responses.
/// 3. If concurrency capacity is exhausted, the request is DROPPED (not queued).
/// 4. Dropped requests are counted and visible in output.
/// 5. Time drift is eliminated by computing each deadline from the start time.
///
/// Why this design?
/// - Closed-loop tools (like wrk) issue the next request when the previous completes.
///   This creates coordinated omission: when the system slows down, request rate drops,
///   hiding tail latency under load.
/// - Open-loop tools (like wrk2 and this) issue requests on a fixed schedule.
///   If the system can't keep up, saturation becomes visible (dropped > 0).
///
/// Algorithm:
/// - For request i: deadline_nanos = start_nanos + (i * 1_000_000_000) / rate
/// - Sleep until deadline
/// - Attempt to acquire concurrency permit (non-blocking)
/// - If acquired → spawn worker task
/// - If not acquired → drop request and continue
///
/// The scheduler runs for exactly `duration` seconds on the wall clock,
/// issuing requests at the target rate regardless of system response.
async fn scheduler(
    rate: u64,
    duration: Duration,
    concurrency: Arc<Semaphore>,
    client: Client<HttpConnector>,
    uri: Uri,
    timeout: Duration,
    stats: Arc<Stats>,
) {
    let start = Instant::now();
    let end_time = start + duration;

    let mut request_index: u64 = 0;

    loop {
        // Compute deadline for this request using drift-free formula:
        // deadline_nanos = (request_index * 1_000_000_000) / rate
        //
        // This ensures long-term rate accuracy without accumulating truncation error.
        let deadline_nanos = (request_index * 1_000_000_000) / rate;
        let deadline = start + Duration::from_nanos(deadline_nanos);

        // Check if we've exceeded the test duration
        if deadline >= end_time {
            break;
        }

        // Sleep until deadline (drift-free scheduling)
        let now = Instant::now();
        if now < deadline {
            // Convert std::time::Instant to tokio::time::Instant for sleep_until
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
        }
        // If we're behind schedule (now >= deadline), issue immediately without sleeping

        // Record that this request was scheduled (on the absolute time axis)
        stats.record_scheduled();

        // CRITICAL: Attempt to acquire concurrency permit WITHOUT BLOCKING.
        // This is what preserves open-loop semantics.
        //
        // If try_acquire fails, it means all `connections` slots are full
        // (system is saturated). We drop this request and continue scheduling.
        match concurrency.clone().try_acquire_owned() {
            Ok(permit) => {
                // Concurrency slot available - spawn worker task
                let client_clone = client.clone();
                let uri_clone = uri.clone();
                let stats_clone = Arc::clone(&stats);

                tokio::spawn(async move {
                    // Execute HTTP request
                    execute_request(client_clone, uri_clone, timeout, stats_clone).await;
                    // Permit is automatically dropped here, releasing the concurrency slot
                    drop(permit);
                });
            }
            Err(_) => {
                // All concurrency slots are full - drop this request
                // This is the observable signal that target RPS is not sustainable
                stats.record_dropped();
            }
        }

        request_index += 1;
    }
}

// ============================================================================
// HTTP Request Execution
// ============================================================================

/// Execute a single HTTP request and record latency or error.
///
/// This function is called by spawned worker tasks when concurrency capacity is available.
/// It measures latency from request send to response fully read.
async fn execute_request(
    client: Client<HttpConnector>,
    uri: Uri,
    timeout: Duration,
    stats: Arc<Stats>,
) {
    stats.record_sent();

    let send_time = Instant::now();

    // Build HTTP GET request
    let req = match Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
    {
        Ok(r) => r,
        Err(_) => {
            stats.record_error();
            return;
        }
    };

    // Execute request with timeout
    let result = tokio::time::timeout(timeout, async {
        let resp = client.request(req).await?;
        // Read and discard response body to complete the request
        // This ensures we measure full request/response cycle time
        let mut body = resp.into_body();
        while body.data().await.is_some() {}
        Ok::<_, hyper::Error>(())
    })
    .await;

    let latency = send_time.elapsed();

    match result {
        Ok(Ok(())) => {
            stats.record_success(latency.as_micros() as u64);
        }
        _ => {
            // Timeout or HTTP error
            stats.record_error();
        }
    }
}

// ============================================================================
// Main Execution
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    args.validate()?;

    let uri: Uri = args.url.parse()?;
    let duration = Duration::from_secs(args.duration);
    let timeout = Duration::from_secs(args.timeout);

    // Shared statistics
    let stats = Arc::new(Stats::new());

    // Concurrency control: Semaphore with `connections` permits
    // This represents the maximum number of in-flight requests allowed.
    // When a request starts, it acquires a permit. When it completes, permit is released.
    // The scheduler uses try_acquire (non-blocking) to enforce the cap.
    let concurrency = Arc::new(Semaphore::new(args.connections));

    // Shared HTTP client for all workers
    // HTTP/1.1 keepalive with connection pooling
    let client = Client::builder()
        .pool_max_idle_per_host(args.connections)
        .build_http::<Body>();

    let start = Instant::now();

    // Run the open-loop scheduler
    scheduler(
        args.rate,
        duration,
        concurrency,
        client,
        uri,
        timeout,
        Arc::clone(&stats),
    )
    .await;

    // Wait a brief moment for in-flight requests to complete
    // This is not strictly necessary for correctness but allows clean shutdown
    tokio::time::sleep(Duration::from_millis(100)).await;

    let _elapsed = start.elapsed();

    // Compute final statistics
    let requests_scheduled = stats.requests_scheduled.load(Ordering::Relaxed);
    let dropped = stats.dropped.load(Ordering::Relaxed);
    let requests_sent = stats.requests_sent.load(Ordering::Relaxed);
    let requests_ok = stats.requests_ok.load(Ordering::Relaxed);
    let errors = stats.errors.load(Ordering::Relaxed);

    // Achieved RPS is computed over the configured duration, not elapsed wall time
    // This gives a fair measurement of the loadgen's ability to hit the target rate
    let achieved_rps = requests_ok as f64 / args.duration as f64;

    let latency_stats = stats.compute_percentiles();

    let summary = Summary {
        loadgen: "bench-loadgen".to_string(),
        load_type: "open-loop".to_string(),
        target_rps: args.rate,
        duration_s: args.duration,
        connections: args.connections,
        requests_scheduled,
        dropped,
        requests_sent,
        requests_ok,
        errors,
        achieved_rps,
        latency_ms: latency_stats,
    };

    // Output JSON
    let json = serde_json::to_string_pretty(&summary)?;
    match args.output {
        Some(path) => {
            std::fs::write(path, json)?;
        }
        None => {
            println!("{}", json);
        }
    }

    // If dropped > 0, the target RPS was not sustainable (system saturated)
    // This is working as designed - it makes saturation observable
    if dropped > 0 {
        eprintln!(
            "Warning: {} requests dropped due to saturation (target RPS not sustainable)",
            dropped
        );
    }

    Ok(())
}
