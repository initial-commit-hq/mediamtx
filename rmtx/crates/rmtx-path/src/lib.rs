//! Path registry and runtime status for the Control API.
//!
//! Go counterpart: `internal/core` path manager.
//!
//! Phase 1: configured-path snapshots + thread-safe runtime state (`list` / `get`).

#![forbid(unsafe_code)]

mod conf;
mod manager;
mod record;
mod status;

pub use conf::PathConf;
pub use manager::{PathError, PathManager};
pub use record::RecordSupervisor;
pub use status::{Path, PathReader, PathSource};

impl PathManager {
    /// Seeds configured paths from [`rmtx_conf::Conf`].
    pub fn load_from_conf(&self, conf: &rmtx_conf::Conf) {
        for (name, path) in &conf.paths {
            self.upsert_conf(PathConf::from_conf_entry(name, &conf.path_defaults, path));
        }
        tracing::info!(
            count = conf.paths.len(),
            "path manager loaded paths from conf"
        );
    }
}
