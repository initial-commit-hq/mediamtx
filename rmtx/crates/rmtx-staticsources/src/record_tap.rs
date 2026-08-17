//! Subscribes to a path [`Stream`] and appends [`MediaUnit`] payloads to the recorder.

use std::sync::Arc;

use rmtx_path::PathManager;
use rmtx_stream::Stream;
use tokio::task::JoinHandle;

/// Background task that forwards stream units into [`PathManager::record_media_part`].
pub fn spawn_record_tap(
    path: String,
    stream: Arc<Stream>,
    paths: Arc<PathManager>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut sub = stream.subscribe();
        loop {
            match sub.recv().await {
                Ok(unit) => paths.record_media_part(&path, unit.payload.as_ref()),
                Err(_) => break,
            }
        }
    })
}
