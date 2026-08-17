//! Per-path configuration (`pathDefaults` and `paths` entries).
//!
//! Go counterpart: `internal/conf/path.go`.

use serde::{Deserialize, Serialize};

use crate::always_available_track::AlwaysAvailableTrack;
use crate::de;

/// Path-level settings shared by `pathDefaults` and named entries under `paths`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Path {
    /// Path name; usually omitted in YAML and filled from the `paths` map key after load.
    pub name: String,

    // General
    pub source: String,
    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub source_on_demand: bool,
    pub source_on_demand_start_timeout: String,
    pub source_on_demand_close_after: String,

    // Always available
    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub always_available: bool,
    #[serde(default)]
    pub always_available_tracks: Vec<AlwaysAvailableTrack>,
    pub always_available_file: String,

    // Record
    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub record: bool,
    pub record_path: String,
    pub record_format: String,

    // Publisher source
    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub override_publisher: bool,

    // RTSP source
    pub rtsp_transport: String,

    // Hooks (see `rmtx/spec/hooks-env.json`)
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_init: String,
    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub run_on_init_restart: bool,
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_demand: String,
    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub run_on_demand_restart: bool,
    pub run_on_demand_start_timeout: String,
    pub run_on_demand_close_after: String,
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_un_demand: String,
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_ready: String,
    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub run_on_ready_restart: bool,
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_not_ready: String,
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_source_connect: String,
    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub run_on_source_connect_restart: bool,
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_source_disconnect: String,
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_read: String,
    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub run_on_read_restart: bool,
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_unread: String,
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_record_segment_create: String,
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_record_segment_complete: String,
}

impl Default for Path {
    fn default() -> Self {
        Self {
            name: String::new(),
            source: "publisher".to_owned(),
            source_on_demand: false,
            source_on_demand_start_timeout: "10s".to_owned(),
            source_on_demand_close_after: "10s".to_owned(),
            always_available: false,
            always_available_tracks: Vec::new(),
            always_available_file: String::new(),
            record: false,
            record_path: "./recordings/%path/%Y-%m-%d_%H-%M-%S-%f".to_owned(),
            record_format: "fmp4".to_owned(),
            override_publisher: true,
            rtsp_transport: "automatic".to_owned(),
            run_on_init: String::new(),
            run_on_init_restart: false,
            run_on_demand: String::new(),
            run_on_demand_restart: false,
            run_on_demand_start_timeout: "10s".to_owned(),
            run_on_demand_close_after: "10s".to_owned(),
            run_on_un_demand: String::new(),
            run_on_ready: String::new(),
            run_on_ready_restart: false,
            run_on_not_ready: String::new(),
            run_on_source_connect: String::new(),
            run_on_source_connect_restart: false,
            run_on_source_disconnect: String::new(),
            run_on_read: String::new(),
            run_on_read_restart: false,
            run_on_unread: String::new(),
            run_on_record_segment_create: String::new(),
            run_on_record_segment_complete: String::new(),
        }
    }
}
