//! Per-path ffmpeg subprocess fallback
//!
//! Spawns ffmpeg with an argv vector (never a shell). Resource limits are TODO.
//!
//! Fallback strategy (from port brief): when Retina cannot handle a source (UDP reorder,
//! ONVIF backchannel/replay, minimal RTCP SR), spawn ffmpeg to pull the remote URL and
//! republish to a local RTSP endpoint that rmtx ingests as a publisher.

#![forbid(unsafe_code)]

use std::process::{Child, Command, Stdio};

use thiserror::Error;
use tokio::process::Command as AsyncCommand;
use tracing::debug;

/// ffmpeg pull → local RTSP republish command builder.
#[derive(Debug, Clone)]
pub struct FfmpegPull {
    pub url: String,
    /// Local RTSP publish URL (placeholder until path manager assigns one).
    pub output_rtsp: String,
}

impl FfmpegPull {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            output_rtsp: "rtsp://127.0.0.1:8554/_ffmpeg_fallback".into(),
        }
    }

    pub fn with_output_rtsp(mut self, output_rtsp: impl Into<String>) -> Self {
        self.output_rtsp = output_rtsp.into();
        self
    }

    /// argv for `ffmpeg -rtsp_transport tcp -i <url> -c copy -f rtsp <output>` (no shell).
    pub fn argv(&self) -> Vec<String> {
        vec![
            "ffmpeg".into(),
            "-hide_banner".into(),
            "-loglevel".into(),
            "warning".into(),
            "-rtsp_transport".into(),
            "tcp".into(),
            "-i".into(),
            self.url.clone(),
            "-c".into(),
            "copy".into(),
            "-f".into(),
            "rtsp".into(),
            self.output_rtsp.clone(),
        ]
    }

    /// argv for MPEG-TS to stdout (pipe ingest into StreamBus).
    pub fn argv_mpegts_stdout(&self) -> Vec<String> {
        vec![
            "ffmpeg".into(),
            "-hide_banner".into(),
            "-loglevel".into(),
            "warning".into(),
            "-rtsp_transport".into(),
            "tcp".into(),
            "-i".into(),
            self.url.clone(),
            "-c".into(),
            "copy".into(),
            "-f".into(),
            "mpegts".into(),
            "pipe:1".into(),
        ]
    }
}

/// ffmpeg spawn errors.
#[derive(Debug, Error)]
pub enum FfmpegError {
    #[error("failed to spawn ffmpeg: {0}")]
    Spawn(#[from] std::io::Error),
}

/// Spawn ffmpeg using an explicit argv array (no shell).
///
/// TODO: apply resource limits (rlimit CPU/memory, cgroup on Linux, job objects on Windows).
pub fn spawn(argv: &[String]) -> Result<Child, FfmpegError> {
    debug!(?argv, "spawning ffmpeg");
    let program = argv.first().map(String::as_str).unwrap_or("ffmpeg");
    let mut cmd = Command::new(program);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    Ok(cmd.spawn()?)
}

/// Async spawn with stdout piped for MPEG-TS ingest loops.
pub async fn spawn_async_stdout(argv: &[String]) -> Result<tokio::process::Child, FfmpegError> {
    debug!(?argv, "spawning ffmpeg (async, stdout pipe)");
    let program = argv.first().map(String::as_str).unwrap_or("ffmpeg");
    let mut cmd = AsyncCommand::new(program);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(cmd.spawn()?)
}

/// Async spawn via [`tokio::process::Command`] (argv only, no shell).
pub async fn spawn_async(argv: &[String]) -> Result<tokio::process::Child, FfmpegError> {
    debug!(?argv, "spawning ffmpeg (async)");
    let program = argv.first().map(String::as_str).unwrap_or("ffmpeg");
    let mut cmd = AsyncCommand::new(program);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    Ok(cmd.spawn()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_is_copy_rtsp_placeholder() {
        let pull = FfmpegPull::new("rtsp://camera/live");
        let argv = pull.argv();
        assert_eq!(argv[0], "ffmpeg");
        assert!(argv.contains(&"-rtsp_transport".into()));
        assert!(argv.contains(&"tcp".into()));
        assert!(argv.contains(&"-i".into()));
        assert!(argv.contains(&"rtsp://camera/live".into()));
        assert!(argv.contains(&"-c".into()));
        assert!(argv.contains(&"copy".into()));
        assert!(argv.contains(&"-f".into()));
        assert!(argv.contains(&"rtsp".into()));
    }

    #[test]
    fn argv_mpegts_stdout_uses_pipe() {
        let pull = FfmpegPull::new("rtsp://camera/live");
        let argv = pull.argv_mpegts_stdout();
        assert!(argv.contains(&"pipe:1".into()));
        assert!(argv.contains(&"mpegts".into()));
    }

    #[tokio::test]
    async fn spawn_missing_binary_returns_error_without_panic() {
        let argv = vec![
            "definitely-not-a-real-ffmpeg-binary-rmtx-test".into(),
            "-version".into(),
        ];
        let err = spawn_async(&argv).await.unwrap_err();
        match err {
            FfmpegError::Spawn(io_err) => {
                assert_eq!(io_err.kind(), std::io::ErrorKind::NotFound);
            }
        }
    }
}
