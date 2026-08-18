//! Static file server for the embedded MediaMTX GUI.

use std::net::SocketAddr;

use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::Embed;
use thiserror::Error;
use tokio::net::TcpListener;
use tracing::info;

/// Nuxt build output (`internal/viewerserver/dist`), same assets Go embeds.
#[derive(Embed)]
#[folder = "../../../internal/viewerserver/dist"]
struct Dist;

/// Viewer listener bound via [`ViewerServer::bind`].
pub struct ViewerServer {
    listener: TcpListener,
}

impl ViewerServer {
    /// Binds a TCP listener on `addr` (e.g. `:9999`).
    ///
    /// Bare `:port` forms are normalized to `0.0.0.0:port` (MediaMTX-compatible).
    pub async fn bind(addr: impl AsRef<str>) -> Result<Self, ViewerServerError> {
        let addr = addr.as_ref();
        let normalized = if let Some(port) = addr.strip_prefix(':') {
            format!("0.0.0.0:{port}")
        } else {
            addr.to_owned()
        };
        let listener = TcpListener::bind(&normalized)
            .await
            .map_err(ViewerServerError::Io)?;
        Ok(Self { listener })
    }

    /// Returns the local socket address (useful when binding `:0`).
    pub fn local_addr(&self) -> Result<SocketAddr, ViewerServerError> {
        self.listener.local_addr().map_err(ViewerServerError::Io)
    }

    /// Serves the GUI until the listener is closed.
    pub async fn run(self) -> Result<(), ViewerServerError> {
        let addr = self.local_addr()?;
        info!(%addr, "viewer listening");
        axum::serve(self.listener, router())
            .await
            .map_err(ViewerServerError::Serve)
    }
}

#[derive(Debug, Error)]
pub enum ViewerServerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("server error: {0}")]
    Serve(std::io::Error),
}

/// Builds the axum router (exported for unit tests).
pub fn router() -> Router {
    Router::new().fallback(get(static_handler))
}

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let file_path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = Dist::get(file_path) {
        return embed_response(file_path, file.data.as_ref());
    }

    // Nuxt SPA: unknown paths (e.g. /sources) fall back to index.html.
    match Dist::get("index.html") {
        Some(file) => embed_response("index.html", file.data.as_ref()),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn embed_response(path: &str, data: &[u8]) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(data.to_vec()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn dist_includes_index() {
        assert!(Dist::get("index.html").is_some());
    }

    #[tokio::test]
    async fn root_serves_gui() {
        let response = router()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("MediaMTX"), "got: {}", &text[..text.len().min(200)]);
    }

    #[tokio::test]
    async fn spa_fallback_serves_index() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/sources")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("MediaMTX"));
    }
}
