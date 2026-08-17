//! RTSP publisher ingest (ANNOUNCE/RECORD → StreamBus).

use std::sync::Arc;

use bytes::Bytes;
use rmtx_path::{PathManager, PathSource};
use rmtx_stream::{MediaUnit, StreamLookup};
use tokio::io::AsyncReadExt;
use tracing::debug;

use crate::server::RtspServerError;

fn parse_interleaved_frame(buf: &[u8]) -> Option<(usize, u8, &[u8])> {
    if buf.first() != Some(&b'$') || buf.len() < 4 {
        return None;
    }
    let channel = buf[1];
    let payload_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    let frame_len = 4 + payload_len;
    if buf.len() < frame_len {
        return None;
    }
    let rtp = &buf[4..frame_len];
    Some((frame_len, channel, rtp))
}

fn rtp_payload(packet: &[u8]) -> Option<&[u8]> {
    if packet.len() < 12 {
        return None;
    }
    let cc = (packet[0] & 0x0F) as usize;
    let mut off = 12 + cc * 4;
    if packet.len() <= off {
        return None;
    }
    if (packet[0] & 0x10) != 0 {
        if packet.len() < off + 4 {
            return None;
        }
        let ext_len = u16::from_be_bytes([packet[off + 2], packet[off + 3]]) as usize;
        off += 4 + ext_len * 4;
    }
    if (packet[0] & 0x20) != 0 {
        return None;
    }
    packet.get(off..)
}

/// Handles ANNOUNCE: attach publisher stream and mark path ready.
pub fn handle_announce(
    path: &str,
    paths: &Arc<PathManager>,
    streams: &dyn StreamLookup,
) -> Result<Arc<rmtx_stream::Stream>, RtspServerError> {
    if paths.get(path).is_err() {
        return Err(RtspServerError::InvalidRequest("path not found".into()));
    }
    let stream = streams
        .attach_publisher(path)
        .ok_or_else(|| RtspServerError::InvalidRequest("publish not supported".into()))?;
    paths
        .set_ready(
            path,
            true,
            Some(PathSource {
                source_type: "rtspSession".into(),
                id: String::new(),
            }),
        )
        .map_err(|e| RtspServerError::InvalidRequest(e.to_string()))?;
    streams.after_publisher_attach(path, &stream, paths);
    Ok(stream)
}

/// Reads interleaved RTP from `reader` and publishes payloads to the path stream bus.
pub async fn ingest_publish_interleaved(
    path: &str,
    paths: Arc<PathManager>,
    streams: Arc<dyn StreamLookup>,
    reader: &mut tokio::net::tcp::OwnedReadHalf,
) -> Result<(), RtspServerError> {
    let mut buf = Vec::new();
    let mut scratch = [0u8; 4096];
    let mut pts_us: u64 = 0;

    loop {
        let n = reader.read(&mut scratch).await.map_err(RtspServerError::Io)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&scratch[..n]);

        while let Some((consumed, channel, rtp)) = parse_interleaved_frame(&buf) {
            let rtp_owned = rtp.to_vec();
            buf.drain(..consumed);
            let _ = channel;
            if let Some(payload) = rtp_payload(&rtp_owned) {
                if let Some(stream) = streams.stream(path) {
                    let unit = MediaUnit {
                        track_id: 0,
                        pts_us,
                        payload: Bytes::copy_from_slice(payload),
                    };
                    let _ = stream.write(unit);
                    let _ = paths.add_bytes_received(path, payload.len() as u64);
                    pts_us = pts_us.saturating_add(33_333);
                }
            }
        }

        if buf.len() > 256 * 1024 {
            buf.clear();
        }
    }

    debug!(path, "RTSP publish ingest ended");
    Ok(())
}
