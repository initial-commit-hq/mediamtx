use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OkResponse {
    pub status: &'static str,
}

impl OkResponse {
    pub fn new() -> Self {
        Self { status: "ok" }
    }
}

impl Default for OkResponse {
    fn default() -> Self {
        Self::new()
    }
}

/// GET /v3/info
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InfoResponse {
    pub version: String,
    pub started: String,
}

/// GET /v3/paths/list
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathListResponse {
    #[serde(rename = "pageCount")]
    pub page_count: i64,
    #[serde(rename = "itemCount")]
    pub item_count: i64,
    pub items: Vec<super::state::PathInfo>,
}

/// GET /v3/* list endpoints returning empty paginated results (Phase 2 stubs).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmptyListResponse {
    #[serde(rename = "pageCount")]
    pub page_count: i64,
    #[serde(rename = "itemCount")]
    pub item_count: i64,
    pub items: Vec<serde_json::Value>,
}

impl EmptyListResponse {
    pub fn new() -> Self {
        Self {
            page_count: 0,
            item_count: 0,
            items: Vec::new(),
        }
    }
}

impl Default for EmptyListResponse {
    fn default() -> Self {
        Self::new()
    }
}

/// GET /v3/recordings/get/{name}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecordingResponse {
    pub name: String,
    pub segments: Vec<serde_json::Value>,
}

/// GET /v3/config/paths/list
#[derive(Debug, Clone, Serialize)]
pub struct PathConfListResponse {
    #[serde(rename = "pageCount")]
    pub page_count: i64,
    #[serde(rename = "itemCount")]
    pub item_count: i64,
    pub items: Vec<serde_json::Value>,
}
