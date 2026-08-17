//! Minimal RTSP server listener (Phase 2).
//!
//! Handles OPTIONS, DESCRIBE, SETUP (session creation), PLAY (media pump), and TEARDOWN.
//!
//! On PLAY with TCP interleaved transport, subscribes to [`StreamLookup`] / `StreamBus` and
//! writes RTP as RTSP interleaved `$` frames. UDP transport and RTCP are not implemented yet.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rmtx_path::PathManager;
use rmtx_stream::{Stream, StreamLookup};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// RTSP listener bound via [`Self::bind`].
pub struct RtspServer {
    listener: TcpListener,
}

impl RtspServer {
    /// Binds a TCP listener on `addr` (e.g. `:8554` or `127.0.0.1:0`).
    ///
    /// Bare `:port` forms are normalized to `0.0.0.0:port` (MediaMTX-compatible).
    pub async fn bind(addr: impl AsRef<str>) -> Result<Self, RtspServerError> {
        let addr = addr.as_ref();
        let normalized = if let Some(port) = addr.strip_prefix(':') {
            format!("0.0.0.0:{port}")
        } else {
            addr.to_owned()
        };
        let listener = TcpListener::bind(&normalized)
            .await
            .map_err(RtspServerError::Io)?;
        Ok(Self { listener })
    }

    /// Returns the local socket address (useful when binding `:0`).
    pub fn local_addr(&self) -> Result<SocketAddr, RtspServerError> {
        self.listener.local_addr().map_err(RtspServerError::Io)
    }

    /// Accepts RTSP connections until the listener is closed.
    ///
    /// **Implemented:** OPTIONS, DESCRIBE, SETUP (session + Transport echo),
    /// PLAY (TCP interleaved media pump), TEARDOWN (session removal + pump abort).
    /// **501 Not Implemented:** any other method.
    pub async fn run(
        self,
        paths: Arc<PathManager>,
        streams: Arc<dyn StreamLookup>,
    ) -> Result<(), RtspServerError> {
        info!(addr = %self.local_addr()?.to_string(), "RTSP server listening");
        loop {
            let (stream, peer) = self.listener.accept().await.map_err(RtspServerError::Io)?;
            debug!(%peer, "RTSP client connected");
            let paths = Arc::clone(&paths);
            let streams = Arc::clone(&streams);
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, paths, streams).await {
                    debug!(%peer, error = %e, "RTSP connection ended");
                }
            });
        }
    }
}

#[derive(Debug, Error)]
pub enum RtspServerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid RTSP request: {0}")]
    InvalidRequest(String),
}

async fn handle_connection(
    stream: TcpStream,
    paths: Arc<PathManager>,
    streams: Arc<dyn StreamLookup>,
) -> Result<(), RtspServerError> {
    let (mut reader, writer) = stream.into_split();
    let writer = Arc::new(Mutex::new(writer));
    let mut state = ConnectionState::default();

    loop {
        let request = match read_rtsp_request(&mut reader).await {
            Ok(Some(req)) => req,
            Ok(None) => break,
            Err(e) => {
                let _ = writer.lock().await.shutdown().await;
                state.abort_all_pumps();
                return Err(e);
            }
        };

        match request.method.as_str() {
            "PLAY" => {
                let response = handle_play(&request, &state);
                if let Err(e) = writer.lock().await.write_all(response.as_bytes()).await {
                    state.abort_all_pumps();
                    return Err(RtspServerError::Io(e));
                }
                let pump_config = play_session(&request, &state).and_then(|(session_id, session)| {
                    match session.transport {
                        SessionTransport::TcpInterleaved { rtp_channel } => {
                            Some((session_id, session.path.clone(), rtp_channel))
                        }
                        SessionTransport::Udp => {
                            warn!(path = %session.path, "PLAY with UDP transport: media pump not implemented");
                            None
                        }
                    }
                });
                if let Some((session_id, path, rtp_channel)) = pump_config {
                    state.abort_pump(&session_id);
                    let handle = spawn_media_pump(
                        path,
                        rtp_channel,
                        Arc::clone(&paths),
                        Arc::clone(&streams),
                        Arc::clone(&writer),
                    );
                    state.pumps.insert(session_id, handle);
                }
            }
            "RECORD" => {
                let response = handle_record(&request, &state);
                if let Err(e) = writer.lock().await.write_all(response.as_bytes()).await {
                    state.abort_all_pumps();
                    return Err(RtspServerError::Io(e));
                }
                if let Some((_, session)) = play_session(&request, &state) {
                    let path = session.path.clone();
                    if let Err(e) = crate::publish::ingest_publish_interleaved(
                        &path,
                        Arc::clone(&paths),
                        Arc::clone(&streams),
                        &mut reader,
                    )
                    .await
                    {
                        return Err(e);
                    }
                }
            }
            "TEARDOWN" => {
                if let Some(session_id) = request.session.as_deref() {
                    state.abort_pump(session_id);
                }
                let response = handle_teardown(&request, &mut state);
                if let Err(e) = writer.lock().await.write_all(response.as_bytes()).await {
                    state.abort_all_pumps();
                    return Err(RtspServerError::Io(e));
                }
            }
            _ => {
                let response = dispatch_request(&request, &paths, streams.as_ref(), &mut state);
                if let Err(e) = writer.lock().await.write_all(response.as_bytes()).await {
                    state.abort_all_pumps();
                    return Err(RtspServerError::Io(e));
                }
            }
        }
    }

    state.abort_all_pumps();
    Ok(())
}

/// Per-connection RTSP session state.
#[derive(Debug, Default)]
struct ConnectionState {
    sessions: HashMap<String, SessionState>,
    pumps: HashMap<String, JoinHandle<()>>,
}

#[derive(Debug, Clone)]
struct SessionState {
    path: String,
    transport: SessionTransport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionTransport {
    TcpInterleaved {
        rtp_channel: u8,
    },
    /// UDP transport accepted at SETUP; media pump not implemented yet.
    Udp,
}

impl ConnectionState {
    fn abort_pump(&mut self, session_id: &str) {
        if let Some(handle) = self.pumps.remove(session_id) {
            handle.abort();
        }
    }

    fn abort_all_pumps(&mut self) {
        for (_, handle) in self.pumps.drain() {
            handle.abort();
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RtspRequest {
    method: String,
    uri: String,
    cseq: Option<u32>,
    transport: Option<String>,
    session: Option<String>,
}

fn dispatch_request(
    req: &RtspRequest,
    paths: &Arc<PathManager>,
    streams: &dyn StreamLookup,
    state: &mut ConnectionState,
) -> String {
    match req.method.as_str() {
        "OPTIONS" => rtsp_response(
            200,
            "OK",
            req.cseq,
            "Public: OPTIONS, DESCRIBE, SETUP, PLAY, RECORD, ANNOUNCE, TEARDOWN\r\n",
        ),
        "DESCRIBE" => handle_describe(req, paths.as_ref(), streams),
        "ANNOUNCE" => handle_announce_rtsp(req, paths, streams),
        "SETUP" => handle_setup(req, paths.as_ref(), state),
        "PLAY" => unreachable!("PLAY handled in handle_connection"),
        "TEARDOWN" => unreachable!("TEARDOWN handled in handle_connection"),
        _ => rtsp_response(501, "Not Implemented", req.cseq, ""),
    }
}

fn handle_record(req: &RtspRequest, state: &ConnectionState) -> String {
    let Some(session_id) = req.session.as_deref() else {
        return rtsp_response(454, "Session Not Found", req.cseq, "");
    };
    if state.sessions.get(session_id).is_none() {
        return rtsp_response(454, "Session Not Found", req.cseq, "");
    }
    let headers = format!("Session: {session_id}\r\n");
    rtsp_response(200, "OK", req.cseq, &headers)
}

fn handle_announce_rtsp(
    req: &RtspRequest,
    paths: &Arc<PathManager>,
    streams: &dyn StreamLookup,
) -> String {
    let Some(path_name) = path_from_rtsp_uri(&req.uri) else {
        return rtsp_response(400, "Bad Request", req.cseq, "");
    };
    match crate::publish::handle_announce(&path_name, paths, streams) {
        Ok(_) => rtsp_response(200, "OK", req.cseq, ""),
        Err(_) => rtsp_response(404, "Not Found", req.cseq, ""),
    }
}

fn handle_setup(req: &RtspRequest, paths: &PathManager, state: &mut ConnectionState) -> String {
    let Some(path_name) = path_from_rtsp_uri(&req.uri) else {
        return rtsp_response(400, "Bad Request", req.cseq, "");
    };

    if paths.get(&path_name).is_err() {
        return rtsp_response(404, "Not Found", req.cseq, "");
    }

    let session_id = new_session_id();
    let transport = session_transport_from_setup(req.transport.as_deref());
    state.sessions.insert(
        session_id.clone(),
        SessionState {
            path: path_name.clone(),
            transport,
        },
    );

    let transport = build_transport_response(req.transport.as_deref());
    let headers = format!("Session: {session_id}\r\n{transport}");
    rtsp_response(200, "OK", req.cseq, &headers)
}

fn session_transport_from_setup(client_transport: Option<&str>) -> SessionTransport {
    let transport = client_transport.unwrap_or("");
    let upper = transport.to_ascii_uppercase();

    if upper.contains("UDP") {
        return SessionTransport::Udp;
    }

    let rtp_channel = parse_rtp_interleaved_channel(transport);
    SessionTransport::TcpInterleaved { rtp_channel }
}

fn parse_rtp_interleaved_channel(transport: &str) -> u8 {
    parse_interleaved_channels(transport)
        .and_then(|channels| channels.split('-').next())
        .and_then(|ch| ch.trim().parse().ok())
        .unwrap_or(0)
}

fn play_session<'a>(
    req: &RtspRequest,
    state: &'a ConnectionState,
) -> Option<(String, &'a SessionState)> {
    let session_id = req.session.as_deref()?;
    let session = state.sessions.get(session_id)?;
    Some((session_id.to_owned(), session))
}

fn handle_play(req: &RtspRequest, state: &ConnectionState) -> String {
    let Some(session_id) = req.session.as_deref() else {
        return rtsp_response(454, "Session Not Found", req.cseq, "");
    };

    let Some(path_name) = state.sessions.get(session_id).map(|s| s.path.as_str()) else {
        return rtsp_response(454, "Session Not Found", req.cseq, "");
    };

    let rtp_info = format!(
        "Session: {session_id}\r\n\
         RTP-Info: url=rtsp://127.0.0.1/{path_name}/trackID=0;seq=0;rtptime=0\r\n"
    );
    rtsp_response(200, "OK", req.cseq, &rtp_info)
}

fn handle_teardown(req: &RtspRequest, state: &mut ConnectionState) -> String {
    let Some(session_id) = req.session.as_deref() else {
        return rtsp_response(454, "Session Not Found", req.cseq, "");
    };

    if state.sessions.remove(session_id).is_none() {
        return rtsp_response(454, "Session Not Found", req.cseq, "");
    }

    let headers = format!("Session: {session_id}\r\n");
    rtsp_response(200, "OK", req.cseq, &headers)
}

fn spawn_media_pump(
    path: String,
    rtp_channel: u8,
    paths: Arc<PathManager>,
    streams: Arc<dyn StreamLookup>,
    writer: Arc<Mutex<OwnedWriteHalf>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let stream = match wait_for_stream(&streams, &path).await {
            Some(stream) => stream,
            None => return,
        };

        let mut rx = stream.subscribe();
        let mut seq: u16 = 0;
        let ssrc: u32 = 0x5254_5850; // "RTPX"

        debug!(path = %path, rtp_channel, "RTSP media pump started");

        loop {
            match rx.recv().await {
                Ok(unit) => {
                    let pt = payload_type_for_track(unit.track_id);
                    let clock_rate = clock_rate_for_track(&stream, unit.track_id);
                    let timestamp = pts_to_rtp_timestamp(unit.pts_us, clock_rate);
                    let rtp = build_rtp_packet(pt, seq, timestamp, ssrc, &unit.payload);
                    seq = seq.wrapping_add(1);
                    let frame = build_interleaved_frame(rtp_channel, &rtp);
                    let frame_len = frame.len() as u64;
                    if writer.lock().await.write_all(&frame).await.is_err() {
                        debug!(path = %path, "RTSP media pump write failed");
                        break;
                    }
                    let _ = paths.add_bytes_sent(&path, frame_len);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    debug!(path = %path, "RTSP media pump: stream bus closed");
                    break;
                }
            }
        }
    })
}

async fn wait_for_stream(streams: &Arc<dyn StreamLookup>, path: &str) -> Option<Arc<Stream>> {
    let mut wait_logged = false;
    loop {
        if let Some(stream) = streams.stream(path) {
            return Some(stream);
        }
        if !wait_logged {
            info!(path, "PLAY: waiting for stream to attach");
            wait_logged = true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn payload_type_for_track(track_id: u32) -> u8 {
    96u8.saturating_add(track_id as u8)
}

fn clock_rate_for_track(stream: &Stream, track_id: u32) -> u32 {
    stream
        .tracks()
        .iter()
        .find(|track| track.id == track_id)
        .map(|track| track.clock_rate)
        .unwrap_or(90_000)
}

fn pts_to_rtp_timestamp(pts_us: u64, clock_rate: u32) -> u32 {
    ((pts_us.saturating_mul(clock_rate as u64)) / 1_000_000) as u32
}

fn build_rtp_packet(pt: u8, seq: u16, timestamp: u32, ssrc: u32, payload: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(12 + payload.len());
    packet.push(0x80);
    packet.push(pt & 0x7F);
    packet.extend_from_slice(&seq.to_be_bytes());
    packet.extend_from_slice(&timestamp.to_be_bytes());
    packet.extend_from_slice(&ssrc.to_be_bytes());
    packet.extend_from_slice(payload);
    packet
}

fn build_interleaved_frame(channel: u8, rtp_packet: &[u8]) -> Vec<u8> {
    let len = rtp_packet.len();
    debug_assert!(len <= u16::MAX as usize);
    let mut frame = Vec::with_capacity(4 + len);
    frame.push(b'$');
    frame.push(channel);
    frame.extend_from_slice(&(len as u16).to_be_bytes());
    frame.extend_from_slice(rtp_packet);
    frame
}

fn new_session_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("{:016x}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// Builds a Transport response echoing TCP interleaved or UDP (loosely parsed).
fn build_transport_response(client_transport: Option<&str>) -> String {
    let transport = client_transport.unwrap_or("");
    let upper = transport.to_ascii_uppercase();

    if upper.contains("UDP") {
        return "Transport: RTP/AVP/UDP;unicast;client_port=8000-8001;server_port=8002-8003\r\n"
            .to_owned();
    }

    if upper.contains("TCP") || transport.contains("interleaved") {
        let interleaved = parse_interleaved_channels(transport).unwrap_or("0-1");
        return format!("Transport: RTP/AVP/TCP;unicast;interleaved={interleaved}\r\n");
    }

    "Transport: RTP/AVP/TCP;unicast;interleaved=0-1\r\n".to_owned()
}

fn parse_interleaved_channels(transport: &str) -> Option<&str> {
    transport
        .split(';')
        .find_map(|part| part.trim().strip_prefix("interleaved="))
}

fn handle_describe(req: &RtspRequest, paths: &PathManager, streams: &dyn StreamLookup) -> String {
    let Some(path_name) = path_from_rtsp_uri(&req.uri) else {
        return rtsp_response(400, "Bad Request", req.cseq, "");
    };

    if paths.get(&path_name).is_err() {
        return rtsp_response(404, "Not Found", req.cseq, "");
    }

    let sdp = streams
        .stream(&path_name)
        .map(|s| sdp_from_stream(&path_name, &s))
        .unwrap_or_else(|| stub_h264_sdp(&path_name));

    let headers = format!(
        "Content-Type: application/sdp\r\nContent-Length: {}\r\n",
        sdp.len()
    );
    let mut body = rtsp_response(200, "OK", req.cseq, &headers);
    body.push_str(&sdp);
    body
}

fn rtsp_response(status: u16, reason: &str, cseq: Option<u32>, extra_headers: &str) -> String {
    let mut out = format!("RTSP/1.0 {status} {reason}\r\n");
    if let Some(seq) = cseq {
        out.push_str(&format!("CSeq: {seq}\r\n"));
    }
    out.push_str(extra_headers);
    out.push_str("\r\n");
    out
}

fn path_from_rtsp_uri(uri: &str) -> Option<String> {
    if uri == "*" {
        return None;
    }

    let path_part = if let Some(rest) = uri
        .strip_prefix("rtsp://")
        .or_else(|| uri.strip_prefix("rtsps://"))
    {
        rest.find('/').map(|i| &rest[i + 1..])
    } else if let Some(rest) = uri.strip_prefix('/') {
        Some(rest)
    } else {
        None
    }?;

    let segment = path_part.split('/').next()?.split('?').next()?;
    if segment.is_empty() {
        None
    } else {
        Some(segment.to_owned())
    }
}

fn stub_h264_sdp(path: &str) -> String {
    format!(
        "v=0\r\n\
         o=- 0 0 IN IP4 127.0.0.1\r\n\
         s={path}\r\n\
         t=0 0\r\n\
         a=control:*\r\n\
         m=video 0 RTP/AVP 96\r\n\
         a=rtpmap:96 H264/90000\r\n\
         a=fmtp:96 packetization-mode=1\r\n\
         a=control:trackID=0\r\n"
    )
}

fn sdp_from_stream(path: &str, stream: &Stream) -> String {
    let tracks = stream.tracks();
    if tracks.is_empty() {
        return stub_h264_sdp(path);
    }

    let mut sdp = format!(
        "v=0\r\n\
         o=- 0 0 IN IP4 127.0.0.1\r\n\
         s={path}\r\n\
         t=0 0\r\n\
         a=control:*\r\n"
    );

    for (i, track) in tracks.iter().enumerate() {
        let pt = 96 + i as u32;
        sdp.push_str(&format!(
            "m=video 0 RTP/AVP {pt}\r\n\
             a=rtpmap:{pt} {}/ {}\r\n\
             a=control:trackID={}\r\n",
            track.codec, track.clock_rate, track.id
        ));
    }

    sdp
}

async fn read_rtsp_request(
    stream: &mut OwnedReadHalf,
) -> Result<Option<RtspRequest>, RtspServerError> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 1024];

    loop {
        skip_interleaved_frames(&mut buf);

        if let Some(end) = find_header_end(&buf) {
            let header_bytes = &buf[..end];
            let header_text = std::str::from_utf8(header_bytes).map_err(|_| {
                RtspServerError::InvalidRequest("request headers are not valid UTF-8".into())
            })?;
            let req = parse_request_line_and_headers(header_text)?;
            buf.drain(0..end);
            return Ok(Some(req));
        }

        if !buf.is_empty() && buf[0] != b'$' {
            // Partial RTSP headers; keep reading.
        } else if !buf.is_empty() {
            skip_interleaved_frames(&mut buf);
            if buf.is_empty() {
                // Consumed interleaved only; read more.
            }
        }

        let n = stream.read(&mut chunk).await.map_err(RtspServerError::Io)?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);

        if buf.len() > 64 * 1024 {
            return Err(RtspServerError::InvalidRequest(
                "request headers too large".into(),
            ));
        }
    }
}

/// Removes complete RTSP interleaved frames (`$` + channel + length + payload) from the front of `buf`.
fn skip_interleaved_frames(buf: &mut Vec<u8>) {
    loop {
        if buf.first() != Some(&b'$') {
            return;
        }
        if buf.len() < 4 {
            return;
        }
        let payload_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        let frame_len = 4 + payload_len;
        if buf.len() < frame_len {
            return;
        }
        buf.drain(0..frame_len);
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn parse_request_line_and_headers(text: &str) -> Result<RtspRequest, RtspServerError> {
    let mut lines = text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| RtspServerError::InvalidRequest("empty request".into()))?;

    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| RtspServerError::InvalidRequest("missing method".into()))?
        .to_owned();
    let uri = parts
        .next()
        .ok_or_else(|| RtspServerError::InvalidRequest("missing URI".into()))?
        .to_owned();
    let version = parts
        .next()
        .ok_or_else(|| RtspServerError::InvalidRequest("missing RTSP version".into()))?;
    if version != "RTSP/1.0" {
        warn!(version, "non-RTSP/1.0 request");
    }

    let mut cseq = None;
    let mut transport = None;
    let mut session = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some(value) = line
            .strip_prefix("CSeq:")
            .or_else(|| line.strip_prefix("cseq:"))
        {
            cseq = value.trim().parse().ok();
        } else if let Some(value) = line
            .strip_prefix("Transport:")
            .or_else(|| line.strip_prefix("transport:"))
        {
            transport = Some(value.trim().to_owned());
        } else if let Some(value) = line
            .strip_prefix("Session:")
            .or_else(|| line.strip_prefix("session:"))
        {
            session = Some(parse_session_header(value.trim()));
        }
    }

    Ok(RtspRequest {
        method,
        uri,
        cseq,
        transport,
        session,
    })
}

fn parse_session_header(value: &str) -> String {
    value.split(';').next().unwrap_or(value).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use bytes::Bytes;
    use rmtx_path::{PathConf, PathManager};
    use rmtx_stream::{MediaUnit, NoStreamLookup, Stream, Track};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;

    struct TestStreamLookup {
        streams: Mutex<HashMap<String, Arc<Stream>>>,
    }

    impl TestStreamLookup {
        fn with_stream(path: &str, stream: Arc<Stream>) -> Self {
            let mut map = HashMap::new();
            map.insert(path.to_owned(), stream);
            Self {
                streams: Mutex::new(map),
            }
        }
    }

    impl StreamLookup for TestStreamLookup {
        fn stream(&self, path: &str) -> Option<Arc<Stream>> {
            self.streams.lock().unwrap().get(path).cloned()
        }
    }

    #[tokio::test]
    async fn options_returns_200() {
        let server = RtspServer::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let paths = Arc::new(PathManager::new());
        let streams = Arc::new(NoStreamLookup);

        let handle = tokio::spawn(async move { server.run(paths, streams).await });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("RTSP/1.0 200 OK"));
        assert!(response.contains("CSeq: 1"));
        assert!(response.contains("ANNOUNCE"));
        assert!(response.contains("RECORD"));

        drop(stream);
        handle.abort();
    }

    #[tokio::test]
    async fn describe_unknown_path_returns_404() {
        let server = RtspServer::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let paths = Arc::new(PathManager::new());
        let streams = Arc::new(NoStreamLookup);

        let handle = tokio::spawn(async move { server.run(paths, streams).await });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"DESCRIBE rtsp://127.0.0.1/missing RTSP/1.0\r\nCSeq: 2\r\n\r\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("RTSP/1.0 404 Not Found"));

        drop(stream);
        handle.abort();
    }

    #[tokio::test]
    async fn describe_known_path_returns_sdp() {
        let server = RtspServer::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("cam1"));
        let streams = Arc::new(NoStreamLookup);

        let handle = tokio::spawn(async move { server.run(paths, streams).await });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"DESCRIBE rtsp://127.0.0.1/cam1 RTSP/1.0\r\nCSeq: 3\r\n\r\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("RTSP/1.0 200 OK"));
        assert!(response.contains("Content-Type: application/sdp"));
        assert!(response.contains("H264/90000"));

        drop(stream);
        handle.abort();
    }

    #[test]
    fn path_from_uri_extracts_first_segment() {
        assert_eq!(
            path_from_rtsp_uri("rtsp://host:8554/live/extra"),
            Some("live".into())
        );
        assert_eq!(path_from_rtsp_uri("rtsp://host/cam1"), Some("cam1".into()));
        assert_eq!(path_from_rtsp_uri("/cam1"), Some("cam1".into()));
        assert_eq!(path_from_rtsp_uri("*"), None);
    }

    #[test]
    fn build_transport_response_tcp_and_udp() {
        assert!(
            build_transport_response(Some("RTP/AVP/TCP;unicast;interleaved=2-3"))
                .contains("interleaved=2-3")
        );
        assert!(
            build_transport_response(Some("RTP/AVP/UDP;unicast;client_port=5000-5001"))
                .contains("server_port=8002-8003")
        );
        assert!(build_transport_response(None).contains("interleaved=0-1"));
    }

    #[test]
    fn parse_rtp_interleaved_channel_extracts_first() {
        assert_eq!(
            parse_rtp_interleaved_channel("RTP/AVP/TCP;unicast;interleaved=2-3"),
            2
        );
        assert_eq!(parse_rtp_interleaved_channel(""), 0);
    }

    #[test]
    fn rtp_and_interleaved_framing() {
        let rtp = build_rtp_packet(96, 1, 90000, 0x11223344, b"payload");
        assert_eq!(rtp.len(), 12 + 7);
        assert_eq!(rtp[0], 0x80);
        assert_eq!(rtp[1], 96);
        assert_eq!(&rtp[2..4], &[0, 1]);
        assert_eq!(&rtp[4..8], &90000u32.to_be_bytes());
        assert_eq!(&rtp[8..12], &0x11223344u32.to_be_bytes());
        assert_eq!(&rtp[12..], b"payload");

        let frame = build_interleaved_frame(0, &rtp);
        assert_eq!(frame[0], b'$');
        assert_eq!(frame[1], 0);
        assert_eq!(u16::from_be_bytes([frame[2], frame[3]]) as usize, rtp.len());
        assert_eq!(&frame[4..], rtp);
    }

    #[test]
    fn skip_interleaved_frames_removes_one_or_more() {
        let rtp = build_rtp_packet(96, 0, 0, 0, b"x");
        let frame = build_interleaved_frame(1, &rtp);
        let mut buf = frame.clone();
        buf.extend_from_slice(b"OPTIONS * RTSP/1.0\r\n\r\n");
        skip_interleaved_frames(&mut buf);
        assert_eq!(buf, b"OPTIONS * RTSP/1.0\r\n\r\n");
    }

    #[tokio::test]
    async fn setup_play_teardown_happy_path() {
        let server = RtspServer::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("cam1"));
        let streams = Arc::new(NoStreamLookup);

        let handle = tokio::spawn(async move { server.run(paths, streams).await });

        let mut stream = TcpStream::connect(addr).await.unwrap();

        // SETUP
        stream
            .write_all(
                b"SETUP rtsp://127.0.0.1/cam1/trackID=0 RTSP/1.0\r\n\
                  CSeq: 10\r\n\
                  Transport: RTP/AVP/TCP;unicast;interleaved=0-1\r\n\
                  \r\n",
            )
            .await
            .unwrap();

        let setup_resp = read_rtsp_response(&mut stream).await;
        assert!(setup_resp.contains("RTSP/1.0 200 OK"));
        assert!(setup_resp.contains("CSeq: 10"));
        assert!(setup_resp.contains("Session: "));
        assert!(setup_resp.contains("Transport: RTP/AVP/TCP;unicast;interleaved=0-1"));

        let session_id = extract_header_value(&setup_resp, "Session").unwrap();

        // PLAY
        let play_req = format!(
            "PLAY rtsp://127.0.0.1/cam1 RTSP/1.0\r\n\
             CSeq: 11\r\n\
             Session: {session_id}\r\n\
             \r\n"
        );
        stream.write_all(play_req.as_bytes()).await.unwrap();

        let play_resp = read_rtsp_response(&mut stream).await;
        assert!(play_resp.contains("RTSP/1.0 200 OK"));
        assert!(play_resp.contains("CSeq: 11"));
        assert!(play_resp.contains(&format!("Session: {session_id}")));
        assert!(play_resp.contains("RTP-Info:"));

        // TEARDOWN
        let teardown_req = format!(
            "TEARDOWN rtsp://127.0.0.1/cam1 RTSP/1.0\r\n\
             CSeq: 12\r\n\
             Session: {session_id}\r\n\
             \r\n"
        );
        stream.write_all(teardown_req.as_bytes()).await.unwrap();

        let teardown_resp = read_rtsp_response(&mut stream).await;
        assert!(teardown_resp.contains("RTSP/1.0 200 OK"));
        assert!(teardown_resp.contains("CSeq: 12"));
        assert!(teardown_resp.contains(&format!("Session: {session_id}")));

        drop(stream);
        handle.abort();
    }

    #[tokio::test]
    async fn play_pumps_interleaved_rtp_from_stream() {
        let server = RtspServer::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let paths = Arc::new(PathManager::new());
        paths.upsert_conf(PathConf::new("cam1"));

        let media_stream = Arc::new(Stream::new(vec![Track {
            id: 0,
            codec: "H264".to_owned(),
            clock_rate: 90_000,
        }]));
        let streams = Arc::new(TestStreamLookup::with_stream(
            "cam1",
            Arc::clone(&media_stream),
        ));

        let handle = tokio::spawn(async move { server.run(paths, streams).await });

        let mut stream = TcpStream::connect(addr).await.unwrap();

        stream
            .write_all(
                b"SETUP rtsp://127.0.0.1/cam1/trackID=0 RTSP/1.0\r\n\
                  CSeq: 20\r\n\
                  Transport: RTP/AVP/TCP;unicast;interleaved=0-1\r\n\
                  \r\n",
            )
            .await
            .unwrap();

        let setup_resp = read_rtsp_response(&mut stream).await;
        assert!(setup_resp.contains("RTSP/1.0 200 OK"));
        let session_id = extract_header_value(&setup_resp, "Session").unwrap();

        let play_req = format!(
            "PLAY rtsp://127.0.0.1/cam1 RTSP/1.0\r\n\
             CSeq: 21\r\n\
             Session: {session_id}\r\n\
             \r\n"
        );
        stream.write_all(play_req.as_bytes()).await.unwrap();
        let play_resp = read_rtsp_response(&mut stream).await;
        assert!(play_resp.contains("RTSP/1.0 200 OK"));

        tokio::time::sleep(Duration::from_millis(50)).await;

        media_stream
            .write(MediaUnit {
                track_id: 0,
                pts_us: 1_000_000,
                payload: Bytes::from_static(b"\x00\x00\x00\x01\x65"),
            })
            .unwrap();

        let mut buf = vec![0u8; 4096];
        let read_result = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf))
            .await
            .expect("timed out waiting for interleaved RTP")
            .expect("read failed");
        assert!(read_result >= 4, "expected interleaved frame");
        assert_eq!(buf[0], b'$');
        assert_eq!(buf[1], 0);
        let rtp_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        assert_eq!(read_result, 4 + rtp_len);
        assert!(rtp_len >= 12);
        assert_eq!(buf[4], 0x80);
        assert_eq!(buf[5], 96);
        let rtp = &buf[4..4 + rtp_len];
        assert_eq!(&rtp[12..], b"\x00\x00\x00\x01\x65");

        drop(stream);
        handle.abort();
    }

    async fn read_rtsp_response(stream: &mut TcpStream) -> String {
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }

    fn extract_header_value(response: &str, name: &str) -> Option<String> {
        let prefix = format!("{name}: ");
        response.lines().find_map(|line| {
            line.strip_prefix(&prefix)
                .map(|v| parse_session_header(v.trim()))
        })
    }
}
