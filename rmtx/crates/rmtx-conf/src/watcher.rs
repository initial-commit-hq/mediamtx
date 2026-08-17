//! Hot-reload watcher stub (mtime polling).
//!
//! Phase 1: detect file changes; later phases can wire this into the server loop.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::conf::Conf;
use crate::error::Result;

/// Polls a configuration file for changes via last-modified time.
#[derive(Debug, Clone)]
pub struct ConfWatcher {
    path: PathBuf,
    last_modified: Option<SystemTime>,
}

impl ConfWatcher {
    /// Watch the configuration file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            last_modified: None,
        }
    }

    /// Path being watched.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns `Ok(Some(conf))` when the file changed since the last successful poll,
    /// `Ok(None)` when unchanged, or an error on I/O / parse failure.
    pub fn poll(&mut self) -> Result<Option<Conf>> {
        let modified = std::fs::metadata(&self.path)?.modified()?;
        if self.last_modified == Some(modified) {
            return Ok(None);
        }
        self.last_modified = Some(modified);
        Conf::load_from_file(&self.path).map(Some)
    }

    /// Reset the cached modification time (e.g. after an external reload).
    pub fn reset(&mut self) {
        self.last_modified = None;
    }

    /// Records the current file mtime without loading (call after an initial load).
    pub fn seed(&mut self) -> Result<()> {
        self.last_modified = Some(std::fs::metadata(&self.path)?.modified()?);
        Ok(())
    }

    /// Returns the current mtime without loading or updating poll state.
    pub fn current_modified(&self) -> Result<SystemTime> {
        Ok(std::fs::metadata(&self.path)?.modified()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn poll_detects_rewrite() {
        let dir = std::env::temp_dir().join(format!("rmtx-conf-watcher-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mediamtx.yml");

        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "logLevel: info").unwrap();
        drop(file);

        let mut watcher = ConfWatcher::new(&path);
        let first = watcher.poll().unwrap();
        assert!(first.is_some());
        assert_eq!(first.unwrap().log_level, "info");

        let unchanged = watcher.poll().unwrap();
        assert!(unchanged.is_none());

        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&path, "logLevel: debug\n").unwrap();

        let second = watcher.poll().unwrap();
        assert!(second.is_some());
        assert_eq!(second.unwrap().log_level, "debug");

        let _ = std::fs::remove_dir_all(dir);
    }
}
