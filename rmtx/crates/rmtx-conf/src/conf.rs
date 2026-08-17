//! Top-level MediaMTX configuration.
//!
//! Go counterpart: `internal/conf/conf.go`.

use std::collections::HashMap;
use std::path::Path as FsPath;

use serde::{Deserialize, Serialize};

use crate::auth::AuthInternalUser;
use crate::de;
use crate::error::Result;
use crate::path::Path;

/// MediaMTX-compatible server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Conf {
    pub log_level: String,

    #[serde(deserialize_with = "de::null_string")]
    pub run_on_connect: String,
    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub run_on_connect_restart: bool,
    #[serde(deserialize_with = "de::null_string")]
    pub run_on_disconnect: String,

    pub auth_method: String,
    pub auth_internal_users: Vec<AuthInternalUser>,
    #[serde(default, deserialize_with = "de::null_string")]
    pub auth_jwt_jwks: String,
    #[serde(default, deserialize_with = "de::null_string")]
    pub auth_jwt_claim_key: String,
    #[serde(default, deserialize_with = "de::null_string")]
    pub auth_jwt_issuer: String,
    #[serde(default, deserialize_with = "de::null_string")]
    pub auth_jwt_audience: String,

    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub api: bool,
    pub api_address: String,

    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub rtsp: bool,
    pub rtsp_address: String,

    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub rtmp: bool,
    pub rtmp_address: String,

    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub hls: bool,
    pub hls_address: String,

    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub webrtc: bool,
    pub webrtc_address: String,

    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub metrics: bool,
    pub metrics_address: String,

    #[serde(deserialize_with = "crate::bool_yaml::deserialize")]
    pub playback: bool,
    pub playback_address: String,

    pub path_defaults: Path,
    pub paths: HashMap<String, Path>,
}

impl Default for Conf {
    fn default() -> Self {
        Self {
            log_level: "info".to_owned(),
            run_on_connect: String::new(),
            run_on_connect_restart: false,
            run_on_disconnect: String::new(),
            auth_method: "internal".to_owned(),
            auth_internal_users: Vec::new(),
            auth_jwt_jwks: String::new(),
            auth_jwt_claim_key: "mediamtx_permissions".to_owned(),
            auth_jwt_issuer: String::new(),
            auth_jwt_audience: String::new(),
            api: false,
            api_address: ":9997".to_owned(),
            rtsp: true,
            rtsp_address: ":8554".to_owned(),
            rtmp: true,
            rtmp_address: ":1935".to_owned(),
            hls: true,
            hls_address: ":8888".to_owned(),
            webrtc: true,
            webrtc_address: ":8889".to_owned(),
            metrics: false,
            metrics_address: ":9998".to_owned(),
            playback: false,
            playback_address: ":9996".to_owned(),
            path_defaults: Path::default(),
            paths: HashMap::new(),
        }
    }
}

impl Conf {
    /// Load configuration from a YAML file at `path`.
    pub fn load_from_file(path: impl AsRef<FsPath>) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        Self::load_str(&contents)
    }

    /// Parse configuration from a YAML string.
    pub fn load_str(yaml: &str) -> Result<Self> {
        let mut conf: Conf = serde_yaml::from_str(yaml)?;
        conf.normalize();
        Ok(conf)
    }

    fn normalize(&mut self) {
        for (name, path) in &mut self.paths {
            path.name.clone_from(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root_mediamtx_yml() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mediamtx.yml")
    }

    #[test]
    fn default_values_match_go_baseline() {
        let conf = Conf::default();
        assert_eq!(conf.api, false);
        assert_eq!(conf.api_address, ":9997");
        assert_eq!(conf.rtsp, true);
        assert_eq!(conf.rtsp_address, ":8554");
        assert_eq!(conf.playback, false);
        assert_eq!(conf.playback_address, ":9996");
        assert!(conf.paths.is_empty());
        assert_eq!(conf.path_defaults.source, "publisher");
    }

    #[test]
    fn load_minimal_yaml() {
        let yaml = r#"
logLevel: debug
api: true
paths:
  cam1:
    source: rtsp://example/cam
    record: true
"#;
        let conf = Conf::load_str(yaml).unwrap();
        assert_eq!(conf.log_level, "debug");
        assert!(conf.api);
        assert_eq!(conf.rtsp_address, ":8554");
        let cam1 = conf.paths.get("cam1").unwrap();
        assert_eq!(cam1.name, "cam1");
        assert_eq!(cam1.source, "rtsp://example/cam");
        assert!(cam1.record);
    }

    #[test]
    fn load_repo_mediamtx_yml() {
        let path = repo_root_mediamtx_yml();
        assert!(
            path.is_file(),
            "expected mediamtx.yml at {}",
            path.display()
        );

        let conf = Conf::load_from_file(&path).unwrap();
        assert_eq!(conf.log_level, "info");
        assert_eq!(conf.auth_method, "internal");
        assert_eq!(conf.api, false);
        assert_eq!(conf.api_address, ":9997");
        assert_eq!(conf.rtsp, true);
        assert_eq!(conf.rtsp_address, ":8554");
        assert_eq!(conf.rtmp_address, ":1935");
        assert_eq!(conf.path_defaults.source, "publisher");
        assert_eq!(conf.path_defaults.rtsp_transport, "automatic");
        assert!(conf.paths.contains_key("all_others"));
        assert_eq!(conf.paths["all_others"].name, "all_others");
    }

    #[test]
    fn yes_no_booleans() {
        let yaml = r#"
api: yes
rtsp: no
paths:
  cam1:
    record: yes
"#;
        let conf = Conf::load_str(yaml).unwrap();
        assert!(conf.api);
        assert!(!conf.rtsp);
        assert!(conf.paths["cam1"].record);
    }

    #[test]
    fn null_hook_fields_deserialize_as_empty() {
        let yaml = r#"
runOnConnect:
runOnDisconnect:
pathDefaults:
  runOnInit:
  runOnDemand:
paths:
  p:
    runOnReady:
"#;
        let conf = Conf::load_str(yaml).unwrap();
        assert_eq!(conf.run_on_connect, "");
        assert_eq!(conf.run_on_disconnect, "");
        assert_eq!(conf.path_defaults.run_on_init, "");
        assert_eq!(conf.paths["p"].run_on_ready, "");
    }
}
