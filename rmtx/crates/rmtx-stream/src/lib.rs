//! Shared media stream fan-out.
//!
//! Go counterpart: `internal/stream`.
//!
//! Phase 1: track metadata plus a broadcast bus for [`MediaUnit`] fan-out.

#![forbid(unsafe_code)]

mod lookup;
mod placeholder;

use std::sync::Arc;

use bytes::Bytes;
use thiserror::Error;
use tokio::sync::broadcast;

pub use lookup::{NoStreamLookup, StreamLookup};
pub use placeholder::{
    placeholder_from_tracks, placeholder_payload, spawn_placeholder_ticker,
    PLACEHOLDER_TICK_INTERVAL,
};

const BUS_CAPACITY: usize = 256;

/// Static track description for a stream (codec label and RTP clock rate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: u32,
    pub codec: String,
    pub clock_rate: u32,
}

/// One encoded media sample routed through the stream bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaUnit {
    pub track_id: u32,
    /// Presentation timestamp in microseconds.
    pub pts_us: u64,
    pub payload: Bytes,
}

/// Errors returned when publishing to a stream bus.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StreamError {
    #[error("no active subscribers")]
    NoSubscribers,
}

/// Receive-side handle for a [`Stream`] subscription.
pub struct StreamReceiver {
    rx: broadcast::Receiver<MediaUnit>,
}

impl StreamReceiver {
    /// Waits for the next media unit from the bus.
    pub async fn recv(&mut self) -> Result<MediaUnit, broadcast::error::RecvError> {
        self.rx.recv().await
    }
}

/// Shared media stream with metadata and a fan-out broadcast bus.
#[derive(Debug, Clone)]
pub struct Stream {
    inner: Arc<StreamInner>,
}

/// Alias for [`Stream`] (matches Go "stream bus" terminology).
pub type StreamBus = Stream;

#[derive(Debug)]
struct StreamInner {
    tracks: Vec<Track>,
    bus: broadcast::Sender<MediaUnit>,
}

impl Stream {
    /// Creates a stream with the given track set and an empty subscriber bus.
    pub fn new(tracks: Vec<Track>) -> Self {
        let (bus, _) = broadcast::channel(BUS_CAPACITY);
        Self {
            inner: Arc::new(StreamInner { tracks, bus }),
        }
    }

    /// Returns the configured tracks (immutable for the lifetime of the stream).
    pub fn tracks(&self) -> &[Track] {
        &self.inner.tracks
    }

    /// Publishes a media unit to all current subscribers.
    pub fn write(&self, unit: MediaUnit) -> Result<(), StreamError> {
        match self.inner.bus.send(unit) {
            Ok(_) => Ok(()),
            Err(broadcast::error::SendError(_)) => Err(StreamError::NoSubscribers),
        }
    }

    /// Subscribes to media units; each subscriber receives every unit written after subscribe.
    pub fn subscribe(&self) -> StreamReceiver {
        StreamReceiver {
            rx: self.inner.bus.subscribe(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fan_out_to_two_subscribers() {
        let stream = Stream::new(vec![Track {
            id: 1,
            codec: "H264".to_owned(),
            clock_rate: 90_000,
        }]);

        let mut sub_a = stream.subscribe();
        let mut sub_b = stream.subscribe();

        let unit = MediaUnit {
            track_id: 1,
            pts_us: 1_000_000,
            payload: Bytes::from_static(b"\x00\x00\x00\x01\x65"),
        };

        stream.write(unit.clone()).unwrap();

        assert_eq!(sub_a.recv().await.unwrap(), unit);
        assert_eq!(sub_b.recv().await.unwrap(), unit);
    }

    #[test]
    fn write_without_subscribers_returns_error() {
        let stream = Stream::new(vec![]);
        let err = stream
            .write(MediaUnit {
                track_id: 0,
                pts_us: 0,
                payload: Bytes::new(),
            })
            .unwrap_err();
        assert_eq!(err, StreamError::NoSubscribers);
    }
}
