#!/usr/bin/env bash
# Phase 1 live smoke test: build rmtx, exercise Control API, clean shutdown.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

API="http://127.0.0.1:19997"
METRICS="http://127.0.0.1:19998/metrics"
HLS="http://127.0.0.1:18889"
WHEP="http://127.0.0.1:18890"
PLAYBACK="http://127.0.0.1:19996"
CONFIG="$ROOT/testdata/phase1.yml"
BIN="$ROOT/target/debug/rmtx"
PID=""

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

cleanup() {
  if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

echo "==> building rmtx (with xiu-rtmp for RTMP listener)"
cargo build -p rmtx --features xiu-rtmp --quiet

echo "==> starting rmtx with $CONFIG"
"$BIN" "$CONFIG" &
PID=$!

echo "==> waiting for control API"
ready=0
for _ in $(seq 1 50); do
  if curl -sf "$API/v3/info" >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.1
done
[[ "$ready" -eq 1 ]] || fail "control API did not become ready on $API"

echo "==> GET /v3/info"
info="$(curl -sf "$API/v3/info")"
echo "$info" | grep -q '"version"' || fail "/v3/info missing version field"
echo "$info"

echo "==> GET /v3/paths/list"
paths="$(curl -sf "$API/v3/paths/list")"
echo "$paths" | grep -q '"demo"' || fail "/v3/paths/list missing demo path"
echo "$paths"

echo "==> GET /v3/config/paths/list"
conf_paths="$(curl -sf "$API/v3/config/paths/list")"
echo "$conf_paths" | grep -q '"demo"' || fail "/v3/config/paths/list missing demo path"
echo "$conf_paths"

echo "==> POST /v3/config/paths/add/smoke"
curl -sf -X POST "$API/v3/config/paths/add/smoke" \
  -H 'Content-Type: application/json' \
  -d '{"source":"publisher"}' >/dev/null

echo "==> DELETE /v3/config/paths/delete/smoke"
curl -sf -X DELETE "$API/v3/config/paths/delete/smoke" >/dev/null

echo "==> GET /metrics"
metrics="$(curl -sf "$METRICS")"
echo "$metrics" | grep -qE 'paths_total|# TYPE' || fail "/metrics missing paths_total or # TYPE"
echo "$metrics" | head -20

echo "==> RTSP OPTIONS * (port 18554)"
rtsp_opts="$(printf 'OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n' | nc -w 2 127.0.0.1 18554 || true)"
echo "$rtsp_opts" | grep -q 'RTSP/1.0 200' || fail "RTSP OPTIONS did not return 200"
echo "$rtsp_opts" | grep -qi 'Public:' || fail "RTSP OPTIONS missing Public header"
echo "$rtsp_opts" | head -5

echo "==> RTSP DESCRIBE rtsp://127.0.0.1:18554/demo"
rtsp_desc="$(printf 'DESCRIBE rtsp://127.0.0.1:18554/demo RTSP/1.0\r\nCSeq: 2\r\nAccept: application/sdp\r\n\r\n' | nc -w 2 127.0.0.1 18554 || true)"
echo "$rtsp_desc" | grep -q 'RTSP/1.0 200' || fail "RTSP DESCRIBE demo did not return 200"
echo "$rtsp_desc" | grep -qi 'application/sdp' || fail "RTSP DESCRIBE missing SDP content-type"
echo "$rtsp_desc" | head -12

echo "==> GET /demo/index.m3u8 (port 18889)"
playlist="$(curl -sf "$HLS/demo/index.m3u8")"
echo "$playlist" | grep -q '#EXTM3U' || fail "HLS playlist missing #EXTM3U"
echo "$playlist" | grep -q '#EXT-X-VERSION' || fail "HLS playlist missing #EXT-X-VERSION"
echo "$playlist"

echo "==> RTMP TCP connect (port 11935)"
if nc -z -w 2 127.0.0.1 11935 2>/dev/null; then
  echo "RTMP port 11935 accepting connections"
else
  fail "RTMP port 11935 not accepting TCP connections"
fi

echo "==> POST /demo/whep (port 18890)"
MINIMAL_OFFER='v=0
o=- 0 0 IN IP4 127.0.0.1
s=-
t=0 0
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=recvonly'
whep_code="$(curl -s -o /tmp/rmtx-whep.out -w '%{http_code}' -X POST "$WHEP/demo/whep" \
  -H 'Content-Type: application/sdp' \
  --data-binary "$MINIMAL_OFFER" || true)"
if [[ "$whep_code" != "501" && "$whep_code" != "201" ]]; then
  fail "WHEP POST returned HTTP $whep_code (expected 501 or 201, not connection refused)"
fi
echo "WHEP POST HTTP $whep_code"
head -5 /tmp/rmtx-whep.out || true

echo "==> POST /v3/auth/jwks/refresh"
jwks="$(curl -sf -X POST "$API/v3/auth/jwks/refresh")"
echo "$jwks" | grep -q '"status":"ok"' || fail "JWKS refresh missing status ok"

echo "==> GET /v3/recordings/get/demo"
rec="$(curl -sf "$API/v3/recordings/get/demo")"
echo "$rec" | grep -q '"name":"demo"' || fail "recordings/get missing demo name"
echo "$rec" | grep -q '"segments"' || fail "recordings/get missing segments"

echo "==> GET /v3/rtspconns/get/missing (expect 404)"
conn_code="$(curl -s -o /tmp/rmtx-conn.out -w '%{http_code}' "$API/v3/rtspconns/get/missing" || true)"
[[ "$conn_code" == "404" ]] || fail "rtspconns/get expected 404, got $conn_code"

echo "==> GET playback /list?path=demo (port 19996)"
pb_code="$(curl -s -o /tmp/rmtx-playback.out -w '%{http_code}' "$PLAYBACK/list?path=demo" || true)"
[[ "$pb_code" == "404" ]] || fail "playback /list expected 404 no segments, got HTTP $pb_code (not connection refused)"
grep -q 'no segments found' /tmp/rmtx-playback.out || fail "playback /list missing 'no segments found'"

echo "==> stopping rmtx"
kill "$PID" 2>/dev/null || true
wait "$PID" 2>/dev/null || true
PID=""

echo "PASS: Phase 1 smoke test completed successfully"
