//! RTSP server and Retina-based client ingest
//!
//! Go counterpart: `internal/servers/rtsp`.
//!
//! Phase 1 scaffold: types, traits, and `connect_source` via Retina DESCRIBE when enabled.

#![forbid(unsafe_code)]

mod ingest;
mod publish;
mod pull;
mod server;
mod source;

pub use ingest::{FfmpegFallback, RetinaIngest, RtspIngest, StubIngest};
pub use pull::RtspPullSession;
pub use rmtx_stream::{NoStreamLookup, StreamLookup};
pub use server::{RtspServer, RtspServerError};
pub use source::{connect_source, RtspSource, RtspSourceError};
