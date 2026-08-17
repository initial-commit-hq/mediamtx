//! RTMP server (Xiu ecosystem)
//!
//! Go counterpart: `internal/servers/rtmp` (uses `gortmplib` today).
//!
//! # Xiu strategy
//!
//! Phase 1 defines types and API alignment only. Runtime RTMP is deferred to
//! Phase 1.5/2. The planned integration path is the [harlanc/xiu](https://github.com/harlanc/xiu)
//! crate family on crates.io, not embedding the full `xiu` binary:
//!
//! - **`rtmp`** — RTMP handshake, sessions, and `RtmpServer` listener.
//! - **`streamhub`** — publisher/subscriber fan-out hub shared by RTMP/HLS/HTTP-FLV in Xiu.
//! - **`xiu`** — monolithic live server (`Service::run`); useful reference, too heavy to embed
//!   wholesale (pulls WebRTC, HLS, HTTP-FLV, etc.).
//!
//! Phase 1.5 can prove a listener using `rtmp` + `streamhub` behind the optional `xiu-rtmp`
//! feature. Phase 2 wires sessions into `rmtx-path`, auth, hooks, and Control API conn listing.
//!
//! See [`../../docs/xiu-evaluation.md`](../../docs/xiu-evaluation.md) for crate evaluation.

#![forbid(unsafe_code)]

mod conn;
mod server;

pub use conn::{RtmpConn, RtmpConnState};
pub use server::{RtmpServer, RtmpServerError};

/// Compile-time marker when the optional `xiu-rtmp` feature is enabled.
#[cfg(feature = "xiu-rtmp")]
pub fn xiu_rtmp_available() -> bool {
    true
}

#[cfg(not(feature = "xiu-rtmp"))]
pub fn xiu_rtmp_available() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rtmp_conn_state_serializes_openapi_values() {
        assert_eq!(
            serde_json::to_value(RtmpConnState::Idle).unwrap(),
            json!("idle")
        );
        assert_eq!(
            serde_json::to_value(RtmpConnState::Read).unwrap(),
            json!("read")
        );
        assert_eq!(
            serde_json::to_value(RtmpConnState::Publish).unwrap(),
            json!("publish")
        );
    }

    #[test]
    fn rtmp_conn_state_deserializes_openapi_values() {
        assert_eq!(
            serde_json::from_value::<RtmpConnState>(json!("idle")).unwrap(),
            RtmpConnState::Idle
        );
        assert_eq!(
            serde_json::from_value::<RtmpConnState>(json!("read")).unwrap(),
            RtmpConnState::Read
        );
        assert_eq!(
            serde_json::from_value::<RtmpConnState>(json!("publish")).unwrap(),
            RtmpConnState::Publish
        );
    }

    #[test]
    fn rtmp_conn_starts_idle() {
        let conn = RtmpConn::new("id", "127.0.0.1:1234");
        assert_eq!(conn.state, RtmpConnState::Idle);
        assert!(conn.path.is_none());
    }

    #[tokio::test]
    async fn rtmp_server_listen_returns_not_implemented() {
        let server = RtmpServer::bind("127.0.0.1:0").await.unwrap();
        let err = server.listen().await.unwrap_err();
        assert!(matches!(err, RtmpServerError::NotImplemented(_)));
    }
}
