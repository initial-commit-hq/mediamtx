//! In-process micro-benchmarks (Rust-only baselines).

use std::time::Instant;

use bytes::Bytes;
use rmtx_stream::{MediaUnit, Stream, Track};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct StreamFanoutResult {
    pub subscribers: usize,
    pub units: u64,
    pub elapsed_secs: f64,
    pub units_per_sec: f64,
}

/// Measures StreamBus write + recv fan-out throughput.
pub async fn stream_fanout(subscribers: usize, units: u64) -> StreamFanoutResult {
    let stream = Stream::new(vec![Track {
        id: 1,
        codec: "H264".into(),
        clock_rate: 90_000,
    }]);

    let mut receivers = Vec::with_capacity(subscribers);
    for _ in 0..subscribers {
        receivers.push(stream.subscribe());
    }

    let payload = Bytes::from_static(&[0u8; 1400]);
    let started = Instant::now();

    for i in 0..units {
        let unit = MediaUnit {
            track_id: 1,
            pts_us: i * 33_333,
            payload: payload.clone(),
        };
        let _ = stream.write(unit);
        for rx in &mut receivers {
            let _ = rx.recv().await;
        }
    }

    let elapsed = started.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    StreamFanoutResult {
        subscribers,
        units,
        elapsed_secs,
        units_per_sec: if elapsed_secs > 0.0 {
            units as f64 / elapsed_secs
        } else {
            0.0
        },
    }
}
