//! WHIP SDP negotiation (Phase 2 scaffold).
//!
//! Reuses WHEP SDP answer generation; media ingest from the publisher is Phase 3.

/// Phase 3 placeholder message returned when built without `webrtc-rs`.
pub const PHASE3_NOT_IMPLEMENTED: &str =
    "WebRTC WHIP publishing is not implemented yet (Phase 3). \
HTTP endpoint is live; rebuild with the webrtc-rs feature for SDP answer generation.";

#[cfg(feature = "webrtc-rs")]
pub use crate::whep::{create_answer, WhepNegotiationError as WhipNegotiationError};
