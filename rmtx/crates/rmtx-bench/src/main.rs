//! `rmtx-bench` — load generators and Go vs Rust comparison helpers.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use rmtx_bench::micro;
use rmtx_bench::proc_sample;
use rmtx_bench::{
    parse_mediamtx_paths, parse_prometheus_gauge, run_api_load, ApiLoadConfig, ComparisonReport,
};
use serde::Serialize;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "rmtx-bench", about = "Benchmark MediaMTX / rmtx")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Concurrent HTTP GET load with latency histogram.
    ApiLoad {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "10")]
        duration_secs: u64,
        #[arg(long, default_value = "4")]
        concurrency: usize,
    },
    /// In-process StreamBus fan-out throughput (Rust baseline).
    MicroStreamFanout {
        #[arg(long, default_value = "8")]
        subscribers: usize,
        #[arg(long, default_value = "50000")]
        units: u64,
    },
    /// Sample `/metrics` once and print JSON.
    SampleMetrics {
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "prometheus")]
        format: MetricsFormat,
    },
    /// Poll process RSS/CPU for a running server PID.
    SampleProcess {
        #[arg(long)]
        pid: u32,
        #[arg(long, default_value = "10")]
        duration_secs: u64,
        #[arg(long, default_value = "500")]
        interval_ms: u64,
    },
    /// API load + metrics + process sample in one JSON report.
    Suite {
        #[arg(long)]
        implementation: String,
        #[arg(long)]
        api_url: String,
        #[arg(long)]
        metrics_url: String,
        #[arg(long, default_value = "prometheus")]
        metrics_format: MetricsFormat,
        #[arg(long)]
        pid: u32,
        #[arg(long, default_value = "15")]
        duration_secs: u64,
        #[arg(long, default_value = "4")]
        concurrency: usize,
    },
    /// Print a comparison table from two suite JSON files.
    Compare {
        #[arg(long)]
        go: PathBuf,
        #[arg(long)]
        rust: PathBuf,
    },
}

#[derive(Clone, clap::ValueEnum)]
enum MetricsFormat {
    Prometheus,
    Mediamtx,
}

#[derive(Serialize)]
struct SuiteReport {
    implementation: String,
    api_load: rmtx_bench::ApiLoadResult,
    metrics: serde_json::Value,
    process: proc_sample::ProcSample,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::ApiLoad {
            url,
            duration_secs,
            concurrency,
        } => {
            let result = run_api_load(ApiLoadConfig {
                url,
                duration: Duration::from_secs(duration_secs),
                concurrency,
            })
            .await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::MicroStreamFanout { subscribers, units } => {
            let result = micro::stream_fanout(subscribers, units).await;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::SampleMetrics { url, format } => {
            let client = reqwest::Client::new();
            let body = client.get(&url).send().await?.error_for_status()?.text().await?;
            let metrics = match format {
                MetricsFormat::Prometheus => {
                    serde_json::to_value(parse_prometheus_gauge(&body))?
                }
                MetricsFormat::Mediamtx => serde_json::to_value(parse_mediamtx_paths(&body))?,
            };
            println!("{}", serde_json::to_string_pretty(&metrics)?);
        }
        Commands::SampleProcess {
            pid,
            duration_secs,
            interval_ms,
        } => {
            let sample = proc_sample::sample_process(
                pid,
                Duration::from_secs(duration_secs),
                Duration::from_millis(interval_ms),
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&sample)?);
        }
        Commands::Suite {
            implementation,
            api_url,
            metrics_url,
            metrics_format,
            pid,
            duration_secs,
            concurrency,
        } => {
            let duration = Duration::from_secs(duration_secs);
            let proc_handle = tokio::spawn(proc_sample::sample_process(
                pid,
                duration,
                Duration::from_millis(500),
            ));
            let api_load = run_api_load(ApiLoadConfig {
                url: api_url,
                duration,
                concurrency,
            })
            .await?;

            let client = reqwest::Client::new();
            let body = client
                .get(&metrics_url)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            let metrics = match metrics_format {
                MetricsFormat::Prometheus => {
                    serde_json::to_value(parse_prometheus_gauge(&body))?
                }
                MetricsFormat::Mediamtx => serde_json::to_value(parse_mediamtx_paths(&body))?,
            };

            let process = proc_handle.await??;
            let report = SuiteReport {
                implementation,
                api_load,
                metrics,
                process,
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::Compare { go, rust } => {
            let go_json: serde_json::Value =
                serde_json::from_slice(&std::fs::read(go)?)?;
            let rust_json: serde_json::Value =
                serde_json::from_slice(&std::fs::read(rust)?)?;
            let table = ComparisonReport::from_latency_json(&go_json, &rust_json);
            table.print_table();
        }
    }
    Ok(())
}
