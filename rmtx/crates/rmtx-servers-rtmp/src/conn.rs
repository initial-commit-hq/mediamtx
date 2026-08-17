//! RTMP connection types aligned with Control API `RTMPConn` / `RTMPConnState`.

use serde::{Deserialize, Serialize};

/// Connection lifecycle state (`RTMPConnState` in OpenAPI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RtmpConnState {
    Idle,
    Read,
    Publish,
}

/// RTMP connection metadata exposed via `/v3/rtmpconns/*`.
#[derive(Debug, Clone)]
pub struct RtmpConn {
    pub id: String,
    pub remote_addr: String,
    pub state: RtmpConnState,
    pub path: Option<String>,
}

impl RtmpConn {
    pub fn new(id: impl Into<String>, remote_addr: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            remote_addr: remote_addr.into(),
            state: RtmpConnState::Idle,
            path: None,
        }
    }
}
