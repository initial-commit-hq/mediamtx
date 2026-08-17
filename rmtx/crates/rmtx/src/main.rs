//! rmtx binary — MediaMTX-compatible media server.
//!
//! Phase 1: config load, path manager, Control API skeleton, config hot reload,
//! graceful shutdown.

mod reload;

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use rmtx_api::AppState;
use rmtx_conf::{Conf, ConfError, ConfWatcher};
use rmtx_hooks::{rtsp_port_from_address, HookRunner};
use rmtx_metrics::MetricsExporter;
use rmtx_path::PathManager;
use rmtx_playback::PlaybackServer;
use rmtx_servers_hls::HlsServer;
#[cfg(feature = "xiu-rtmp")]
use rmtx_servers_rtmp::RtmpServer;
use rmtx_servers_rtsp::RtspServer;
use rmtx_servers_webrtc::WebRtcServer;
use rmtx_staticsources::{
    ensure_always_available_shared, spawn_configured_sources, StaticSourceSideTable,
};
use tokio::signal;
use tracing::{error, info, warn};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
struct Cli {
    config_path: PathBuf,
    show_version: bool,
}

fn parse_cli() -> Cli {
    let mut args = env::args().skip(1);
    let mut config_path = PathBuf::from("mediamtx.yml");
    let mut show_version = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => show_version = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other if other.starts_with('-') => {
                eprintln!("rmtx: unknown flag {other}");
                print_help();
                std::process::exit(2);
            }
            path => config_path = PathBuf::from(path),
        }
    }

    Cli {
        config_path,
        show_version,
    }
}

fn print_help() {
    eprintln!(
        "Usage: rmtx [OPTIONS] [CONFIG]\n\
         \n\
         MediaMTX-compatible media server (Rust port).\n\
         \n\
         Arguments:\n\
           [CONFIG]  Config file path [default: mediamtx.yml]\n\
         \n\
         Options:\n\
           -V, --version  Print version and exit\n\
           -h, --help     Print help and exit"
    );
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}

fn load_or_exit(path: &Path) -> Conf {
    match Conf::load_from_file(path) {
        Ok(cfg) => cfg,
        Err(ConfError::NotFound(p)) => {
            error!(path = %p, "config file not found");
            std::process::exit(2);
        }
        Err(ConfError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            error!(path = %path.display(), "config file not found");
            std::process::exit(2);
        }
        Err(e @ ConfError::Io(_) | e @ ConfError::Parse(_)) => {
            error!(error = %e, "invalid config");
            std::process::exit(2);
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    let cli = parse_cli();
    if cli.show_version {
        println!("rmtx {VERSION}");
        return ExitCode::SUCCESS;
    }

    info!(version = VERSION, path = %cli.config_path.display(), "starting rmtx");
    let conf = load_or_exit(&cli.config_path);

    let paths = Arc::new(PathManager::new());
    let hook_runner = Arc::new(HookRunner::new(rtsp_port_from_address(&conf.rtsp_address)));
    paths.set_hook_runner(Arc::clone(&hook_runner));
    paths.load_from_conf(&conf);
    let source_side = Arc::new(StaticSourceSideTable::new());
    ensure_always_available_shared(&conf, Arc::clone(&paths), Arc::clone(&source_side));
    let _source_handles =
        spawn_configured_sources(&conf, Arc::clone(&paths), Arc::clone(&source_side));

    let api_state = if conf.api {
        Some(AppState::new(VERSION, conf.clone(), Arc::clone(&paths)))
    } else {
        warn!("control API disabled (api: false). Phase 1 integration tests should set api: yes");
        None
    };

    if let Some(state) = api_state.clone() {
        let addr = conf.api_address.clone();
        tokio::spawn(async move {
            if let Err(e) = rmtx_api::serve_listen(&addr, state).await {
                error!(error = %e, "control API exited with error");
            }
        });
        info!(address = %conf.api_address, "control API enabled");
    }

    if conf.metrics {
        let metrics_exporter = Arc::new(MetricsExporter::new().with_paths(Arc::clone(&paths)));
        let addr = conf.metrics_address.clone();
        tokio::spawn(async move {
            if let Err(e) = rmtx_metrics::serve_listen(&addr, metrics_exporter).await {
                error!(error = %e, "metrics listener exited with error");
            }
        });
        info!(address = %conf.metrics_address, "metrics enabled");
    }

    if conf.rtsp {
        let addr = conf.rtsp_address.clone();
        let rtsp_paths = Arc::clone(&paths);
        let rtsp_streams = Arc::clone(&source_side);
        tokio::spawn(async move {
            match RtspServer::bind(&addr).await {
                Ok(server) => {
                    info!(address = %addr, "RTSP server enabled");
                    if let Err(e) = server.run(rtsp_paths, rtsp_streams).await {
                        error!(error = %e, "RTSP server exited with error");
                    }
                }
                Err(e) => error!(error = %e, address = %addr, "failed to bind RTSP server"),
            }
        });
    } else {
        info!("RTSP server disabled (rtsp: false)");
    }

    if conf.hls {
        let addr = conf.hls_address.clone();
        let hls_paths = Arc::clone(&paths);
        let hls_streams = Arc::clone(&source_side);
        tokio::spawn(async move {
            match HlsServer::bind(&addr).await {
                Ok(server) => {
                    info!(address = %addr, "HLS server enabled");
                    if let Err(e) = server.run(hls_paths, hls_streams).await {
                        error!(error = %e, "HLS server exited with error");
                    }
                }
                Err(e) => error!(error = %e, address = %addr, "failed to bind HLS server"),
            }
        });
    } else {
        info!("HLS server disabled (hls: false)");
    }

    if conf.webrtc {
        let addr = conf.webrtc_address.clone();
        let webrtc_paths = Arc::clone(&paths);
        let webrtc_streams = Arc::clone(&source_side);
        tokio::spawn(async move {
            match WebRtcServer::bind(&addr).await {
                Ok(server) => {
                    info!(address = %addr, "WebRTC/WHEP server enabled");
                    if let Err(e) = server.run(webrtc_paths, webrtc_streams).await {
                        error!(error = %e, "WebRTC server exited with error");
                    }
                }
                Err(e) => error!(error = %e, address = %addr, "failed to bind WebRTC server"),
            }
        });
    } else {
        info!("WebRTC server disabled (webrtc: false)");
    }

    if conf.playback {
        let addr = conf.playback_address.clone();
        let playback_paths = Arc::clone(&paths);
        tokio::spawn(async move {
            match PlaybackServer::bind(&addr).await {
                Ok(server) => {
                    info!(address = %addr, "playback server enabled");
                    if let Err(e) = server.run(playback_paths).await {
                        error!(error = %e, "playback server exited with error");
                    }
                }
                Err(e) => error!(error = %e, address = %addr, "failed to bind playback server"),
            }
        });
    } else {
        info!("playback server disabled (playback: false)");
    }

    if conf.rtmp {
        #[cfg(feature = "xiu-rtmp")]
        {
            let addr = conf.rtmp_address.clone();
            let rtmp_paths = Arc::clone(&paths);
            let rtmp_side = Arc::clone(&source_side);
            tokio::spawn(async move {
                match RtmpServer::bind(&addr).await {
                    Ok(server) => {
                        info!(address = %addr, "RTMP server enabled");
                        if let Err(e) = server.run(rtmp_paths, rtmp_side).await {
                            error!(error = %e, "RTMP server exited with error");
                        }
                    }
                    Err(e) => error!(error = %e, address = %addr, "failed to bind RTMP server"),
                }
            });
        }
        #[cfg(not(feature = "xiu-rtmp"))]
        warn!("RTMP enabled in config but rmtx built without xiu-rtmp feature");
    } else {
        info!("RTMP server disabled (rtmp: false)");
    }

    let (reload_shutdown_tx, reload_shutdown_rx) = tokio::sync::oneshot::channel();
    let conf_watcher = ConfWatcher::new(&cli.config_path);
    tokio::spawn(reload::run_conf_reload_loop(
        conf_watcher,
        Arc::clone(&paths),
        Some(Arc::clone(&hook_runner)),
        api_state,
        Arc::clone(&source_side),
        reload_shutdown_rx,
    ));
    info!(path = %cli.config_path.display(), "config hot reload enabled");

    info!("rmtx running; press Ctrl+C to stop");
    match signal::ctrl_c().await {
        Ok(()) => info!("shutdown signal received"),
        Err(e) => error!(error = %e, "failed to listen for shutdown signal"),
    }

    let _ = reload_shutdown_tx.send(());

    ExitCode::SUCCESS
}
