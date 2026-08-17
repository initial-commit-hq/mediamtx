//! Recording lifecycle tied to path ready transitions.

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use parking_lot::Mutex;
use rmtx_record::{RecordError, RecordFormat, Recorder};
use tracing::warn;

use crate::conf::PathConf;

/// Manages per-path recorder stubs while a path is ready and recording is enabled.
#[derive(Debug, Default)]
pub struct RecordSupervisor {
    recorders: Mutex<HashMap<String, Recorder>>,
}

impl RecordSupervisor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a placeholder recorder for `name` when `conf.record` is true.
    pub fn start(&self, name: &str, conf: &PathConf) -> Result<(), RecordError> {
        if !conf.record {
            return Ok(());
        }

        let format = RecordFormat::from_str(&conf.record_format)?;
        let mut recorder = Recorder::new(name, format, "1h", &conf.record_path);
        recorder.on_part(&[])?;

        let mut recorders = self.recorders.lock();
        if let Some(mut previous) = recorders.insert(name.to_owned(), recorder) {
            if let Err(err) = previous.close() {
                warn!(path = name, error = %err, "failed closing previous recorder");
            }
        }
        Ok(())
    }

    /// Stops and finalizes the recorder for `name`, if any.
    pub fn stop(&self, name: &str) -> Result<(), RecordError> {
        let mut recorders = self.recorders.lock();
        if let Some(mut recorder) = recorders.remove(name) {
            recorder.close()?;
        }
        Ok(())
    }

    /// Appends a media payload to the open segment for `name`, if recording is active.
    pub fn on_media_part(&self, name: &str, data: &[u8]) -> Result<(), RecordError> {
        let mut recorders = self.recorders.lock();
        if let Some(recorder) = recorders.get_mut(name) {
            recorder.on_part(data)?;
        }
        Ok(())
    }

    /// Returns the open segment path for `name` (tests / diagnostics).
    pub fn current_segment(&self, name: &str) -> Option<PathBuf> {
        self.recorders
            .lock()
            .get(name)
            .and_then(|rec| rec.current_segment().map(|p| p.to_path_buf()))
    }
}
