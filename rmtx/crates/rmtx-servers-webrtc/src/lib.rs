//! WebRTC WHIP/WHEP server
//!
//! Go counterpart: `internal/servers/webrtc`.
//!
//! Phase 2 scaffold: HTTP WHEP endpoint backed by [`PathManager`]. With the
//! optional `webrtc-rs` feature, POST `/{path}/whep` negotiates SDP via
//! [`webrtc`]. Media from StreamBus is Phase 3.

#![forbid(unsafe_code)]

mod media;
mod server;
mod whep;
mod whip;

pub use server::{router, WebRtcServer, WebRtcServerError, WebRtcState};
pub use whep::PHASE3_NOT_IMPLEMENTED as WHEP_PHASE3_NOT_IMPLEMENTED;
pub use whip::PHASE3_NOT_IMPLEMENTED as WHIP_PHASE3_NOT_IMPLEMENTED;
