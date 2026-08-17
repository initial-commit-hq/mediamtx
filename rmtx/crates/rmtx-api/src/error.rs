use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// Generic API error body (`{ "status": "error", "error": "..." }`).
#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub status: &'static str,
    pub error: String,
}

impl ApiError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            status: "error",
            error: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (StatusCode::BAD_REQUEST, Json(Self::new(message)))
    }

    pub fn not_found(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (StatusCode::NOT_FOUND, Json(Self::new(message)))
    }

    pub fn internal(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(Self::new(message)))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}
