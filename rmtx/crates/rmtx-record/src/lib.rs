//! Recording to fMP4 / MPEG-TS
//!
//! Go counterpart: `internal/recorder`.
//!
//! Phase 2 scaffold: creates the on-disk directory layout and placeholder
//! segment files so operators can verify path templates. Real muxing (the Rust
//! equivalent of mediacommon's fMP4/MPEG-TS writers) comes in a later phase.

#![forbid(unsafe_code)]

mod format;
mod path_template;
mod recorder;

pub use format::RecordFormat;
pub use path_template::{add_format_extension, encode_segment_path};
pub use recorder::{RecordError, Recorder};
