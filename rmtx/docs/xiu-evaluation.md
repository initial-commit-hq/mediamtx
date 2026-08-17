# Xiu / RTMP crate evaluation

Date: 2026-08-13  
Scope: `rmtx-servers-rtmp` Phase 1 scaffold → Phase 1.5/2 integration planning.

## Summary

| Crate | crates.io | Library? | Fit for rmtx |
|---|---|---|---|
| `xiu` 0.13.0 | yes | Monolithic server app | Reference only — too heavy to embed |
| `streamhub` 0.2.4 | yes | Pub/sub hub | Phase 2 fan-out bridge (with `rtmp`) |
| `rtmp` 0.6.5 | yes | RTMP protocol + server | **Recommended Phase 1.5/2 path** (Xiu ecosystem) |
| `rml_rtmp` 0.8.0 | yes | Protocol primitives (sans-IO) | Alternative if we build our own session loop |
| `rtmp-rs` 0.5.0 | yes | Client/server library | Less aligned with Xiu; evaluate separately |
| `oxideav-rtmp` 0.0.6 | yes | Ingest + push | Newer; API still evolving |

Go MediaMTX uses [`gortmplib`](https://github.com/bluenviron/gortmplib), not Xiu. The Rust port brief targets the harlanc/Xiu stack for RTMP server parity.

## Can we use Xiu as a library?

**Partially — not as a drop-in embeddable server like `gortmplib`.**

The `xiu` crate exposes `xiu::service::Service` with `Service::new(cfg).run()`. Internally it wires:

- `rtmp::rtmp::RtmpServer`
- `streamhub::StreamsHub`
- HLS, HTTP-FLV, RTSP (`xrtsp`), WebRTC (`xwebrtc`), auth, HTTP notify hooks

It is designed as a standalone live media **application**, not a composable library crate for another media router. Embedding full `xiu` would duplicate rmtx’s own HLS/WebRTC/API layers and pull a large dependency tree (WebRTC stack, codec crates, etc.).

**Verdict:** Use `xiu` source/docs as architecture reference. Do **not** depend on the `xiu` crate in rmtx Phase 2.

## Harlanc crate family (Xiu ecosystem)

All published from [github.com/harlanc/xiu](https://github.com/harlanc/xiu):

### `rtmp` (0.6.5)

- RTMP handshake, chunk protocol, netconnection/netstream, server sessions.
- `rtmp::rtmp::RtmpServer` accepts TCP connections and spawns `ServerSession` tasks.
- Requires `streamhub::StreamHubEventSender` for publisher/subscriber events.
- **Compiles** in isolation (verified 2026-08-13).
- Added to `rmtx-servers-rtmp` as optional dep behind feature `xiu-rtmp` (default off).

### `streamhub` (0.2.4)

- `StreamsHub` — central pub/sub for live streams.
- Used by RTMP/HLS/HTTP-FLV in Xiu; natural bridge to `rmtx-stream` in Phase 2.
- Not an RTMP listener by itself.

### `commonlib`, `hls`, `httpflv`, `xrtsp`, `xwebrtc`

Supporting crates inside the Xiu monorepo. Only needed if we replicate Xiu’s full multi-protocol server — out of scope for rmtx.

## Alternative pure-Rust RTMP crates

### `rml_rtmp` (KallDrexx / rust-media-libs)

- Mature RTMP **protocol** handling (handshake, AMF, chunk streams).
- Sans-IO / session-oriented; you own the TCP loop and state machine.
- Good if we want maximum control and minimal transitive deps.
- Does not provide Xiu-style stream hub or MediaMTX-shaped session objects.

### `rtmp-rs`

- Client/server library (torresjeff/rtmp-rs).
- Compiles; less documentation and no obvious MediaMTX/Xiu alignment.

### `oxideav-rtmp`

- Pure-Rust ingest + push from the oxideav project.
- Very early (0.0.x); worth re-evaluating before Phase 2 if harlanc stack stalls.

### `rtmp-runtime` (0.6.0)

- Sans-IO RTMP 1.0 ingest session engine.
- Interesting for a custom server loop; overlaps with `rml_rtmp` approach.

## Compile verification

```bash
# Default scaffold (no Xiu deps)
cargo test -p rmtx-servers-rtmp

# Optional harlanc RTMP stack probe
cargo check -p rmtx-servers-rtmp --features xiu-rtmp
```

Both succeed as of Phase 1 scaffold.

## Recommendation

### Phase 1 (current)

- Types only: `RtmpServer`, `RtmpConn`, `RtmpConnState` aligned with OpenAPI.
- Optional `rtmp` dep behind `xiu-rtmp` feature — compile probe, no runtime wiring.
- No full `xiu` dependency.

### Phase 1.5

1. Enable `xiu-rtmp` in CI matrix.
2. Stand up minimal `rtmp::RtmpServer` + `StreamsHub` listener on configurable address.
3. Map `ServerSession` lifecycle → `RtmpConnState` (`idle` → `publish` / `read`).
4. Log-only or stub path attach (no `rmtx-path` yet).

### Phase 2

1. Bridge `streamhub` publisher output into `rmtx-stream` / path manager (replace or wrap `StreamsHub`).
2. Wire auth (`rmtx-auth`), hooks (`rmtx-hooks`), Control API `/v3/rtmpconns/*`.
3. RTMPS via rustls (mirror Go TLS listener).
4. RTMP **pull sources** in `rmtx-staticsources` can reuse `rtmp` relay client or defer to ffmpeg fallback.

### Fallback if harlanc stack is insufficient

Re-evaluate `oxideav-rtmp` or a thin server on `rml_rtmp` + custom fan-out to `rmtx-stream`. Porting Go’s `gortmplib` patterns to Rust is not available as a crate today.

## References

- [xiu on crates.io](https://crates.io/crates/xiu)
- [rtmp on crates.io](https://crates.io/crates/rtmp)
- [streamhub on crates.io](https://crates.io/crates/streamhub)
- [rml_rtmp on crates.io](https://crates.io/crates/rml_rtmp)
- OpenAPI: `RTMPConnState` = `idle | read | publish`
