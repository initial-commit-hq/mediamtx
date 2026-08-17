//! Minimal playback HTTP server (Phase 2 scaffold).
//!
//! Go counterpart: `internal/playback`. Serves `GET /list` and `GET /get` so
//! clients get a bound listener instead of connection refused. Segment muxing
//! from on-disk recordings is not implemented yet.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rmtx_path::PathManager;
use serde::Deserialize;
use thiserror::Error;
use tokio::net::TcpListener;
use tracing::info;

use crate::recordstore;

/// Playback listener bound via [`Self::bind`].
pub struct PlaybackServer {
    listener: TcpListener,
}

impl PlaybackServer {
    /// Binds a TCP listener on `addr` (e.g. `:9996` or `127.0.0.1:0`).
    ///
    /// Bare `:port` forms are normalized to `0.0.0.0:port` (MediaMTX-compatible).
    pub async fn bind(addr: impl AsRef<str>) -> Result<Self, PlaybackServerError> {
        let addr = addr.as_ref();
        let normalized = if let Some(port) = addr.strip_prefix(':') {
            format!("0.0.0.0:{port}")
        } else {
            addr.to_owned()
        };
        let listener = TcpListener::bind(&normalized)
            .await
            .map_err(PlaybackServerError::Io)?;
        Ok(Self { listener })
    }

    /// Returns the local socket address (useful when binding `:0`).
    pub fn local_addr(&self) -> Result<SocketAddr, PlaybackServerError> {
        self.listener.local_addr().map_err(PlaybackServerError::Io)
    }

    /// Serves playback list/get endpoints until the listener is closed.
    pub async fn run(self, paths: Arc<PathManager>) -> Result<(), PlaybackServerError> {
        let addr = self.local_addr()?;
        info!(%addr, "playback server listening");
        axum::serve(self.listener, router(paths))
            .await
            .map_err(PlaybackServerError::Serve)
    }
}

#[derive(Debug, Error)]
pub enum PlaybackServerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("server error: {0}")]
    Serve(std::io::Error),
}

/// Builds the axum router (exported for unit tests).
pub fn router(paths: Arc<PathManager>) -> Router {
    Router::new()
        .route("/list", get(list_handler).options(options_handler))
        .route("/get", get(get_handler).options(options_handler))
        .with_state(paths)
}

#[derive(Debug, Deserialize)]
struct PlaybackQuery {
    path: Option<String>,
    start: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    path: Option<String>,
}

fn cors_headers() -> [(header::HeaderName, HeaderValue); 3] {
    [
        (
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        ),
        (
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("OPTIONS, GET"),
        ),
        (
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Authorization"),
        ),
    ]
}

fn with_cors(response: Response) -> Response {
    let (mut parts, body) = response.into_parts();
    for (name, value) in cors_headers() {
        parts.headers.insert(name, value);
    }
    Response::from_parts(parts, body)
}

async fn options_handler() -> Response {
    (StatusCode::NO_CONTENT, cors_headers()).into_response()
}

fn invalid_path_name() -> Response {
    (StatusCode::BAD_REQUEST, "invalid path name").into_response()
}

fn no_segments() -> Response {
    (StatusCode::NOT_FOUND, "no segments found").into_response()
}

async fn list_handler(
    State(paths): State<Arc<PathManager>>,
    Query(query): Query<ListQuery>,
) -> Response {
    let path = match query.path.as_deref() {
        Some(name) if !name.is_empty() => name,
        _ => return with_cors(invalid_path_name()),
    };

    let Some(conf) = paths.path_conf(path) else {
        return with_cors((StatusCode::BAD_REQUEST, "path not found").into_response());
    };

    let entries = recordstore::list_segments(&conf, path);
    if entries.is_empty() {
        return with_cors(no_segments());
    }

    with_cors(
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into()),
        )
            .into_response(),
    )
}

async fn get_handler(
    State(paths): State<Arc<PathManager>>,
    Query(query): Query<PlaybackQuery>,
) -> Response {
    let path = match query.path.as_deref() {
        Some(name) if !name.is_empty() => name,
        _ => return with_cors(invalid_path_name()),
    };

    let Some(conf) = paths.path_conf(path) else {
        return with_cors((StatusCode::BAD_REQUEST, "path not found").into_response());
    };

    let start = match query.start.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => return with_cors(invalid_path_name()),
    };

    let content_type = if conf.record_format.eq_ignore_ascii_case("mpegts") {
        "video/mp2t"
    } else {
        "video/mp4"
    };

    let Some(file_path) = recordstore::open_segment(&conf, path, start) else {
        return with_cors(no_segments());
    };

    let bytes = match tokio::fs::read(&file_path).await {
        Ok(b) => b,
        Err(_) => return with_cors(no_segments()),
    };

    with_cors(
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            bytes,
        )
            .into_response(),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rmtx_path::{PathConf, PathManager};
    use tower::ServiceExt;

    use super::*;

    fn app_with_demo() -> Router {
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("demo"));
        router(paths)
    }

    #[tokio::test]
    async fn list_missing_path_query_returns_400() {
        let response = app_with_demo()
            .oneshot(Request::builder().uri("/list").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_unknown_path_returns_400() {
        let response = app_with_demo()
            .oneshot(
                Request::builder()
                    .uri("/list?path=missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_known_path_without_segments_returns_404() {
        let response = app_with_demo()
            .oneshot(
                Request::builder()
                    .uri("/list?path=demo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "*"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.as_ref(), b"no segments found");
    }

    #[tokio::test]
    async fn get_known_path_without_start_returns_400() {
        let response = app_with_demo()
            .oneshot(
                Request::builder()
                    .uri("/get?path=demo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_known_path_without_segments_returns_404() {
        let response = app_with_demo()
            .oneshot(
                Request::builder()
                    .uri("/get?path=demo&start=2020-01-01T00:00:00Z")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_returns_json_when_segment_on_disk() {
        use std::fs;
        use std::io::Write;

        use rmtx_path::PathConf;

        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("demo");
        fs::create_dir_all(&rec_dir).unwrap();
        let segment = rec_dir.join("clip.mp4");
        fs::File::create(&segment)
            .unwrap()
            .write_all(b"fake")
            .unwrap();

        let mut conf = PathConf::new("demo");
        conf.record_path = format!("{}/%path", dir.path().display());

        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(conf);

        let response = router(paths)
            .oneshot(
                Request::builder()
                    .uri("/list?path=demo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 1);
        assert!(json[0]["start"].as_str().is_some());
    }

    #[tokio::test]
    async fn options_returns_cors() {
        let response = app_with_demo()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/list")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "*"
        );
    }

    #[tokio::test]
    async fn playback_bound_listener() {
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("demo"));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, router(paths)).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://{addr}/list?path=demo"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    }
}
