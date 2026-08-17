//! WHEP media delivery scaffold — subscribes to StreamBus (WebRTC send is Phase 3).

use std::sync::Arc;

use rmtx_stream::Stream;
use tokio::task::JoinHandle;
use tracing::info;

/// Starts a background task that consumes [`Stream`] units for a WHEP session.
///
/// Real RTP → WebRTC track injection is deferred; this proves StreamLookup wiring.
pub fn spawn_whep_stream_delivery(path: String, stream: Arc<Stream>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut sub = stream.subscribe();
        let mut units: u64 = 0;
        loop {
            match sub.recv().await {
                Ok(unit) => {
                    units += 1;
                    if units == 1 {
                        info!(
                            path = %path,
                            bytes = unit.payload.len(),
                            "WHEP media scaffold: receiving StreamBus units"
                        );
                    }
                }
                Err(_) => break,
            }
        }
    })
}
