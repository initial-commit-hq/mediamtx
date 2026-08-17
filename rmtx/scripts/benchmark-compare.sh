#!/usr/bin/env bash
# Compare Go MediaMTX vs Rust rmtx under the same Control API load.
#
# Usage:
#   ./scripts/benchmark-compare.sh [go|rmtx|both]
#
# Environment:
#   DURATION_SECS   default 15
#   CONCURRENCY     default 4
#   RESULTS_DIR     default benchmark-results/
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO_ROOT="$(cd "$ROOT/.." && pwd)"
IMPL="${1:-both}"
DURATION_SECS="${DURATION_SECS:-15}"
CONCURRENCY="${CONCURRENCY:-4}"
RESULTS_DIR="${RESULTS_DIR:-$ROOT/benchmark-results}"
CONFIG="$ROOT/testdata/benchmark.yml"
API="http://127.0.0.1:29997/v3/paths/list?page=0&itemsPerPage=100"
METRICS_GO="http://127.0.0.1:29998/metrics"
METRICS_RUST="http://127.0.0.1:29998/metrics"
RMTX_BIN="$ROOT/target/release/rmtx"
GO_BIN="$REPO_ROOT/mediamtx"
BENCH_BIN="$ROOT/target/release/rmtx-bench"

mkdir -p "$RESULTS_DIR"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

wait_api() {
  for _ in $(seq 1 60); do
    if curl -sf "http://127.0.0.1:29997/v3/info" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.2
  done
  fail "control API did not become ready"
}

run_suite() {
  local name=$1
  local pid=$2
  local metrics_url=$3
  local metrics_format=$4
  local out=$5

  "$BENCH_BIN" suite \
    --implementation "$name" \
    --api-url "$API" \
    --metrics-url "$metrics_url" \
    --metrics-format "$metrics_format" \
    --pid "$pid" \
    --duration-secs "$DURATION_SECS" \
    --concurrency "$CONCURRENCY" \
    >"$out"
}

bench_rust() {
  echo "==> building rmtx + rmtx-bench (release)"
  cargo build -p rmtx -p rmtx-bench --release --quiet

  echo "==> starting rmtx"
  "$RMTX_BIN" "$CONFIG" &
  local pid=$!
  trap 'kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true' RETURN
  wait_api

  local out="$RESULTS_DIR/${STAMP}-rmtx.json"
  echo "==> running suite (rmtx) -> $out"
  run_suite rmtx "$pid" "$METRICS_RUST" prometheus "$out"
  echo "Wrote $out"

  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  trap - RETURN
}

bench_go() {
  echo "==> building Go mediamtx"
  (cd "$REPO_ROOT" && go build -o "$GO_BIN" .)

  if [[ ! -x "$BENCH_BIN" ]]; then
    cargo build -p rmtx-bench --release --quiet
  fi

  echo "==> starting Go mediamtx"
  "$GO_BIN" "$CONFIG" &
  local pid=$!
  trap 'kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true' RETURN
  wait_api

  local out="$RESULTS_DIR/${STAMP}-go.json"
  echo "==> running suite (go) -> $out"
  run_suite go "$pid" "$METRICS_GO" mediamtx "$out"
  echo "Wrote $out"

  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  trap - RETURN
}

case "$IMPL" in
  rmtx)
    bench_rust
    ;;
  go)
    bench_go
    ;;
  both)
    bench_go
    bench_rust
    go_json="$RESULTS_DIR/${STAMP}-go.json"
    rust_json="$RESULTS_DIR/${STAMP}-rmtx.json"
    if [[ -f "$go_json" && -f "$rust_json" ]]; then
      echo "==> comparison (positive delta% => rmtx higher than go for that metric)"
      "$BENCH_BIN" compare --go "$go_json" --rust "$rust_json"
    fi
    ;;
  *)
    fail "unknown impl: $IMPL (use go, rmtx, or both)"
    ;;
esac

echo "PASS: benchmark finished"
