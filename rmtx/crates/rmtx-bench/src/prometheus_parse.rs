//! Minimal Prometheus text parser for rmtx aggregate gauges.

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct PrometheusSample {
    pub paths_ready: f64,
    pub paths_total: f64,
    pub bytes_received_total: f64,
    pub bytes_sent_total: f64,
    pub api_requests_total: f64,
}

/// Reads aggregate gauge/counter values from an rmtx Prometheus scrape body.
pub fn parse_prometheus_gauge(body: &str) -> PrometheusSample {
    let mut out = PrometheusSample::default();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value_str)) = line.split_once(' ') else {
            continue;
        };
        let name = name.split('{').next().unwrap_or(name);
        let Ok(value) = value_str.parse::<f64>() else {
            continue;
        };
        match name {
            "paths_ready" => out.paths_ready = value,
            "paths_total" => out.paths_total = value,
            "bytes_received_total" => out.bytes_received_total = value,
            "bytes_sent_total" => out.bytes_sent_total = value,
            "api_requests_total" => out.api_requests_total = value,
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rmtx_smoke_metrics() {
        let body = r#"# HELP paths_ready gauge
paths_ready 1
bytes_sent_total 50
"#;
        let s = parse_prometheus_gauge(body);
        assert_eq!(s.paths_ready, 1.0);
        assert_eq!(s.bytes_sent_total, 50.0);
    }
}
