//! Parser for MediaMTX Go `/metrics` text format (paths section).

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct MediamtxPathSample {
    pub paths_ready: u64,
    pub paths_total: u64,
    pub paths_outbound_bytes: u64,
    pub paths_inbound_bytes: u64,
    pub paths_readers: u64,
}

/// Parses `paths*` lines from a MediaMTX metrics scrape body.
pub fn parse_mediamtx_paths(body: &str) -> MediamtxPathSample {
    let mut out = MediamtxPathSample::default();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value_str)) = line.rsplit_once(' ') else {
            continue;
        };
        let Ok(value) = value_str.parse::<u64>() else {
            continue;
        };
        let metric = key.split('{').next().unwrap_or(key);
        match metric {
            "paths" => {
                if key.contains("state=\"ready\"") {
                    out.paths_ready = out.paths_ready.saturating_add(value);
                }
                out.paths_total = out.paths_total.saturating_add(value);
            }
            "paths_outbound_bytes" => {
                out.paths_outbound_bytes = out.paths_outbound_bytes.saturating_add(value);
            }
            "paths_inbound_bytes" => {
                out.paths_inbound_bytes = out.paths_inbound_bytes.saturating_add(value);
            }
            "paths_bytes_sent" => {
                out.paths_outbound_bytes = out.paths_outbound_bytes.saturating_add(value);
            }
            "paths_bytes_received" => {
                out.paths_inbound_bytes = out.paths_inbound_bytes.saturating_add(value);
            }
            "paths_readers" => {
                out.paths_readers = out.paths_readers.saturating_add(value);
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tagged_path_lines() {
        let body = r#"paths{name="live",state="ready"} 1
paths_outbound_bytes{name="live",state="ready"} 4096
paths_readers{name="live",state="ready"} 2
"#;
        let sample = parse_mediamtx_paths(body);
        assert_eq!(sample.paths_total, 1);
        assert_eq!(sample.paths_ready, 1);
        assert_eq!(sample.paths_outbound_bytes, 4096);
        assert_eq!(sample.paths_readers, 2);
    }
}
