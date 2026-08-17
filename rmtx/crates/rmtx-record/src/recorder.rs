use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use tracing::debug;

use crate::format::RecordFormat;
use crate::path_template::{add_format_extension, encode_segment_path};

/// Errors from the Phase 2 recording scaffold.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RecordError {
    #[error("unknown record format: {0}")]
    UnknownFormat(String),

    #[error("io error: {0}")]
    Io(String),
}

impl From<std::io::Error> for RecordError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// Phase 2 recorder stub that materializes placeholder segment files.
///
/// Real RTP/fMP4/MPEG-TS muxing is not implemented yet.
#[derive(Debug)]
pub struct Recorder {
    path_name: String,
    format: RecordFormat,
    segment_duration: String,
    path_template: String,
    current_segment: Option<PathBuf>,
}

impl Recorder {
    /// Creates a recorder for `path_name` using `path_template` and `segment_duration`.
    ///
    /// `segment_duration` is stored for later use when segment rollover is implemented.
    pub fn new(
        path_name: &str,
        format: RecordFormat,
        segment_duration: &str,
        path_template: &str,
    ) -> Self {
        Self {
            path_name: path_name.to_owned(),
            format,
            segment_duration: segment_duration.to_owned(),
            path_template: path_template.to_owned(),
            current_segment: None,
        }
    }

    /// Returns the configured segment duration string (e.g. `"1h"`).
    pub fn segment_duration(&self) -> &str {
        &self.segment_duration
    }

    /// Returns the path of the open placeholder segment, if any.
    pub fn current_segment(&self) -> Option<&Path> {
        self.current_segment.as_deref()
    }

    /// Accepts a media part; on first call creates a placeholder segment file.
    ///
    /// Subsequent non-empty payloads are appended raw (real muxing is Phase 2+).
    pub fn on_part(&mut self, data: &[u8]) -> Result<&Path, RecordError> {
        if self.current_segment.is_none() {
            self.open_placeholder_segment()?;
        }
        if !data.is_empty() {
            let path = self.current_segment.as_ref().expect("segment opened");
            let mut file = fs::OpenOptions::new().append(true).open(path)?;
            if self.format == RecordFormat::Mpegts {
                let segment = rmtx_mux::media_to_ts_segment(data, 0);
                file.write_all(&segment)?;
            } else {
                file.write_all(data)?;
            }
        }
        Ok(self.current_segment.as_deref().expect("segment opened"))
    }

    /// Finalizes the current segment placeholder.
    pub fn close(&mut self) -> Result<(), RecordError> {
        if let Some(path) = self.current_segment.take() {
            debug!(segment = %path.display(), "closed recording segment placeholder");
        }
        Ok(())
    }

    fn open_placeholder_segment(&mut self) -> Result<(), RecordError> {
        let start = Utc::now();
        let encoded = encode_segment_path(&self.path_template, &self.path_name, start);
        let segment_path = PathBuf::from(add_format_extension(&encoded, self.format));

        if let Some(parent) = segment_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = File::create(&segment_path)?;
        file.write_all(b"rmtx phase-2 recording placeholder\n")?;

        debug!(
            path = %self.path_name,
            segment = %segment_path.display(),
            format = ?self.format,
            "created recording segment placeholder"
        );

        self.current_segment = Some(segment_path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RecordFormat;

    #[test]
    fn on_part_creates_placeholder_mp4_with_path_substitution() {
        let dir = tempfile::tempdir().unwrap();
        let template = dir
            .path()
            .join("recordings/%path/%Y-%m-%d_%H-%M-%S-%f")
            .to_string_lossy()
            .into_owned();

        let mut recorder = Recorder::new("mycam", RecordFormat::Fmp4, "1h", &template);
        let segment = recorder.on_part(&[]).unwrap().to_path_buf();
        recorder.close().unwrap();

        assert!(segment.is_file());
        assert!(segment.to_string_lossy().contains("/recordings/mycam/"));
        assert!(segment.extension().is_some_and(|ext| ext == "mp4"));
    }

    #[test]
    fn on_part_appends_subsequent_payloads() {
        let dir = tempfile::tempdir().unwrap();
        let template = dir
            .path()
            .join("recordings/%path/segment")
            .to_string_lossy()
            .into_owned();

        let mut recorder = Recorder::new("live", RecordFormat::Fmp4, "1h", &template);
        recorder.on_part(&[]).unwrap();
        recorder.on_part(b"media-bytes").unwrap();
        let segment = recorder.current_segment().unwrap().to_path_buf();
        let contents = std::fs::read(&segment).unwrap();
        assert!(contents.windows(6).any(|w| w == b"media-"));
        recorder.close().unwrap();
    }

    #[test]
    fn mpegts_placeholder_uses_ts_extension() {
        let dir = tempfile::tempdir().unwrap();
        let template = dir
            .path()
            .join("recordings/%path/segment")
            .to_string_lossy()
            .into_owned();

        let mut recorder = Recorder::new("live", RecordFormat::Mpegts, "30m", &template);
        let segment = recorder.on_part(&[]).unwrap().to_path_buf();

        assert!(segment.extension().is_some_and(|ext| ext == "ts"));
        recorder.close().unwrap();
    }
}
