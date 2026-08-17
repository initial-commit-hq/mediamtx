//! Runtime path status types (Control API `Path` schema subset).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Source attached to a path (`PathSource` in OpenAPI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub id: String,
}

/// Reader attached to a path (`PathReader` in OpenAPI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathReader {
    #[serde(rename = "type")]
    pub reader_type: String,
    pub id: String,
}

/// Serializable runtime path status (OpenAPI `Path` subset for list/get).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Path {
    pub name: String,
    #[serde(rename = "confName")]
    pub conf_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PathSource>,
    /// Deprecated alias for [`Self::available`]; kept for OpenAPI compatibility.
    pub ready: bool,
    #[serde(rename = "readyTime", skip_serializing_if = "Option::is_none")]
    pub ready_time: Option<DateTime<Utc>>,
    pub available: bool,
    #[serde(rename = "availableTime", skip_serializing_if = "Option::is_none")]
    pub available_time: Option<DateTime<Utc>>,
    pub online: bool,
    #[serde(rename = "onlineTime", skip_serializing_if = "Option::is_none")]
    pub online_time: Option<DateTime<Utc>>,
    pub tracks: Vec<String>,
    #[serde(rename = "bytesReceived")]
    pub bytes_received: u64,
    #[serde(rename = "bytesSent")]
    pub bytes_sent: u64,
    pub readers: Vec<PathReader>,
}
