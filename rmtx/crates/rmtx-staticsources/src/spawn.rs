//! Background tasks that start pull sessions for configured sources at startup / reload.

use std::sync::Arc;

use rmtx_conf::Conf;
use rmtx_ffmpeg::{spawn_async, spawn_async_stdout, FfmpegPull};
use rmtx_path::{PathManager, PathSource};
use rmtx_servers_rtsp::RtspPullSession;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::ffmpeg_pipe::{ffmpeg_ingest_stream, pump_mpegts_stdout};
use crate::side_table::StaticSourceSideTable;
use crate::StaticSource;

/// Spawns a background pull task for each configured RTSP/RTSPS source.
///
/// Publisher and other non-pull sources are no-ops (paths stay not-ready until
/// a publisher connects).
pub fn spawn_configured_sources(
    conf: &Conf,
    paths: Arc<PathManager>,
    side: Arc<StaticSourceSideTable>,
) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::new();

    let rtsp_listen = conf.rtsp_address.clone();

    for (name, path_conf) in &conf.paths {
        match StaticSource::from_source_field(&path_conf.source) {
            StaticSource::Rtsp(url) | StaticSource::Rtsps(url) => {
                let name = name.clone();
                let paths = Arc::clone(&paths);
                let side = Arc::clone(&side);
                let rtsp_listen = rtsp_listen.clone();
                handles.push(tokio::spawn(async move {
                    let publish_url = rtsp_publish_url(&rtsp_listen, &name);
                    start_rtsp_pull(&name, &url, paths, side, publish_url).await;
                }));
            }
            StaticSource::Other(_)
            | StaticSource::Rtmp(_)
            | StaticSource::RtmpS(_)
            | StaticSource::Hls(_)
            | StaticSource::HttpsHls(_) => {}
        }
    }

    handles
}

fn rtsp_publish_url(listen: &str, path: &str) -> String {
    let host = if let Some(port) = listen.strip_prefix(':') {
        format!("127.0.0.1:{port}")
    } else {
        listen.to_owned()
    };
    format!("rtsp://{host}/{path}")
}

async fn start_rtsp_pull(
    name: &str,
    url: &str,
    paths: Arc<PathManager>,
    side: Arc<StaticSourceSideTable>,
    local_publish_url: String,
) {
    let _ = paths.set_ready(name, false, None);

    let path_name = name.to_owned();
    let paths_for_bytes = Arc::clone(&paths);
    let on_bytes = Arc::new(move |delta: u64| {
        let _ = paths_for_bytes.add_bytes_received(&path_name, delta);
    });

    match RtspPullSession::start(url, on_bytes).await {
        Ok(session) => {
            info!(path = %name, %url, "RTSP pull session started");
            let stream = side.attach(name, session);
            if let Err(err) = paths.set_ready(
                name,
                true,
                Some(PathSource {
                    source_type: "rtspSource".into(),
                    id: String::new(),
                }),
            ) {
                warn!(path = %name, error = %err, "failed to mark path ready");
            }
            side.maybe_start_record_tap(name, stream, Arc::clone(&paths));
        }
        Err(err) => {
            warn!(path = %name, %url, error = %err, "RTSP pull session failed to start");
            try_ffmpeg_fallback(name, url, paths, side, local_publish_url).await;
        }
    }
}

async fn try_ffmpeg_fallback(
    name: &str,
    url: &str,
    paths: Arc<PathManager>,
    side: Arc<StaticSourceSideTable>,
    local_publish_url: String,
) {
    let pull = FfmpegPull::new(url).with_output_rtsp(local_publish_url);
    let pipe_argv = pull.argv_mpegts_stdout();

    match spawn_async_stdout(&pipe_argv).await {
        Ok(mut child) => {
            let stdout = match child.stdout.take() {
                Some(s) => s,
                None => {
                    warn!(path = %name, "ffmpeg pipe: no stdout");
                    return try_ffmpeg_rtsp_republish(name, url, paths, side, pull).await;
                }
            };
            let stream = ffmpeg_ingest_stream();
            let arc = side.attach_stream(name, stream);
            let path_name = name.to_owned();
            let stream_for_pump = Arc::clone(&arc);
            tokio::spawn(async move {
                pump_mpegts_stdout(&path_name, stdout, stream_for_pump).await;
                let _ = child.wait().await;
            });
            info!(
                path = %name,
                %url,
                ingest = "ffmpeg-fallback-pipe",
                "ffmpeg MPEG-TS pipe ingest started"
            );
            if let Err(err) = paths.set_ready(
                name,
                true,
                Some(PathSource {
                    source_type: "rtspSource".into(),
                    id: "ffmpeg-fallback".into(),
                }),
            ) {
                warn!(path = %name, error = %err, "failed to mark path ready (ffmpeg pipe)");
            }
            side.maybe_start_record_tap(name, arc, paths);
        }
        Err(rmtx_ffmpeg::FfmpegError::Spawn(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            warn!(
                path = %name,
                %url,
                ingest = "ffmpeg-fallback",
                error = %e,
                "ffmpeg not found; path stays not-ready"
            );
        }
        Err(err) => {
            warn!(
                path = %name,
                %url,
                error = %err,
                "ffmpeg pipe spawn failed; trying RTSP republish argv"
            );
            try_ffmpeg_rtsp_republish(name, url, paths, side, pull).await;
        }
    }
}

async fn try_ffmpeg_rtsp_republish(
    name: &str,
    url: &str,
    paths: Arc<PathManager>,
    side: Arc<StaticSourceSideTable>,
    pull: FfmpegPull,
) {
    let argv = pull.argv();

    match spawn_async(&argv).await {
        Ok(mut child) => {
            info!(
                path = %name,
                %url,
                ingest = "ffmpeg-fallback-rtsp",
                ?argv,
                "ffmpeg RTSP republish started (ingest when publish path exists)"
            );
            let path_name = name.to_owned();
            let publish_url = pull.output_rtsp.clone();
            tokio::spawn(async move {
                if let Ok(status) = child.wait().await {
                    warn!(path = %path_name, ?status, "ffmpeg fallback process exited");
                }
            });
            info!(
                path = %name,
                %publish_url,
                "ffmpeg RTSP republish started; path becomes ready on RTSP publish ingest"
            );
            let _ = (paths, side);
        }
        Err(rmtx_ffmpeg::FfmpegError::Spawn(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            warn!(
                path = %name,
                %url,
                ingest = "ffmpeg-fallback",
                error = %e,
                "ffmpeg not found; path stays not-ready"
            );
        }
        Err(err) => {
            warn!(
                path = %name,
                %url,
                ingest = "ffmpeg-fallback",
                error = %err,
                "ffmpeg fallback spawn failed; path stays not-ready"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmtx_conf::Path;

    #[tokio::test]
    async fn spawn_without_rtsp_urls_is_empty() {
        let paths = Arc::new(PathManager::new());
        let side = Arc::new(StaticSourceSideTable::new());
        let mut conf = rmtx_conf::Conf::default();
        conf.paths.insert("live".into(), Path::default());

        let handles = spawn_configured_sources(&conf, paths, side);
        assert!(handles.is_empty());
    }
}
