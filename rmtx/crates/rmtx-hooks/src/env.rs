//! Hook environment builders (`rmtx/spec/hooks-env.json`).

use std::collections::HashMap;

/// Inputs for path-scoped base env (`path.ExternalCmdEnv()` in Go).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathBaseEnv {
    pub path_name: String,
    pub rtsp_port: String,
    /// Regex capture groups after the full match (`G1`, `G2`, …).
    pub regex_groups: Vec<String>,
}

impl PathBaseEnv {
    pub fn new(path_name: impl Into<String>, rtsp_port: impl Into<String>) -> Self {
        Self {
            path_name: path_name.into(),
            rtsp_port: rtsp_port.into(),
            regex_groups: Vec::new(),
        }
    }

    pub fn with_regex_groups(mut self, groups: Vec<String>) -> Self {
        self.regex_groups = groups;
        self
    }
}

fn insert_path_base(out: &mut HashMap<String, String>, base: &PathBaseEnv) {
    out.insert("MTX_PATH".into(), base.path_name.clone());
    out.insert("RTSP_PATH".into(), base.path_name.clone());
    out.insert("RTSP_PORT".into(), base.rtsp_port.clone());
    for (i, group) in base.regex_groups.iter().enumerate() {
        out.insert(format!("G{}", i + 1), group.clone());
    }
}

/// Base path env: `MTX_PATH`, `RTSP_PATH`, `RTSP_PORT`, `G1..Gn`.
pub fn env_path_base(base: &PathBaseEnv) -> HashMap<String, String> {
    let mut env = HashMap::new();
    insert_path_base(&mut env, base);
    env
}

/// `runOnConnect` / `runOnDisconnect` env.
pub fn env_on_connect(conn_type: &str, conn_id: &str, rtsp_port: &str) -> HashMap<String, String> {
    HashMap::from([
        ("MTX_CONN_TYPE".into(), conn_type.into()),
        ("MTX_CONN_ID".into(), conn_id.into()),
        ("RTSP_PORT".into(), rtsp_port.into()),
    ])
}

/// `runOnReady` / `runOnNotReady` env.
pub fn env_on_ready(
    base: &PathBaseEnv,
    query: &str,
    source_type: Option<&str>,
    source_id: Option<&str>,
) -> HashMap<String, String> {
    let mut env = env_path_base(base);
    env.insert("MTX_QUERY".into(), query.into());
    if let Some(t) = source_type {
        env.insert("MTX_SOURCE_TYPE".into(), t.into());
    }
    if let Some(id) = source_id {
        env.insert("MTX_SOURCE_ID".into(), id.into());
    }
    env
}

/// `runOnRead` / `runOnUnread` env.
pub fn env_on_read(
    base: &PathBaseEnv,
    query: &str,
    reader_type: &str,
    reader_id: &str,
) -> HashMap<String, String> {
    let mut env = env_path_base(base);
    env.insert("MTX_QUERY".into(), query.into());
    env.insert("MTX_READER_TYPE".into(), reader_type.into());
    env.insert("MTX_READER_ID".into(), reader_id.into());
    env
}

/// `runOnRecordSegmentCreate` / `runOnRecordSegmentComplete` env.
pub fn env_on_record_segment(
    base: &PathBaseEnv,
    segment_path: &str,
    segment_duration_secs: Option<f64>,
) -> HashMap<String, String> {
    let mut env = env_path_base(base);
    env.insert("MTX_SEGMENT_PATH".into(), segment_path.into());
    if let Some(secs) = segment_duration_secs {
        env.insert("MTX_SEGMENT_DURATION".into(), format_float(secs));
    }
    env
}

/// `runOnDemand` / `runOnUnDemand` env (query only, no source).
pub fn env_on_demand(base: &PathBaseEnv, query: &str) -> HashMap<String, String> {
    let mut env = env_path_base(base);
    env.insert("MTX_QUERY".into(), query.into());
    env
}

/// `runOnSourceConnect` / `runOnSourceDisconnect` env.
pub fn env_on_source_connect(
    base: &PathBaseEnv,
    source_type: &str,
    source_id: &str,
) -> HashMap<String, String> {
    let mut env = env_path_base(base);
    env.insert("MTX_SOURCE_TYPE".into(), source_type.into());
    env.insert("MTX_SOURCE_ID".into(), source_id.into());
    env
}

fn format_float(v: f64) -> String {
    // Match Go strconv.FormatFloat(..., 'f', -1, 64).
    let s = format!("{v}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_base_includes_regex_groups() {
        let base = PathBaseEnv::new("cam~([0-9]+)", "8554").with_regex_groups(vec!["42".into()]);
        let env = env_path_base(&base);
        assert_eq!(env["MTX_PATH"], "cam~([0-9]+)");
        assert_eq!(env["RTSP_PATH"], "cam~([0-9]+)");
        assert_eq!(env["RTSP_PORT"], "8554");
        assert_eq!(env["G1"], "42");
    }

    #[test]
    fn on_connect_env() {
        let env = env_on_connect("rtspConn", "abc", "8554");
        assert_eq!(env["MTX_CONN_TYPE"], "rtspConn");
        assert_eq!(env["MTX_CONN_ID"], "abc");
        assert_eq!(env["RTSP_PORT"], "8554");
    }

    #[test]
    fn on_ready_env() {
        let base = PathBaseEnv::new("live", "8554");
        let env = env_on_ready(&base, "key=val", Some("rtspSession"), Some("s1"));
        assert_eq!(env["MTX_QUERY"], "key=val");
        assert_eq!(env["MTX_SOURCE_TYPE"], "rtspSession");
        assert_eq!(env["MTX_SOURCE_ID"], "s1");
    }

    #[test]
    fn on_read_env() {
        let base = PathBaseEnv::new("live", "8554");
        let env = env_on_read(&base, "q=1", "hlsMuxer", "m1");
        assert_eq!(env["MTX_READER_TYPE"], "hlsMuxer");
        assert_eq!(env["MTX_READER_ID"], "m1");
    }

    #[test]
    fn on_record_segment_complete_env() {
        let base = PathBaseEnv::new("live", "8554");
        let env = env_on_record_segment(&base, "/tmp/seg.mp4", Some(6.0));
        assert_eq!(env["MTX_SEGMENT_PATH"], "/tmp/seg.mp4");
        assert_eq!(env["MTX_SEGMENT_DURATION"], "6");
    }
}
