//! SRT server (libsrt FFI or ffmpeg TBD).
//!
//! Go counterpart: `internal/servers/srt`.
//!
//! Phase 0 stub — no runtime behavior yet.
//!
//! # Safety
//! Any future `libsrt` FFI bindings **must** justify each `unsafe` block in
//! comments (project brief §9). Prefer ffmpeg fallback until FFI is reviewed.

#![forbid(unsafe_code)]

/// Crate placeholder so the workspace builds during Phase 0.
pub fn phase() -> &'static str {
    "0"
}
