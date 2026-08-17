//! RTSP pull session: DESCRIBE + SETUP + PLAY feeding a [`StreamBus`].

use std::sync::Arc;

use rmtx_stream::Stream;
use tokio::sync::watch;
use tokio::task::JoinHandle;

#[cfg(feature = "retina")]
use bytes::Bytes;
#[cfg(feature = "retina")]
use rmtx_stream::{MediaUnit, Track};
#[cfg(feature = "retina")]
use tracing::{debug, info, warn};

use crate::source::RtspSourceError;

/// Active RTSP pull session (Retina PLAY) writing into a local stream bus.
#[derive(Debug)]
pub struct RtspPullSession {
    stream: Stream,
    cancel: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl RtspPullSession {
    /// Starts DESCRIBE + SETUP + PLAY for `url` and spawns a reader task.
    ///
    /// `on_bytes` is invoked with the payload byte count for each RTP packet or
    /// demuxed frame (best-effort).
    #[cfg(feature = "retina")]
    pub async fn start(
        url: &str,
        on_bytes: Arc<dyn Fn(u64) + Send + Sync>,
    ) -> Result<Self, RtspSourceError> {
        let parsed = url
            .parse()
            .map_err(|e| RtspSourceError::InvalidUrl(format!("{url}: {e}")))?;

        use retina::client::{Session, SessionOptions};

        let session = Session::describe(parsed, SessionOptions::default())
            .await
            .map_err(|e| RtspSourceError::Retina(e.to_string()))?;

        let tracks: Vec<Track> = session
            .streams()
            .iter()
            .enumerate()
            .map(|(i, s)| retina_stream_to_track(i, s))
            .collect();

        info!(
            url,
            tracks = tracks.len(),
            "retina DESCRIBE succeeded for pull session"
        );

        let stream = Stream::new(tracks);
        let stream_for_task = stream.clone();
        let url_owned = url.to_owned();
        let (cancel_tx, cancel_rx) = watch::channel(false);

        let task = tokio::spawn(async move {
            if let Err(err) =
                run_pull_loop(&url_owned, session, stream_for_task, cancel_rx, on_bytes).await
            {
                warn!(url = %url_owned, error = %err, "RTSP pull session ended");
            }
        });

        Ok(Self {
            stream,
            cancel: cancel_tx,
            task,
        })
    }

    /// Stub when the `retina` feature is disabled.
    #[cfg(not(feature = "retina"))]
    pub async fn start(
        url: &str,
        _on_bytes: Arc<dyn Fn(u64) + Send + Sync>,
    ) -> Result<Self, RtspSourceError> {
        Err(RtspSourceError::NotImplemented(format!(
            "RTSP pull session for {url} requires the `retina` feature"
        )))
    }

    /// Returns the stream bus fed by this session.
    pub fn stream(&self) -> &Stream {
        &self.stream
    }

    /// Requests teardown of the background reader.
    pub fn abort(&self) {
        let _ = self.cancel.send(true);
    }

    /// Waits for the background reader task to finish.
    pub async fn join(self) {
        let _ = self.task.await;
    }
}

#[cfg(feature = "retina")]
async fn run_pull_loop(
    url: &str,
    session: retina::client::Session<retina::client::Described>,
    stream: Stream,
    mut cancel: watch::Receiver<bool>,
    on_bytes: Arc<dyn Fn(u64) + Send + Sync>,
) -> Result<(), RtspSourceError> {
    use retina::client::{PlayOptions, SetupOptions};
    use retina::codec::FrameFormat;

    let stream_count = session.streams().len();
    let mut session = session;
    for i in 0..stream_count {
        session
            .setup(i, SetupOptions::default().frame_format(FrameFormat::SIMPLE))
            .await
            .map_err(|e| RtspSourceError::Retina(e.to_string()))?;
    }

    let playing = session
        .play(PlayOptions::default())
        .await
        .map_err(|e| RtspSourceError::Retina(e.to_string()))?;

    info!(url, streams = stream_count, "retina PLAY started");

    match playing.demuxed() {
        Ok(demuxed) => run_demux_loop(demuxed, stream, &mut cancel, on_bytes).await,
        Err(e) => {
            warn!(
                url,
                error = %e,
                "demux unavailable for one or more streams; reconnecting for raw RTP"
            );
            run_packet_pull(url, stream, cancel, on_bytes).await
        }
    }
}

#[cfg(feature = "retina")]
async fn run_demux_loop(
    mut demuxed: retina::client::Demuxed,
    stream: Stream,
    cancel: &mut watch::Receiver<bool>,
    on_bytes: Arc<dyn Fn(u64) + Send + Sync>,
) -> Result<(), RtspSourceError> {
    use futures::StreamExt;

    loop {
        tokio::select! {
            changed = cancel.changed() => {
                if changed.is_ok() && *cancel.borrow() {
                    debug!("RTSP pull session aborted (demux)");
                    return Ok(());
                }
            }
            item = demuxed.next() => {
                match item {
                    None => {
                        debug!("RTSP demux stream ended");
                        return Ok(());
                    }
                    Some(Ok(codec_item)) => {
                        if let Some((track_id, pts_us, payload)) = codec_item_to_unit(&codec_item) {
                            on_bytes(payload.len() as u64);
                            let unit = MediaUnit { track_id, pts_us, payload };
                            if let Err(rmtx_stream::StreamError::NoSubscribers) = stream.write(unit) {
                                debug!("no stream subscribers; media unit dropped");
                            }
                        }
                    }
                    Some(Err(e)) => {
                        return Err(RtspSourceError::Retina(e.to_string()));
                    }
                }
            }
        }
    }
}

#[cfg(feature = "retina")]
async fn run_packet_pull(
    url: &str,
    stream: Stream,
    mut cancel: watch::Receiver<bool>,
    on_bytes: Arc<dyn Fn(u64) + Send + Sync>,
) -> Result<(), RtspSourceError> {
    use futures::StreamExt;
    use retina::client::{PacketItem, PlayOptions, Session, SessionOptions, SetupOptions};
    use retina::codec::FrameFormat;

    let parsed = url
        .parse()
        .map_err(|e| RtspSourceError::InvalidUrl(format!("{url}: {e}")))?;

    let mut session = Session::describe(parsed, SessionOptions::default())
        .await
        .map_err(|e| RtspSourceError::Retina(e.to_string()))?;

    let stream_count = session.streams().len();
    for i in 0..stream_count {
        session
            .setup(i, SetupOptions::default().frame_format(FrameFormat::SIMPLE))
            .await
            .map_err(|e| RtspSourceError::Retina(e.to_string()))?;
    }

    let mut playing = session
        .play(PlayOptions::default())
        .await
        .map_err(|e| RtspSourceError::Retina(e.to_string()))?;

    info!(url, "retina raw RTP pull started (demux fallback)");

    loop {
        tokio::select! {
            changed = cancel.changed() => {
                if changed.is_ok() && *cancel.borrow() {
                    debug!("RTSP pull session aborted (raw RTP)");
                    return Ok(());
                }
            }
            item = playing.next() => {
                match item {
                    None => {
                        debug!("RTSP packet stream ended");
                        return Ok(());
                    }
                    Some(Ok(PacketItem::Rtp(pkt))) => {
                        on_bytes(pkt.raw().len() as u64);
                        let unit = MediaUnit {
                            track_id: pkt.stream_id() as u32,
                            pts_us: timestamp_to_pts_us(pkt.timestamp()),
                            payload: pkt.into_payload_bytes(),
                        };
                        if let Err(rmtx_stream::StreamError::NoSubscribers) = stream.write(unit) {
                            debug!("no stream subscribers; RTP unit dropped");
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        return Err(RtspSourceError::Retina(e.to_string()));
                    }
                }
            }
        }
    }
}

#[cfg(feature = "retina")]
fn codec_item_to_unit(item: &retina::codec::CodecItem) -> Option<(u32, u64, Bytes)> {
    use retina::codec::CodecItem;

    match item {
        CodecItem::VideoFrame(f) => Some((
            f.stream_id() as u32,
            timestamp_to_pts_us(f.timestamp()),
            Bytes::copy_from_slice(f.data()),
        )),
        CodecItem::AudioFrame(f) => Some((
            f.stream_id() as u32,
            timestamp_to_pts_us(f.timestamp()),
            Bytes::copy_from_slice(f.data()),
        )),
        CodecItem::MessageFrame(f) => Some((
            f.stream_id() as u32,
            timestamp_to_pts_us(f.timestamp()),
            Bytes::copy_from_slice(f.data()),
        )),
        CodecItem::Rtcp(_) => None,
        _ => None,
    }
}

#[cfg(feature = "retina")]
fn retina_stream_to_track(i: usize, stream: &retina::client::Stream) -> Track {
    Track {
        id: i as u32,
        codec: codec_label(stream.encoding_name(), stream.media()),
        clock_rate: stream.clock_rate_hz(),
    }
}

/// Best-effort codec label from Retina SDP fields.
#[cfg(feature = "retina")]
fn codec_label(encoding_name: &str, media: &str) -> String {
    match encoding_name {
        "h264" => "H264".into(),
        "h265" | "hevc" => "H265".into(),
        "jpeg" => "JPEG".into(),
        "mpeg4-generic" => "MPEG4-GENERIC".into(),
        "pcma" => "PCMA".into(),
        "pcmu" => "PCMU".into(),
        "opus" => "OPUS".into(),
        "vnd.onvif.metadata" => "ONVIF-METADATA".into(),
        other if media == "video" => other.to_ascii_uppercase(),
        other if media == "audio" => other.to_ascii_uppercase(),
        other => other.to_owned(),
    }
}

#[cfg(feature = "retina")]
fn timestamp_to_pts_us(ts: retina::Timestamp) -> u64 {
    let elapsed = ts.elapsed();
    let rate = ts.clock_rate().get() as u128;
    ((elapsed as u128).saturating_mul(1_000_000) / rate.max(1)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use rmtx_stream::{MediaUnit, Track};

    #[cfg(feature = "retina")]
    #[test]
    fn codec_label_maps_common_encodings() {
        assert_eq!(codec_label("h264", "video"), "H264");
        assert_eq!(codec_label("pcma", "audio"), "PCMA");
        assert_eq!(codec_label("vp8", "video"), "VP8");
    }

    #[cfg(feature = "retina")]
    #[test]
    fn timestamp_to_pts_us_converts_90khz() {
        use std::num::NonZeroU32;

        let ts = retina::Timestamp::new(1_000_000, NonZeroU32::new(90_000).unwrap(), 0).unwrap();
        assert_eq!(timestamp_to_pts_us(ts), 11_111_111);
    }

    #[tokio::test]
    async fn stream_bus_accepts_written_units() {
        let stream = Stream::new(vec![Track {
            id: 0,
            codec: "H264".to_owned(),
            clock_rate: 90_000,
        }]);
        let mut sub = stream.subscribe();

        stream
            .write(MediaUnit {
                track_id: 0,
                pts_us: 1,
                payload: Bytes::from_static(b"test"),
            })
            .unwrap();

        let unit = sub.recv().await.unwrap();
        assert_eq!(unit.payload, Bytes::from_static(b"test"));
    }

    #[tokio::test]
    async fn pull_session_without_retina_returns_not_implemented() {
        #[cfg(not(feature = "retina"))]
        {
            let err = RtspPullSession::start("rtsp://127.0.0.1/test", Arc::new(|_| {}))
                .await
                .unwrap_err();
            assert!(matches!(err, RtspSourceError::NotImplemented(_)));
        }
    }
}
