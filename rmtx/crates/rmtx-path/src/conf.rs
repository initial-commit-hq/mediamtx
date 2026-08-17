//! Snapshot of path configuration (from `rmtx-conf` / OpenAPI `PathConf`).

use serde::{Deserialize, Serialize};

use rmtx_conf::AlwaysAvailableTrack;
use rmtx_hooks::PathReadyHooks;

/// Path configuration snapshot held by [`crate::PathManager`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathConf {
    pub name: String,
    pub always_available: bool,
    pub always_available_tracks: Vec<AlwaysAvailableTrack>,
    pub run_on_ready: String,
    pub run_on_ready_restart: bool,
    pub run_on_not_ready: String,
    pub record: bool,
    pub record_path: String,
    pub record_format: String,
}

impl PathConf {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    pub fn ready_hooks(&self) -> PathReadyHooks {
        PathReadyHooks {
            run_on_ready: self.run_on_ready.clone(),
            run_on_ready_restart: self.run_on_ready_restart,
            run_on_not_ready: self.run_on_not_ready.clone(),
        }
    }

    /// Builds a snapshot from merged `pathDefaults` + named path entry.
    pub fn from_conf_entry(
        name: impl Into<String>,
        defaults: &rmtx_conf::Path,
        path: &rmtx_conf::Path,
    ) -> Self {
        let name = name.into();
        let (run_on_ready, run_on_ready_restart) = effective_ready(defaults, path);
        let run_on_not_ready = effective_string(&path.run_on_not_ready, &defaults.run_on_not_ready);

        Self {
            name,
            always_available: effective_always_available(defaults, path),
            always_available_tracks: effective_always_available_tracks(defaults, path),
            run_on_ready,
            run_on_ready_restart,
            run_on_not_ready,
            record: effective_record(defaults, path),
            record_path: effective_record_path(defaults, path),
            record_format: effective_record_format(defaults, path),
        }
    }
}

impl Default for PathConf {
    fn default() -> Self {
        Self {
            name: String::new(),
            always_available: false,
            always_available_tracks: Vec::new(),
            run_on_ready: String::new(),
            run_on_ready_restart: false,
            run_on_not_ready: String::new(),
            record: false,
            record_path: "./recordings/%path/%Y-%m-%d_%H-%M-%S-%f".to_owned(),
            record_format: "fmp4".to_owned(),
        }
    }
}

fn effective_string(path_val: &str, default_val: &str) -> String {
    if path_val.is_empty() {
        default_val.to_owned()
    } else {
        path_val.to_owned()
    }
}

fn effective_ready(defaults: &rmtx_conf::Path, path: &rmtx_conf::Path) -> (String, bool) {
    if path.run_on_ready.is_empty() {
        (defaults.run_on_ready.clone(), defaults.run_on_ready_restart)
    } else {
        (path.run_on_ready.clone(), path.run_on_ready_restart)
    }
}

fn path_factory() -> rmtx_conf::Path {
    rmtx_conf::Path::default()
}

fn effective_record(defaults: &rmtx_conf::Path, path: &rmtx_conf::Path) -> bool {
    if path.record {
        return true;
    }
    defaults.record && !path_has_record_override(path)
}

fn path_has_record_override(path: &rmtx_conf::Path) -> bool {
    let factory = path_factory();
    path.record_path != factory.record_path || path.record_format != factory.record_format
}

fn effective_record_path(defaults: &rmtx_conf::Path, path: &rmtx_conf::Path) -> String {
    let factory = path_factory();
    if path.record_path != factory.record_path || defaults.record_path == factory.record_path {
        path.record_path.clone()
    } else {
        defaults.record_path.clone()
    }
}

fn effective_record_format(defaults: &rmtx_conf::Path, path: &rmtx_conf::Path) -> String {
    let factory = path_factory();
    if path.record_format != factory.record_format
        || defaults.record_format == factory.record_format
    {
        path.record_format.clone()
    } else {
        defaults.record_format.clone()
    }
}

fn effective_always_available(defaults: &rmtx_conf::Path, path: &rmtx_conf::Path) -> bool {
    path.always_available || defaults.always_available
}

fn effective_always_available_tracks(
    defaults: &rmtx_conf::Path,
    path: &rmtx_conf::Path,
) -> Vec<AlwaysAvailableTrack> {
    if !path.always_available_tracks.is_empty() {
        return path.always_available_tracks.clone();
    }
    if path.always_available || defaults.always_available {
        defaults.always_available_tracks.clone()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherits_ready_hooks_from_defaults() {
        let mut defaults = rmtx_conf::Path::default();
        defaults.run_on_ready = "echo default-ready".into();
        defaults.run_on_ready_restart = true;
        defaults.run_on_not_ready = "echo default-not-ready".into();

        let path = rmtx_conf::Path::default();
        let conf = PathConf::from_conf_entry("live", &defaults, &path);
        assert_eq!(conf.run_on_ready, "echo default-ready");
        assert!(conf.run_on_ready_restart);
        assert_eq!(conf.run_on_not_ready, "echo default-not-ready");
    }

    #[test]
    fn path_entry_overrides_defaults() {
        let defaults = rmtx_conf::Path::default();
        let mut path = rmtx_conf::Path::default();
        path.run_on_ready = "echo path-ready".into();
        path.run_on_ready_restart = true;
        path.run_on_not_ready = "echo path-not-ready".into();

        let conf = PathConf::from_conf_entry("live", &defaults, &path);
        assert_eq!(conf.run_on_ready, "echo path-ready");
        assert!(conf.run_on_ready_restart);
        assert_eq!(conf.run_on_not_ready, "echo path-not-ready");
    }

    #[test]
    fn inherits_record_settings_from_defaults() {
        let mut defaults = rmtx_conf::Path::default();
        defaults.record = true;
        defaults.record_path = "./rec/%path/segment".into();
        defaults.record_format = "mpegts".into();

        let path = rmtx_conf::Path::default();
        let conf = PathConf::from_conf_entry("live", &defaults, &path);
        assert!(conf.record);
        assert_eq!(conf.record_path, "./rec/%path/segment");
        assert_eq!(conf.record_format, "mpegts");
    }

    #[test]
    fn inherits_always_available_from_defaults() {
        let mut defaults = rmtx_conf::Path::default();
        defaults.always_available = true;
        defaults.always_available_tracks = vec![AlwaysAvailableTrack {
            codec: "H264".into(),
            ..AlwaysAvailableTrack::default()
        }];

        let path = rmtx_conf::Path::default();
        let conf = PathConf::from_conf_entry("live", &defaults, &path);
        assert!(conf.always_available);
        assert_eq!(conf.always_available_tracks[0].codec, "H264");
    }

    #[test]
    fn path_entry_overrides_always_available_tracks() {
        let mut defaults = rmtx_conf::Path::default();
        defaults.always_available_tracks = vec![AlwaysAvailableTrack {
            codec: "H265".into(),
            ..AlwaysAvailableTrack::default()
        }];

        let mut path = rmtx_conf::Path::default();
        path.always_available = true;
        path.always_available_tracks = vec![AlwaysAvailableTrack {
            codec: "H264".into(),
            ..AlwaysAvailableTrack::default()
        }];

        let conf = PathConf::from_conf_entry("live", &defaults, &path);
        assert!(conf.always_available);
        assert_eq!(conf.always_available_tracks[0].codec, "H264");
    }
}
