//! Reads MPEG-TS chunks from ffmpeg stdout into a [`Stream`].

use std::sync::Arc;

use bytes::Bytes;
use rmtx_stream::{MediaUnit, Stream, Track};
use tokio::io::AsyncReadExt;
use tokio::process::ChildStdout;
use tracing::warn;

const READ_CHUNK: usize = 4096;

/// Pumps stdout from ffmpeg into `stream` until EOF or error.
pub async fn pump_mpegts_stdout(path: &str, mut stdout: ChildStdout, stream: Arc<Stream>) {
    let mut buf = vec![0u8; READ_CHUNK];
    let mut pts_us: u64 = 0;
    loop {
        match stdout.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let unit = MediaUnit {
                    track_id: 0,
                    pts_us,
                    payload: Bytes::copy_from_slice(&buf[..n]),
                };
                let _ = stream.write(unit);
                pts_us = pts_us.saturating_add(40_000);
            }
            Err(err) => {
                warn!(path, error = %err, "ffmpeg pipe read ended");
                break;
            }
        }
    }
}

/// Default single-track H264 bus for pipe ingest.
pub fn ffmpeg_ingest_stream() -> Stream {
    Stream::new(vec![Track {
        id: 0,
        codec: "H264".into(),
        clock_rate: 90_000,
    }])
}
