//! RTMP server listener (Phase 2 scaffold).
//!
//! With feature `xiu-rtmp`, accepted connections are handed to harlanc `ServerSession`
//! (real RTMP handshake + chunk protocol). Without it, `bind`/`run` still accept TCP
//! for tests; the binary only starts the listener when `xiu-rtmp` is enabled.

use std::net::SocketAddr;

use thiserror::Error;
use tokio::net::TcpListener;
use tracing::{debug, info};

#[cfg(not(feature = "xiu-rtmp"))]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(not(feature = "xiu-rtmp"))]
use tokio::net::TcpStream;

/// RTMP listener bound via [`Self::bind`].
pub struct RtmpServer {
    listener: TcpListener,
    address: String,
}

impl RtmpServer {
    /// Binds a TCP listener on `addr` (e.g. `:1935` or `127.0.0.1:0`).
    ///
    /// Bare `:port` forms are normalized to `0.0.0.0:port` (MediaMTX-compatible).
    pub async fn bind(addr: impl AsRef<str>) -> Result<Self, RtmpServerError> {
        let addr = addr.as_ref();
        let normalized = normalize_listen_addr(addr);
        let listener = TcpListener::bind(&normalized)
            .await
            .map_err(RtmpServerError::Io)?;
        Ok(Self {
            listener,
            address: normalized,
        })
    }

    /// Returns the configured listen address string.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Returns the local socket address (useful when binding `:0`).
    pub fn local_addr(&self) -> Result<SocketAddr, RtmpServerError> {
        self.listener.local_addr().map_err(RtmpServerError::Io)
    }

    /// Accepts RTMP connections until the listener is closed.
    #[cfg(feature = "xiu-rtmp")]
    pub async fn run(
        self,
        paths: std::sync::Arc<rmtx_path::PathManager>,
        side: std::sync::Arc<rmtx_staticsources::StaticSourceSideTable>,
    ) -> Result<(), RtmpServerError> {
        use rmtx_staticsources::on_rtmp_publish;
        use rtmp::session::server_session::ServerSession;
        use streamhub::define::BroadcastEvent;
        use streamhub::stream::StreamIdentifier;
        use streamhub::StreamsHub;

        let mut stream_hub = StreamsHub::new(None);
        let mut client_events = stream_hub.get_client_event_consumer();
        let paths_for_events = std::sync::Arc::clone(&paths);
        let side_for_events = std::sync::Arc::clone(&side);
        tokio::spawn(async move {
            while let Ok(ev) = client_events.recv().await {
                if let BroadcastEvent::Publish { identifier } = ev {
                    if let StreamIdentifier::Rtmp { stream_name, .. } = identifier {
                        on_rtmp_publish(
                            &stream_name,
                            paths_for_events.clone(),
                            side_for_events.clone(),
                        );
                    }
                }
            }
        });

        let sender = stream_hub.get_hub_event_sender();
        tokio::spawn(async move {
            stream_hub.run().await;
        });

        info!(
            addr = %self.local_addr()?.to_string(),
            "RTMP server listening (xiu-rtmp)"
        );

        loop {
            let (stream, peer) = self.listener.accept().await.map_err(RtmpServerError::Io)?;
            debug!(%peer, "RTMP client connected");
            let sender = sender.clone();
            tokio::spawn(async move {
                let mut session = ServerSession::new(stream, sender, 1, None);
                if let Err(e) = session.run().await {
                    debug!(%peer, error = %e, "RTMP session ended");
                }
            });
        }
    }

    /// Stub accept loop when built without `xiu-rtmp`.
    #[cfg(not(feature = "xiu-rtmp"))]
    pub async fn run(self) -> Result<(), RtmpServerError> {
        info!(
            addr = %self.local_addr()?.to_string(),
            "RTMP server listening (stub — enable xiu-rtmp for handshake)"
        );

        loop {
            let (stream, peer) = self.listener.accept().await.map_err(RtmpServerError::Io)?;
            debug!(%peer, "RTMP client connected (stub)");
            tokio::spawn(async move {
                if let Err(e) = handle_stub_connection(stream, peer).await {
                    debug!(error = %e, "RTMP stub connection ended");
                }
            });
        }
    }

    /// Placeholder for Phase 1 API compatibility — prefer [`Self::bind`] + [`Self::run`].
    pub async fn listen(&self) -> Result<(), RtmpServerError> {
        info!(address = %self.address, "RTMP server listen (use bind + run instead)");
        Err(RtmpServerError::NotImplemented(
            "call RtmpServer::bind().await?.run().await instead".into(),
        ))
    }
}

#[cfg(not(feature = "xiu-rtmp"))]
async fn handle_stub_connection(
    mut stream: TcpStream,
    peer: SocketAddr,
) -> Result<(), RtmpServerError> {
    info!(
        %peer,
        "RTMP stub: TCP accepted; handshake/AMF publish deferred (build with xiu-rtmp)"
    );

    let mut buf = [0u8; 1537];
    match stream.read(&mut buf).await {
        Ok(0) => {}
        Ok(n) if n >= 1 && buf[0] == 3 => {
            info!(
                %peer,
                bytes_read = n,
                "RTMP stub: C0 (version 3) seen; next step is S0S1S2 handshake via xiu-rtmp"
            );
        }
        Ok(n) => {
            debug!(%peer, bytes_read = n, "RTMP stub: non-RTMP or partial first read");
        }
        Err(e) => return Err(RtmpServerError::Io(e)),
    }

    let _ = stream.shutdown().await;
    Ok(())
}

fn normalize_listen_addr(addr: &str) -> String {
    if let Some(port) = addr.strip_prefix(':') {
        format!("0.0.0.0:{port}")
    } else {
        addr.to_owned()
    }
}

#[derive(Debug, Error)]
pub enum RtmpServerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("RTMP server not implemented: {0}")]
    NotImplemented(String),
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;

    use super::*;

    #[tokio::test]
    async fn bind_and_accept_connection() {
        let server = RtmpServer::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();

        let handle = tokio::spawn(async move { server.run().await });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(&[3]).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        drop(client);
        handle.abort();
    }

    #[cfg(feature = "xiu-rtmp")]
    #[tokio::test]
    async fn xiu_rtmp_accepts_rtmp_c0() {
        use std::sync::Arc;

        use rmtx_path::PathManager;
        use rmtx_staticsources::StaticSourceSideTable;

        let server = RtmpServer::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let paths = Arc::new(PathManager::new());
        let side = Arc::new(StaticSourceSideTable::new());

        let handle = tokio::spawn(async move { server.run(paths, side).await });

        let mut client = TcpStream::connect(addr).await.unwrap();
        // Minimal C0 only — server session may wait for full C0C1; connect success is enough.
        client.write_all(&[3]).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        drop(client);
        handle.abort();
    }

    #[tokio::test]
    async fn normalize_bare_port() {
        let server = RtmpServer::bind(":0").await.unwrap();
        assert!(server.address().starts_with("0.0.0.0:"));
    }
}
