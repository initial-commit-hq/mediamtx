//! HTTP load generator with latency histograms.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hdrhistogram::Histogram;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ApiLoadConfig {
    pub url: String,
    pub duration: Duration,
    pub concurrency: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatencySnapshot {
    pub count: u64,
    pub requests_per_sec: f64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiLoadResult {
    pub url: String,
    pub duration_secs: f64,
    pub concurrency: usize,
    pub latency: LatencySnapshot,
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("HTTP client: {0}")]
    Http(#[from] reqwest::Error),

    #[error("invalid benchmark duration")]
    InvalidDuration,
}

/// Runs concurrent GET requests until `duration` elapses.
pub async fn run_api_load(config: ApiLoadConfig) -> Result<ApiLoadResult, LoadError> {
    if config.concurrency == 0 || config.duration.is_zero() {
        return Err(LoadError::InvalidDuration);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let stop = Arc::new(AtomicBool::new(false));
    let errors = Arc::new(AtomicU64::new(0));
    let hist = Arc::new(std::sync::Mutex::new(
        Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).expect("histogram bounds"),
    ));

    let started = Instant::now();
    let deadline = started + config.duration;

    let mut handles = Vec::with_capacity(config.concurrency);
    for _ in 0..config.concurrency {
        let client = client.clone();
        let url = config.url.clone();
        let stop = Arc::clone(&stop);
        let errors = Arc::clone(&errors);
        let hist = Arc::clone(&hist);
        handles.push(tokio::spawn(async move {
            while !stop.load(Ordering::Relaxed) {
                if Instant::now() >= deadline {
                    break;
                }
                let t0 = Instant::now();
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        let micros = t0.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
                        if let Ok(mut h) = hist.lock() {
                            let _ = h.record(micros);
                        }
                    }
                    _ => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    tokio::time::sleep(config.duration).await;
    stop.store(true, Ordering::Relaxed);
    for handle in handles {
        let _ = handle.await;
    }

    let elapsed = started.elapsed();
    let hist = hist.lock().expect("histogram lock");
    let count = hist.len();
    let errors = errors.load(Ordering::Relaxed);
    let duration_secs = elapsed.as_secs_f64();

    let latency = LatencySnapshot {
        count,
        requests_per_sec: if duration_secs > 0.0 {
            count as f64 / duration_secs
        } else {
            0.0
        },
        p50_us: hist.value_at_quantile(0.50),
        p95_us: hist.value_at_quantile(0.95),
        p99_us: hist.value_at_quantile(0.99),
        max_us: hist.max(),
        errors,
    };

    Ok(ApiLoadResult {
        url: config.url,
        duration_secs,
        concurrency: config.concurrency,
        latency,
    })
}
