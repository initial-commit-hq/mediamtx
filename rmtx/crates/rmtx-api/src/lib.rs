//! Control API (REST) matching api/openapi.yaml
//!
//! Go counterpart: `internal/api`.

#![forbid(unsafe_code)]

mod auth;
mod config_view;
mod error;
mod handlers;
mod paginate;
mod state;
mod types;

pub use error::ApiError;
pub use state::{AppState, PathInfo};
pub use types::{InfoResponse, OkResponse, PathListResponse, RecordingResponse};

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use rmtx_conf::Conf;
use rmtx_path::PathManager;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

/// Builds the Phase 1 Control API router.
pub fn router(state: AppState) -> Router {
    let cors = cors_layer(&state);
    handlers::routes(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_api_auth,
        ))
        .layer(cors)
}

/// Serves the Control API until the listener is closed or an error occurs.
pub async fn serve(addr: SocketAddr, state: AppState) -> Result<(), ServeError> {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "control API listening");
    axum::serve(listener, app).await.map_err(ServeError::Serve)
}

/// Parse a listen address like `:9997` into a [`SocketAddr`].
///
/// Bare `:port` forms bind `0.0.0.0` (MediaMTX-compatible, reachable on LAN).
pub fn parse_listen_addr(addr: &str) -> Result<SocketAddr, ServeError> {
    let normalized = if let Some(port) = addr.strip_prefix(':') {
        format!("0.0.0.0:{port}")
    } else {
        addr.to_owned()
    };
    normalized
        .parse()
        .map_err(|_| ServeError::InvalidAddr(addr.to_owned()))
}

/// Serves the Control API on a string listen address (e.g. `:9997`).
pub async fn serve_listen(addr: &str, state: AppState) -> Result<(), ServeError> {
    serve(parse_listen_addr(addr)?, state).await
}

/// Compatibility entry point: serves with an empty conf and the given path manager.
pub async fn serve_with_paths(addr: &str, paths: PathManager) -> Result<(), ServeError> {
    let state = AppState::new(env!("CARGO_PKG_VERSION"), Conf::default(), Arc::new(paths));
    serve_listen(addr, state).await
}

/// Errors from [`serve`] and [`serve_listen`].
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("invalid listen address: {0}")]
    InvalidAddr(String),

    #[error("failed to bind or accept connections: {0}")]
    Io(#[from] std::io::Error),

    #[error("server error: {0}")]
    Serve(std::io::Error),
}

impl AppState {
    /// Convenience constructor for tests and early integration.
    pub fn for_test(conf: Conf) -> Self {
        Self::new(
            env!("CARGO_PKG_VERSION"),
            conf,
            Arc::new(PathManager::new()),
        )
    }
}

fn cors_layer(state: &AppState) -> CorsLayer {
    if let Some(origins) = state.api_allow_origins() {
        if origins.iter().any(|o| o == "*") {
            return CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any);
        }
    }
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rmtx_conf::{AuthInternalUser, AuthInternalUserPermission, Conf};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn info_and_paths_list_oneshot() {
        let state = AppState::for_test(Conf::default());
        let app = router(state);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v3/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let info: InfoResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert!(time::OffsetDateTime::parse(
            &info.started,
            &time::format_description::well_known::Rfc3339
        )
        .is_ok());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v3/paths/list")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let list: PathListResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.page_count, 0);
        assert_eq!(list.item_count, 0);
        assert!(list.items.is_empty());
    }

    #[tokio::test]
    async fn info_and_paths_list_bound_listener() {
        let state = AppState::for_test(Conf::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, router(state)).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://{addr}/v3/info"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = client
            .get(format!("http://{addr}/v3/paths/list"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn session_list_stubs_return_empty_pages() {
        let state = AppState::for_test(Conf::default());
        let app = router(state);

        for uri in [
            "/v3/rtspconns/list",
            "/v3/rtspsessions/list",
            "/v3/rtmpconns/list",
            "/v3/webrtcsessions/list",
            "/v3/hlsmuxers/list",
            "/v3/srtconns/list",
            "/v3/recordings/list",
            "/v3/rtspsconns/list",
            "/v3/rtspssessions/list",
            "/v3/rtmpsconns/list",
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "GET {uri}");
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let list: crate::types::EmptyListResponse = serde_json::from_slice(&body).unwrap();
            assert_eq!(list.page_count, 0);
            assert_eq!(list.item_count, 0);
            assert!(list.items.is_empty());
        }
    }

    #[tokio::test]
    async fn session_kick_stubs_return_not_found() {
        let state = AppState::for_test(Conf::default());
        let app = router(state);

        for (uri, error) in [
            ("/v3/rtspsessions/kick/sess-1", "session not found"),
            ("/v3/rtmpconns/kick/conn-1", "connection not found"),
            ("/v3/webrtcsessions/kick/wrtc-1", "session not found"),
            ("/v3/srtconns/kick/srt-1", "connection not found"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(uri)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "POST {uri}");
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(parsed["status"], "error");
            assert_eq!(parsed["error"], error, "POST {uri}");
        }
    }

    #[tokio::test]
    async fn remaining_openapi_get_stubs_return_not_found() {
        let state = AppState::for_test(Conf::default());
        let app = router(state);

        for (uri, error) in [
            ("/v3/rtspconns/get/conn-1", "connection not found"),
            ("/v3/rtspsessions/get/sess-1", "session not found"),
            ("/v3/webrtcsessions/get/wrtc-1", "session not found"),
            ("/v3/hlsmuxers/get/demo", "muxer not found"),
            ("/v3/srtconns/get/srt-1", "connection not found"),
            ("/v3/rtspsconns/get/conn-1", "connection not found"),
            ("/v3/rtmpsconns/get/conn-1", "connection not found"),
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "GET {uri}");
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(parsed["status"], "error");
            assert_eq!(parsed["error"], error, "GET {uri}");
        }
    }

    #[tokio::test]
    async fn jwks_refresh_returns_ok() {
        let state = AppState::for_test(Conf::default());
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v3/auth/jwks/refresh")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["status"], "ok");
    }

    #[tokio::test]
    async fn recordings_get_known_path_returns_empty_segments() {
        let mut conf = Conf::default();
        conf.paths.insert("demo".into(), rmtx_conf::Path::default());
        let state = AppState::for_test(conf);
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v3/recordings/get/demo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let rec: RecordingResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(rec.name, "demo");
        assert!(rec.segments.is_empty());
    }

    #[tokio::test]
    async fn recordings_get_unknown_path_returns_not_found() {
        let state = AppState::for_test(Conf::default());
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v3/recordings/get/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn recordings_delete_segment_stub() {
        let state = AppState::for_test(Conf::default());
        let app = router(state);

        let missing_query = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v3/recordings/deletesegment")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_query.status(), StatusCode::BAD_REQUEST);

        let missing_segment = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v3/recordings/deletesegment?path=demo&start=2020-01-01T00:00:00Z")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_segment.status(), StatusCode::NOT_FOUND);
        let body = missing_segment
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["error"], "segment not found");
    }

    #[tokio::test]
    async fn replace_conf_updates_snapshot() {
        let mut conf = Conf::default();
        conf.log_level = "info".into();
        let state = AppState::for_test(conf);

        let mut new_conf = Conf::default();
        new_conf.log_level = "debug".into();
        state.replace_conf(new_conf).await;

        let global = state.global_config().await;
        assert_eq!(global["logLevel"], "debug");
    }

    #[tokio::test]
    async fn empty_auth_users_allow_info_without_credentials() {
        let state = AppState::for_test(Conf::default());
        assert!(!state.api_auth_enabled());

        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v3/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn configured_auth_user_requires_basic_auth() {
        let mut conf = Conf::default();
        conf.auth_internal_users.push(AuthInternalUser {
            user: "admin".into(),
            pass: "secret".into(),
            ips: vec!["127.0.0.1/32".into()],
            permissions: vec![AuthInternalUserPermission {
                action: "api".into(),
                path: String::new(),
            }],
        });
        let state = AppState::for_test(conf);
        assert!(state.api_auth_enabled());

        let app = router(state);

        let denied = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v3/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

        let credentials =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "admin:secret");
        let ok = app
            .oneshot(
                Request::builder()
                    .uri("/v3/info")
                    .header("Authorization", format!("Basic {credentials}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);
    }
}
