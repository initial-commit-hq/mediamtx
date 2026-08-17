//! Always-available placeholder stream helpers.
//!
//! Go counterpart: `internal/stream` `mediasFromAlwaysAvailableTracks`.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use rmtx_conf::AlwaysAvailableTrack;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::{MediaUnit, Stream, Track};

/// Interval between synthetic placeholder media units (~10 Hz).
pub const PLACEHOLDER_TICK_INTERVAL: Duration = Duration::from_millis(100);

/// Minimal encoded payload for a placeholder unit on `codec`.
pub fn placeholder_payload(codec: &str) -> Bytes {
    match codec {
        "H264" => Bytes::from_static(&[0x00, 0x00, 0x00, 0x01, 0x09, 0x10]),
        "H265" => Bytes::from_static(&[0x00, 0x00, 0x00, 0x01, 0x46, 0x01, 0x10]),
        "Opus" => Bytes::from_static(&[0xF8, 0xFF, 0xFE]),
        "MPEG4Audio" => Bytes::from_static(&[0x01, 0x18, 0x20, 0x07]),
        "G711" => Bytes::from_static(&[0xFF]),
        "LPCM" => Bytes::from_static(&[0x00]),
        "VP9" | "AV1" => Bytes::from_static(&[0x01, 0x02, 0x03]),
        _ => Bytes::from_static(&[0x00, 0x00, 0x00, 0x01, 0x09, 0x10]),
    }
}

/// Spawns a background task that writes tiny [`MediaUnit`]s to `stream` periodically.
///
/// Returns a [`JoinHandle`] that callers should [`JoinHandle::abort`] when the
/// placeholder is replaced by a real source or the path is removed.
pub fn spawn_placeholder_ticker(stream: Arc<Stream>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut pts_us: u64 = 0;
        let tick_us = PLACEHOLDER_TICK_INTERVAL.as_micros() as u64;
        let mut ticker = tokio::time::interval(PLACEHOLDER_TICK_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;

            for track in stream.tracks() {
                let unit = MediaUnit {
                    track_id: track.id,
                    pts_us,
                    payload: placeholder_payload(&track.codec),
                };
                let _ = stream.write(unit);
            }

            pts_us = pts_us.saturating_add(tick_us);
        }
    })
}

fn codec_clock_rate(codec: &str, track: &AlwaysAvailableTrack) -> u32 {
    match codec {
        "MPEG4Audio" | "G711" | "LPCM" if track.sample_rate > 0 => track.sample_rate,
        "Opus" => 48_000,
        "H264" | "H265" | "VP9" | "AV1" => 90_000,
        _ => 90_000,
    }
}

/// Builds a black/silent placeholder stream from configured always-available tracks.
///
/// When no tracks are configured, defaults to a single H264 video track (MediaMTX
/// requires at least one track or an always-available file).
pub fn placeholder_from_tracks(tracks: &[AlwaysAvailableTrack]) -> Stream {
    if tracks.is_empty() {
        return Stream::new(vec![Track {
            id: 0,
            codec: "H264".into(),
            clock_rate: 90_000,
        }]);
    }

    let stream_tracks = tracks
        .iter()
        .enumerate()
        .map(|(id, track)| Track {
            id: id as u32,
            codec: track.codec.clone(),
            clock_rate: codec_clock_rate(&track.codec, track),
        })
        .collect();

    Stream::new(stream_tracks)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn placeholder_ticker_emits_unit_within_one_second() {
        let stream = Arc::new(placeholder_from_tracks(&[]));
        let mut sub = stream.subscribe();
        let ticker = spawn_placeholder_ticker(Arc::clone(&stream));

        let unit = tokio::time::timeout(Duration::from_secs(1), sub.recv())
            .await
            .expect("timed out waiting for placeholder unit")
            .expect("stream bus closed");

        assert_eq!(unit.track_id, 0);
        assert_eq!(unit.payload.as_ref(), placeholder_payload("H264").as_ref());

        ticker.abort();
    }

    #[test]
    fn default_placeholder_is_h264() {
        let stream = placeholder_from_tracks(&[]);
        assert_eq!(stream.tracks().len(), 1);
        assert_eq!(stream.tracks()[0].codec, "H264");
    }

    #[test]
    fn builds_from_configured_codecs() {
        let stream = placeholder_from_tracks(&[
            AlwaysAvailableTrack {
                codec: "H264".into(),
                ..AlwaysAvailableTrack::default()
            },
            AlwaysAvailableTrack {
                codec: "Opus".into(),
                ..AlwaysAvailableTrack::default()
            },
        ]);
        assert_eq!(stream.tracks().len(), 2);
        assert_eq!(stream.tracks()[0].codec, "H264");
        assert_eq!(stream.tracks()[1].codec, "Opus");
        assert_eq!(stream.tracks()[1].clock_rate, 48_000);
    }
}
