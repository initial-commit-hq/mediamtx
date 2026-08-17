//! Side-by-side comparison report for benchmark JSON artifacts.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareRow {
    pub metric: String,
    pub go_value: String,
    pub rust_value: String,
    pub delta_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub rows: Vec<CompareRow>,
}

impl ComparisonReport {
    pub fn from_latency_json(go: &serde_json::Value, rust: &serde_json::Value) -> Self {
        let mut rows = Vec::new();
        for key in [
            "api_load.latency.requests_per_sec",
            "api_load.latency.p50_us",
            "api_load.latency.p95_us",
            "api_load.latency.p99_us",
            "process.rss_kb_max",
            "process.cpu_pct_max",
        ] {
            let go_v = json_path(go, key);
            let rust_v = json_path(rust, key);
            let delta_pct = numeric_delta_pct(&go_v, &rust_v);
            rows.push(CompareRow {
                metric: key.to_owned(),
                go_value: go_v,
                rust_value: rust_v,
                delta_pct,
            });
        }
        Self { rows }
    }

    pub fn print_table(&self) {
        println!("{:<32} {:>16} {:>16} {:>12}", "metric", "go", "rust", "delta%");
        for row in &self.rows {
            let delta = row
                .delta_pct
                .map(|d| format!("{d:+.1}"))
                .unwrap_or_else(|| "-".to_owned());
            println!(
                "{:<32} {:>16} {:>16} {:>12}",
                row.metric, row.go_value, row.rust_value, delta
            );
        }
    }
}

fn json_path(v: &serde_json::Value, path: &str) -> String {
    let mut cur = v;
    for part in path.split('.') {
        cur = cur.get(part).unwrap_or(&serde_json::Value::Null);
    }
    match cur {
        serde_json::Value::Null => "-".to_owned(),
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn numeric_delta_pct(go: &str, rust: &str) -> Option<f64> {
    let go: f64 = go.parse().ok()?;
    let rust: f64 = rust.parse().ok()?;
    if go.abs() < f64::EPSILON {
        return None;
    }
    Some(((rust - go) / go) * 100.0)
}
