//! Benchmark utilities: metric parsing, latency histograms, micro-benchmarks.

#![forbid(unsafe_code)]

mod mediamtx_parse;
mod prometheus_parse;
mod report;
mod stats;

pub use mediamtx_parse::{parse_mediamtx_paths, MediamtxPathSample};
pub use prometheus_parse::{parse_prometheus_gauge, PrometheusSample};
pub use report::{CompareRow, ComparisonReport};
pub use stats::{run_api_load, ApiLoadConfig, ApiLoadResult, LatencySnapshot};

pub mod micro;
pub mod proc_sample;
