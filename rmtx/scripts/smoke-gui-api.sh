#!/usr/bin/env bash
# GUI Control API acceptance test — validates JSON shapes expected by gui/composables/useMediaMtx.ts
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

API="http://127.0.0.1:19997"
CONFIG="$ROOT/testdata/phase1.yml"
BIN="$ROOT/target/debug/rmtx"
GOLDEN="$ROOT/testdata/gui-paths-list-golden.json"
PID=""

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

cleanup() {
  if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

require_cmd curl
require_cmd jq

echo "==> building rmtx"
cargo build -p rmtx --quiet

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

echo "==> GET /v3/paths/list — items[] with name (camelCase)"
paths_json="$(curl -sf "$API/v3/paths/list")"
echo "$paths_json" | jq -e 'type == "object"' >/dev/null || fail "paths/list response is not a JSON object"
echo "$paths_json" | jq -e '.items | type == "array"' >/dev/null || fail "paths/list missing items array"
echo "$paths_json" | jq -e 'has("paths") | not' >/dev/null || fail "paths/list must use items not paths"
echo "$paths_json" | jq -e '.pageCount | type == "number"' >/dev/null || fail "paths/list missing pageCount"
echo "$paths_json" | jq -e '.itemCount | type == "number"' >/dev/null || fail "paths/list missing itemCount"
echo "$paths_json" | jq -e '.items[] | has("name") and (.name | type == "string")' >/dev/null \
  || fail "paths/list items must have string name"
echo "$paths_json" | jq -e '[.items[].name] | index("demo") != null' >/dev/null \
  || fail "paths/list missing demo path"
echo "$paths_json" | jq -e '[.items[].name] | index("placeholder") != null' >/dev/null \
  || fail "paths/list missing placeholder path"
echo "$paths_json" | jq -e '.items[] | select(.name=="placeholder") | .available == true and .ready == true and .online == false' >/dev/null \
  || fail "placeholder path must be available/ready and not online"
echo "$paths_json" | jq -e '.items[] | has("confName")' >/dev/null \
  || fail "paths/list items must use camelCase confName"

if [[ -f "$GOLDEN" ]]; then
  echo "==> compare /v3/paths/list shape against golden (subset)"
  # Drop volatile timestamps and sort items by name for stable diffs.
  echo "$paths_json" | jq -S '
    del(.items[].readyTime, .items[].availableTime, .items[].onlineTime, .items[].source)
    | .items |= sort_by(.name)
  ' > /tmp/rmtx-paths-list.actual.json
  jq -S '.items |= sort_by(.name)' "$GOLDEN" > /tmp/rmtx-paths-list.golden.json
  diff -u /tmp/rmtx-paths-list.golden.json /tmp/rmtx-paths-list.actual.json \
    || fail "paths/list JSON shape differs from golden (see diff above)"
fi

echo "==> GET /v3/config/paths/list — items[]"
conf_json="$(curl -sf "$API/v3/config/paths/list")"
echo "$conf_json" | jq -e '.items | type == "array"' >/dev/null || fail "config/paths/list missing items array"
echo "$conf_json" | jq -e '.pageCount | type == "number"' >/dev/null || fail "config/paths/list missing pageCount"
echo "$conf_json" | jq -e '.itemCount | type == "number"' >/dev/null || fail "config/paths/list missing itemCount"
echo "$conf_json" | jq -e '[.items[].name] | index("demo") != null' >/dev/null \
  || fail "config/paths/list missing demo path"

echo "==> POST /v3/config/paths/add/gui-smoke"
curl -sf -X POST "$API/v3/config/paths/add/gui-smoke" \
  -H 'Content-Type: application/json' \
  -d '{"source":"publisher"}' \
  | jq -e '.status == "ok"' >/dev/null \
  || fail "add path response missing status ok"

echo "==> PATCH /v3/config/paths/patch/gui-smoke"
curl -sf -X PATCH "$API/v3/config/paths/patch/gui-smoke" \
  -H 'Content-Type: application/json' \
  -d '{"source":"publisher"}' \
  | jq -e '.status == "ok"' >/dev/null \
  || fail "patch path response missing status ok"

echo "==> verify gui-smoke appears in config list"
curl -sf "$API/v3/config/paths/list" \
  | jq -e '[.items[].name] | index("gui-smoke") != null' >/dev/null \
  || fail "gui-smoke path not in config list after add"

echo "==> DELETE /v3/config/paths/delete/gui-smoke"
curl -sf -X DELETE "$API/v3/config/paths/delete/gui-smoke" \
  | jq -e '.status == "ok"' >/dev/null \
  || fail "delete path response missing status ok"

echo "==> verify gui-smoke removed from config list"
curl -sf "$API/v3/config/paths/list" \
  | jq -e '[.items[].name] | index("gui-smoke") == null' >/dev/null \
  || fail "gui-smoke path still in config list after delete"

echo "==> stopping rmtx"
kill "$PID" 2>/dev/null || true
wait "$PID" 2>/dev/null || true
PID=""

echo "PASS: GUI Control API smoke test completed successfully"
