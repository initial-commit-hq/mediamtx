//! Prometheus-compatible metrics endpoint
//!
//! Go counterpart: `internal/metrics`.
//!
//! Phase 2 scaffold: exposes aggregate counters/gauges and a `/metrics` scrape
//! endpoint. Per-path labeled series come in a later phase.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use metrics::{counter, describe_counter, describe_gauge, gauge};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use rmtx_path::PathManager;
use serde::Deserialize;
use tracing::info;

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

fn prometheus_handle() -> PrometheusHandle {
    PROMETHEUS_HANDLE
        .get_or_init(|| {
            let handle = PrometheusBuilder::new()
                .install_recorder()
                .expect("install prometheus metrics recorder");

            describe_gauge!("paths_ready", "Number of paths currently in ready state.");
            describe_gauge!("paths_total", "Total number of configured paths.");
            describe_gauge!(
                "bytes_received_total",
                "Total bytes received across all paths (scaffold snapshot)."
            );
            describe_gauge!(
                "bytes_sent_total",
                "Total bytes sent across all paths (scaffold snapshot)."
            );
            describe_counter!("api_requests_total", "Total Control API requests served.");

            handle
        })
        .clone()
}

/// Collects and renders Prometheus text metrics.
pub struct MetricsExporter {
    paths: Option<Arc<PathManager>>,
    api_requests_total: AtomicU64,
}

impl Default for MetricsExporter {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsExporter {
    /// Creates a metrics exporter with no path manager attached.
    pub fn new() -> Self {
        Self {
            paths: None,
            api_requests_total: AtomicU64::new(0),
        }
    }

    /// Attaches a path manager used to snapshot path gauges on render.
    pub fn with_paths(mut self, paths: Arc<PathManager>) -> Self {
        self.paths = Some(paths);
        self
    }

    /// Increments the API request counter (Control API middleware hook point).
    pub fn inc_api_requests(&self) {
        self.api_requests_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Renders all metrics in Prometheus text exposition format.
    pub fn render(&self) -> String {
        let handle = prometheus_handle();

        let paths = self
            .paths
            .as_ref()
            .map(|manager| manager.list())
            .unwrap_or_default();

        let ready = paths.iter().filter(|path| path.ready).count() as f64;
        let total = paths.len() as f64;
        let bytes_received: f64 = paths.iter().map(|path| path.bytes_received).sum::<u64>() as f64;
        let bytes_sent: f64 = paths.iter().map(|path| path.bytes_sent).sum::<u64>() as f64;

        gauge!("paths_ready").set(ready);
        gauge!("paths_total").set(total);
        gauge!("bytes_received_total").set(bytes_received);
        gauge!("bytes_sent_total").set(bytes_sent);
        counter!("api_requests_total").absolute(self.api_requests_total.load(Ordering::Relaxed));

        handle.run_upkeep();
        handle.render()
    }

    /// Renders path metrics in MediaMTX Go `/metrics` text format (for apples-to-apples benchmarks).
    pub fn render_mediamtx_paths(&self) -> String {
        let paths = self
            .paths
            .as_ref()
            .map(|manager| manager.list())
            .unwrap_or_default();

        if paths.is_empty() {
            return concat!(
                "paths 0\n",
                "paths_inbound_bytes 0\n",
                "paths_outbound_bytes 0\n",
                "paths_inbound_frames_in_error 0\n",
                "paths_bytes_received 0\n",
                "paths_bytes_sent 0\n",
                "paths_readers 0\n",
            )
            .to_owned();
        }

        let mut out = String::new();
        for path in paths {
            let state = if path.ready { "ready" } else { "notReady" };
            let tags = format!(r#"{{name="{}",state="{}"}}"#, path.name, state);
            out.push_str(&format!("paths{tags} 1\n"));
            out.push_str(&format!(
                "paths_inbound_bytes{tags} {}\n",
                path.bytes_received
            ));
            out.push_str(&format!("paths_outbound_bytes{tags} {}\n", path.bytes_sent));
            out.push_str(&format!("paths_inbound_frames_in_error{tags} 0\n"));
            out.push_str(&format!("paths_bytes_received{tags} {}\n", path.bytes_received));
            out.push_str(&format!("paths_bytes_sent{tags} {}\n", path.bytes_sent));
            out.push_str(&format!("paths_readers{tags} {}\n", path.readers.len()));
        }
        out
    }
}

/// Query parameters for `/metrics` (Prometheus default; MediaMTX-compatible optional).
#[derive(Debug, Deserialize, Default)]
pub struct MetricsQuery {
    /// When `mediamtx`, returns Go MediaMTX path metric lines instead of Prometheus.
    #[serde(default)]
    pub format: Option<String>,
}

/// Builds an axum router that serves `GET /metrics`.
pub fn router(exporter: Arc<MetricsExporter>) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(exporter)
}

async fn metrics_handler(
    State(exporter): State<Arc<MetricsExporter>>,
    Query(query): Query<MetricsQuery>,
) -> impl IntoResponse {
    let is_mediamtx = query
        .format
        .as_deref()
        .is_some_and(|f| f.eq_ignore_ascii_case("mediamtx"));

    if is_mediamtx {
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            exporter.render_mediamtx_paths(),
        );
    }

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        exporter.render(),
    )
}

/// Parse a listen address like `:9998` into a [`SocketAddr`].
pub fn parse_listen_addr(addr: &str) -> Result<SocketAddr, ServeError> {
    let normalized = if addr.starts_with(':') {
        format!("127.0.0.1{addr}")
    } else {
        addr.to_owned()
    };
    normalized
        .parse()
        .map_err(|_| ServeError::InvalidAddr(addr.to_owned()))
}

/// Serves the metrics scrape endpoint until the listener is closed or an error occurs.
pub async fn serve(addr: SocketAddr, exporter: Arc<MetricsExporter>) -> Result<(), ServeError> {
    let app = router(exporter);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "metrics listener opened");
    axum::serve(listener, app).await.map_err(ServeError::Serve)
}

/// Serves metrics on a string listen address (e.g. `:9998`).
pub async fn serve_listen(addr: &str, exporter: Arc<MetricsExporter>) -> Result<(), ServeError> {
    serve(parse_listen_addr(addr)?, exporter).await
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rmtx_path::PathManager;
    use tower::ServiceExt;

    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn render_contains_help_and_type_lines() {
        let _g = serial_guard();
        let exporter = MetricsExporter::new();
        exporter.inc_api_requests();
        let body = exporter.render();

        assert!(body.contains("# HELP paths_ready"));
        assert!(body.contains("# TYPE paths_ready gauge"));
        assert!(body.contains("# HELP paths_total"));
        assert!(body.contains("# TYPE paths_total gauge"));
        assert!(body.contains("# HELP bytes_received_total"));
        assert!(body.contains("# TYPE bytes_received_total gauge"));
        assert!(body.contains("# HELP bytes_sent_total"));
        assert!(body.contains("# TYPE bytes_sent_total gauge"));
        assert!(body.contains("# HELP api_requests_total"));
        assert!(body.contains("# TYPE api_requests_total counter"));
        assert!(body.contains("api_requests_total"));
    }

    #[test]
    fn render_reflects_path_manager_snapshot() {
        let _g = serial_guard();
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(rmtx_path::PathConf {
            name: "cam".to_owned(),
            ..Default::default()
        });
        paths.set_ready("cam", true, None).unwrap();
        paths.add_bytes_received("cam", 100).unwrap();
        paths.add_bytes_sent("cam", 50).unwrap();

        let exporter = MetricsExporter::new().with_paths(Arc::clone(&paths));
        let body = exporter.render();

        assert!(body.contains("paths_ready 1") || body.contains("paths_ready 1.0"));
        assert!(body.contains("paths_total 1") || body.contains("paths_total 1.0"));
        assert!(body.contains("bytes_received_total 100"));
        assert!(body.contains("bytes_sent_total 50"));
    }

    #[tokio::test]
    async fn metrics_route_oneshot() {
        let _g = serial_guard();
        let exporter = Arc::new(MetricsExporter::new());
        let app = router(Arc::clone(&exporter));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("# TYPE paths_total gauge"));
    }

    #[tokio::test]
    async fn metrics_mediamtx_format_oneshot() {
        let _g = serial_guard();
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(rmtx_path::PathConf {
            name: "cam".to_owned(),
            ..Default::default()
        });
        paths.set_ready("cam", true, None).unwrap();
        paths.add_bytes_sent("cam", 99).unwrap();

        let exporter = Arc::new(MetricsExporter::new().with_paths(paths));
        let app = router(exporter);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics?format=mediamtx")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains(r#"paths{name="cam",state="ready"} 1"#));
        assert!(text.contains("paths_outbound_bytes"));
        assert!(text.contains("99"));
    }

    #[tokio::test]
    async fn metrics_bound_listener() {
        let _g = serial_guard();
        let exporter = Arc::new(MetricsExporter::new());
        exporter.inc_api_requests();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, router(exporter)).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://{addr}/metrics"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let text = response.text().await.unwrap();
        assert!(text.contains("# HELP api_requests_total"));
    }
}
