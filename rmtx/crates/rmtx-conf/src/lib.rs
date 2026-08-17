//! MediaMTX-compatible YAML/env configuration
//!
//! Go counterpart: `internal/conf`.

#![forbid(unsafe_code)]

mod always_available_track;
mod auth;
mod bool_yaml;
mod conf;
mod de;
mod error;
mod path;
mod watcher;

pub use always_available_track::AlwaysAvailableTrack;
pub use auth::{AuthInternalUser, AuthInternalUserPermission};
pub use conf::Conf;
pub use error::{ConfError, Result};
pub use path::Path;
pub use watcher::ConfWatcher;

/// Backward-compatible alias for [`Conf`].
pub use Conf as Config;

/// Backward-compatible alias for [`Path`].
pub use Path as PathConfig;

/// Load configuration from a YAML file path.
pub fn load(path: &std::path::Path) -> Result<Config> {
    if !path.exists() {
        return Err(ConfError::NotFound(path.display().to_string()));
    }
    Conf::load_from_file(path)
}
