# rmtx — Rust port of MediaMTX

Memory-safe reimplementation of this MediaMTX fork. Spec: `.claude/rust-port.md` (local).

**Current milestone: Phase 3 media delivery in progress** (Phase 2 complete).

Phase 0 spec extraction is under [`spec/`](spec/). Phase 1 brought up the binary, Control API, path manager, hooks, auth, and stream bus. Phase 2 adds media forwarding, metrics, protocol listeners, and ffmpeg fallback.

## Phase status

| Component | Status |
|---|---|
| `rmtx` binary | CLI, config load, hot reload, Ctrl+C shutdown |
| Control API | `/v3/info`, paths list/get, full `/v3/config/*` path CRUD, session list/get/kick stubs, recordings get, JWKS refresh |
| Config hot reload | File watcher + path registry refresh |
| `rmtx-path` | Runtime registry + ready transitions + hook firing |
| `rmtx-hooks` | Env contract + `HookRunner` (`runOnReady` / `runOnNotReady`) |
| `rmtx-auth` | Internal auth wired to Control API (Basic when users set) |
| `rmtx-stream` | `StreamBus` fan-out (`Track` / `MediaUnit`); **`StreamLookup`** for RTSP/HLS/WHEP |
| `rmtx-metrics` | Prometheus `/metrics` endpoint |
| `rmtx-bench` | Load generators + Go vs Rust comparison CLI |
| `rmtx-mux` | MPEG-TS segment builder |
| `rmtx-record` | Placeholder segments on path ready when `record: yes`; **StreamBus tap** appends media units |
| `rmtx-servers-rtsp` | Retina PLAY pull → `StreamBus`; listener OPTIONS/DESCRIBE/SETUP/PLAY/TEARDOWN; **TCP interleaved RTP media pump** to PLAY clients |
| `rmtx-servers-hls` | HTTP HLS; **live segment cache** fed from `StreamLookup` when a path has media (stub TS payloads until real mux) |
| `rmtx-servers-rtmp` | RTMP listener via Xiu (`xiu-rtmp` feature); **publish → `on_rtmp_publish`** attaches StreamBus + record tap |
| `rmtx-servers-webrtc` | WHEP HTTP endpoint; **501** without `webrtc-rs`, **201** SDP answer with `webrtc-rs`; **WHEP subscribe scaffold** logs StreamBus units (RTP send Phase 3) |
| `rmtx-playback` | Playback HTTP `GET /list` + `GET /get`; **lists/serves `.mp4`/`.ts` under `record_path`** |
| `rmtx-staticsources` | RTSP PLAY pull via `spawn_configured_sources`; **ffmpeg MPEG-TS pipe** or RTSP republish argv when Retina pull fails |
| `rmtx-ffmpeg` | Argv-only ffmpeg spawn (`-rtsp_transport tcp -i … -c copy -f rtsp …`); no shell |

### Ports (smoke / `testdata/phase1.yml`)

| Service | Address |
|---|---|
| Control API | `:19997` |
| Prometheus metrics | `:19998` |
| RTSP | `:18554` |
| HLS | `:18889` |
| WebRTC / WHEP | `:18890` |
| Playback | `:19996` |
| RTMP (with `xiu-rtmp`) | `:11935` |

### Control API

MediaMTX defaults to `api: false`. For bring-up and tests, enable it explicitly:

```yaml
api: yes
apiAddress: :9997
metrics: yes
metricsAddress: :9998
```

Then:

```bash
curl -s http://127.0.0.1:9997/v3/info
curl -s 'http://127.0.0.1:9997/v3/paths/list?page=0&itemsPerPage=100'
curl -s http://127.0.0.1:9998/metrics
curl -s -X POST http://127.0.0.1:9997/v3/auth/jwks/refresh
curl -s http://127.0.0.1:9997/v3/recordings/get/demo
```

Paths with `source.id` of `"ffmpeg-fallback"` were started via `rmtx-ffmpeg` after Retina pull failed (observable in `/v3/paths/get`).

### Benchmarking (Go vs Rust)

Compare Control API throughput, latency percentiles, RSS/CPU, and path byte counters under the same config:

```bash
cd rmtx
chmod +x scripts/benchmark-compare.sh
# Go only, rmtx only, or side-by-side:
./scripts/benchmark-compare.sh both
```

Uses [`testdata/benchmark.yml`](testdata/benchmark.yml) (ports `29997`/`29998`). JSON reports land in `benchmark-results/`; `both` prints a comparison table.

Manual tools via `rmtx-bench`:

```bash
cargo build -p rmtx-bench --release
./target/release/rmtx-bench api-load --url 'http://127.0.0.1:29997/v3/paths/list' --duration-secs 10 --concurrency 8
./target/release/rmtx-bench micro stream-fanout --subscribers 16 --units 100000
curl -s 'http://127.0.0.1:29998/metrics?format=mediamtx'   # rmtx path metrics in Go scrape shape
```

rmtx exposes Prometheus on `GET /metrics` by default; add `?format=mediamtx` for MediaMTX-compatible `paths_*` lines when diffing scrapes.

### Smoke tests

Live integration check (build, start server, Control API + metrics + protocol round-trip, shutdown):

```bash
cd rmtx
chmod +x scripts/smoke-phase1.sh scripts/smoke-gui-api.sh   # once
./scripts/smoke-phase1.sh
./scripts/smoke-gui-api.sh
```

Uses [`testdata/phase1.yml`](testdata/phase1.yml). `smoke-phase1.sh` builds with `--features xiu-rtmp`, probes RTSP/HLS/RTMP/WHEP/playback, and accepts WHEP **501** (default) or **201** (with `webrtc-rs`).

## Crate layout (mirrors Go `internal/`)

| Rust crate | Go counterpart | Role |
|---|---|---|
| `rmtx` | `main.go` + `internal/core` | Binary + process orchestration |
| `rmtx-conf` | `internal/conf` | YAML/env config, hot reload |
| `rmtx-api` | `internal/api` | Control API (axum), OpenAPI-compatible |
| `rmtx-path` | `internal/core` path manager | Path registry, pub/sub, always-available |
| `rmtx-hooks` | `internal/hooks` + `internal/externalcmd` | Lifecycle hooks |
| `rmtx-auth` | `internal/auth` | Internal / HTTP / JWT auth |
| `rmtx-metrics` | `internal/metrics` | Prometheus `/metrics` |
| `rmtx-record` | `internal/recorder` | fMP4 / MPEG-TS recording |
| `rmtx-playback` | `internal/playback` | On-demand recording playback (list/get scaffold) |
| `rmtx-stream` | `internal/stream` | Shared media units / fan-out bus |
| `rmtx-servers-rtsp` | `internal/servers/rtsp` | RTSP server; Retina client ingest |
| `rmtx-servers-rtmp` | `internal/servers/rtmp` | RTMP (Xiu-based) |
| `rmtx-servers-hls` | `internal/servers/hls` | HLS / LL-HLS |
| `rmtx-servers-webrtc` | `internal/servers/webrtc` | WHIP / WHEP (`webrtc` crate) |
| `rmtx-servers-srt` | `internal/servers/srt` | SRT (FFI/ffmpeg TBD — known gap) |
| `rmtx-staticsources` | `internal/staticsources` | Pull sources (RTSP/HLS/…) |
| `rmtx-ffmpeg` | (new) | Per-path ffmpeg fallback subprocess |

## Build & run

```bash
cd rmtx
cargo check --workspace
cargo run -p rmtx -- ../mediamtx.yml   # from repo root config
```

With RTMP listener:

```bash
cargo run -p rmtx --features xiu-rtmp -- testdata/phase1.yml
```

Exit codes: `0` on normal run / graceful shutdown; `2` if config is missing or invalid.

## Retina RTSP ingest & ffmpeg fallback

`rmtx-servers-rtsp` enables the [`retina`](https://docs.rs/retina) crate by default (`retina` feature). Configured `rtsp://` / `rtsps://` paths start a Retina DESCRIBE + PLAY pull into `StreamBus`.

When Retina pull fails, `rmtx-staticsources` tries **ffmpeg stdout MPEG-TS pipe** ingest into `StreamBus` first, then falls back to argv-only RTSP republish (`rmtx-ffmpeg::FfmpegPull`). If ffmpeg is missing, the path stays not-ready and a warning is logged. If ffmpeg starts, the path is marked ready with `source.type = rtspSource` and `source.id = ffmpeg-fallback`.

Disable Retina (stub ingest):

```bash
cargo check -p rmtx-servers-rtsp --no-default-features
```

## Open risks (from brief)

1. **SRT** — no mature pure-Rust stack; decide FFI to libsrt vs ffmpeg fallback vs defer.
2. **Retina** — gaps (UDP reorder, ONVIF backchannel/replay, minimal RTCP SR) → ffmpeg fallback (spawn wired; full republish loop Phase 2+).
3. **GUI compatibility** — verify against real responses, not just routes (`spec/gui-api-surface.json`).
4. Multi-month project; scope milestones accordingly.

## Next (Phase 3+)

JWKS RS256 signature validation, WebRTC RTP send (`TrackLocalStaticRTP`), fMP4 mux, RTMP A/V → StreamBus, HTTP auth backend, SRT decision.
