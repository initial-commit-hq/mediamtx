//! Camera grid viewer HTTP server.
//!
//! Go counterpart: `internal/viewerserver`.
//! Serves the Nuxt SPA from `internal/viewerserver/dist` on `viewerAddress`.

#![forbid(unsafe_code)]

mod server;

pub use server::{router, ViewerServer, ViewerServerError};
