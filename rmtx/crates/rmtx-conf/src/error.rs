//! Configuration load errors.

use thiserror::Error;

/// Error type for [`crate::Conf`] loading.
#[derive(Debug, Error)]
pub enum ConfError {
    /// Configuration file does not exist.
    #[error("config file not found: {0}")]
    NotFound(String),

    /// I/O error while reading the configuration file.
    #[error("failed to read config: {0}")]
    Io(#[from] std::io::Error),

    /// YAML parse error.
    #[error("invalid config: {0}")]
    Parse(#[from] serde_yaml::Error),
}

/// Result alias for configuration operations.
pub type Result<T> = std::result::Result<T, ConfError>;
