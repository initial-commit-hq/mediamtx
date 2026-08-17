//! `alwaysAvailableTracks` entries.
//!
//! Go counterpart: `internal/conf/always_available_track.go`.

use serde::{Deserialize, Serialize};

/// One track in an always-available placeholder description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AlwaysAvailableTrack {
    pub codec: String,
    pub sample_rate: u32,
    pub channel_count: u32,
    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub mu_law: bool,
}

impl Default for AlwaysAvailableTrack {
    fn default() -> Self {
        Self {
            codec: String::new(),
            sample_rate: 0,
            channel_count: 0,
            mu_law: false,
        }
    }
}
