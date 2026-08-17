//! WHEP SDP negotiation (Phase 2 scaffold).
//!
//! With the `webrtc-rs` feature, uses [`webrtc`] to produce an SDP answer from the
//! client's offer. Media track attachment from [`rmtx_stream::Stream`] / StreamBus is
//! deferred to Phase 3.

use thiserror::Error;

/// Phase 3 placeholder message returned when built without `webrtc-rs`.
pub const PHASE3_NOT_IMPLEMENTED: &str =
    "WebRTC WHEP media delivery is not implemented yet (Phase 3). \
HTTP endpoint is live; rebuild with the webrtc-rs feature for SDP answer generation.";

#[derive(Debug, Error)]
#[cfg_attr(not(feature = "webrtc-rs"), allow(dead_code))]
pub enum WhepNegotiationError {
    #[error("empty SDP offer")]
    EmptyOffer,

    #[cfg(feature = "webrtc-rs")]
    #[error("WebRTC negotiation failed: {0}")]
    WebRtc(String),
}

#[cfg(feature = "webrtc-rs")]
mod webrtc_impl {
    use std::sync::Arc;

    use webrtc::peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
        RTCSessionDescription,
    };

    use super::WhepNegotiationError;

    #[derive(Clone)]
    struct NoopHandler;

    #[async_trait::async_trait]
    impl PeerConnectionEventHandler for NoopHandler {}

    pub async fn create_answer(offer_sdp: &str) -> Result<String, WhepNegotiationError> {
        let offer_sdp = offer_sdp.trim();
        if offer_sdp.is_empty() {
            return Err(WhepNegotiationError::EmptyOffer);
        }

        let config = RTCConfigurationBuilder::default().build();
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(Arc::new(NoopHandler))
            .with_udp_addrs(vec!["0.0.0.0:0"])
            .build()
            .await
            .map_err(|e| WhepNegotiationError::WebRtc(e.to_string()))?;

        let offer = RTCSessionDescription::offer(offer_sdp.to_owned())
            .map_err(|e| WhepNegotiationError::WebRtc(e.to_string()))?;
        pc.set_remote_description(offer)
            .await
            .map_err(|e| WhepNegotiationError::WebRtc(e.to_string()))?;

        let answer = pc
            .create_answer(None)
            .await
            .map_err(|e| WhepNegotiationError::WebRtc(e.to_string()))?;
        pc.set_local_description(answer.clone())
            .await
            .map_err(|e| WhepNegotiationError::WebRtc(e.to_string()))?;

        Ok(answer.sdp)
    }
}

#[cfg(feature = "webrtc-rs")]
pub use webrtc_impl::create_answer;
