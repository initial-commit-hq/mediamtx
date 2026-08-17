//! On-demand playback of recordings
//!
//! Go counterpart: `internal/playback`.
//!
//! Phase 2 scaffold: HTTP server that binds the playback address and exposes
//! MediaMTX-compatible `GET /list` and `GET /get` routes. Real fMP4 muxing of
//! on-disk recording segments is deferred.

#![forbid(unsafe_code)]

mod recordstore;
mod server;

pub use server::{router, PlaybackServer, PlaybackServerError};
