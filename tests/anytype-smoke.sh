#!/usr/bin/env bash
# Opt-in smoke against a real Anytype space. CI does not run this, and it must
# not be added to a workflow. It writes types and objects in the named space.
#
# Required:
#   HBOTS_ANYTYPE_URL     API origin, no path (http://127.0.0.1:31012)
#   HBOTS_ANYTYPE_SPACE   space name
# and one of:
#   HBOTS_ANYTYPE_KEY     bearer key (locks pairing)
#   HBOTS_ANYTYPE_KEY_FILE
#   HBOTS_ANYTYPE_CODE    4 digits, used only when no key is set
#
# Optional: HBOTS_BIN, SMOKE_PORT (default 3796), T3_TOKEN (a dummy is enough;
# this script never chats). The gateway uses a fresh HBOTS_HOME.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -n "${CI:-}${GITHUB_ACTIONS:-}" ]; then
  echo "tests/anytype-smoke.sh is opt-in and does not run in CI" >&2
  exit 2
fi

: "${HBOTS_ANYTYPE_URL:?set HBOTS_ANYTYPE_URL}"
: "${HBOTS_ANYTYPE_SPACE:?set HBOTS_ANYTYPE_SPACE}"
if [ -z "${HBOTS_ANYTYPE_KEY:-}${HBOTS_ANYTYPE_KEY_FILE:-}${HBOTS_ANYTYPE_CODE:-}" ]; then
  echo "set HBOTS_ANYTYPE_KEY, HBOTS_ANYTYPE_KEY_FILE, or HBOTS_ANYTYPE_CODE" >&2
  exit 2
fi

export HBOTS_HOME="${SMOKE_HOME:-$(mktemp -d)}"
export HBOTS_STORAGE=anytype
export HBOTS_NETWORK_ACCESS=local
export HBOTS_PASSWORD=
export T3_TOKEN="${T3_TOKEN:-anytype-smoke}"
export PORT="${SMOKE_PORT:-3796}"
base="http://127.0.0.1:${PORT}"
mkdir -p "$HBOTS_HOME"

if [ -n "${HBOTS_BIN:-}" ]; then
  bin="$HBOTS_BIN"
elif [ -x target/debug/hbots ]; then
  bin="$PWD/target/debug/hbots"
else
  cargo build -q
  bin="$PWD/target/debug/hbots"
fi

log="$HBOTS_HOME/gateway.log"
gw=
cleanup() {
  if [ -n "${gw:-}" ]; then
    kill "$gw" 2>/dev/null || true
    wait "$gw" 2>/dev/null || true
  fi
}
trap cleanup EXIT

start_gateway() {
  if [ -n "${gw:-}" ]; then
    kill "$gw" 2>/dev/null || true
    wait "$gw" 2>/dev/null || true
  fi
  "$bin" serve >>"$log" 2>&1 &
  gw=$!
  for _ in $(seq 80); do
    curl -sf "$base/api/health" >/dev/null && return
    sleep 0.25
  done
  echo "gateway did not start; see $log" >&2
  exit 1
}

api() {
  local method="$1" path="$2" body="${3:-}"
  if [ -n "$body" ]; then
    curl -sf -X "$method" "$base$path" \
      -H 'content-type: application/json' \
      --data "$body"
  else
    curl -sf -X "$method" "$base$path"
  fi
}

anytype() {
  local method="$1" path="$2" body="${3:-}"
  local key
  key="$(anytype_key)"
  if [ -n "$body" ]; then
    curl -sf -X "$method" "${HBOTS_ANYTYPE_URL}${path}" \
      -H "authorization: Bearer ${key}" \
      -H 'Anytype-Version: 2025-11-08' \
      -H 'content-type: application/json' \
      -H "Idempotency-Key: smoke-$(date +%s)-$RANDOM" \
      --data "$body"
  else
    curl -sf -X "$method" "${HBOTS_ANYTYPE_URL}${path}" \
      -H "authorization: Bearer ${key}" \
      -H 'Anytype-Version: 2025-11-08'
  fi
}

anytype_key() {
  if [ -n "${HBOTS_ANYTYPE_KEY:-}" ]; then
    printf '%s' "$HBOTS_ANYTYPE_KEY"
  elif [ -n "${HBOTS_ANYTYPE_KEY_FILE:-}" ]; then
    tr -d '[:space:]' <"$HBOTS_ANYTYPE_KEY_FILE"
  else
    tr -d '[:space:]' <"$HBOTS_HOME/anytype-key"
  fi
}

wait_status() {
  local want="$1" tries="${2:-60}"
  local status
  for _ in $(seq "$tries"); do
    status="$(api GET /api/sync/status || true)"
    if printf '%s' "$status" | python3 -c 'import json,sys
status=json.load(sys.stdin)
want=sys.argv[1]
ok=status.get("running") and status.get("phase")=="live" and status.get("outbox")==0 and status.get("state")=="live"
if want=="conflict":
    ok=ok and len(status.get("conflicts") or [])>=1
if want=="clear":
    ok=ok and not status.get("conflicts")
raise SystemExit(0 if ok else 1)' "$want"; then
      printf '%s' "$status"
      return
    fi
    api POST /api/sync/run '{}' >/dev/null || true
    sleep 2
  done
  echo "sync did not reach $want; last status: ${status:-}" >&2
  exit 1
}

start_gateway

if [ -z "${HBOTS_ANYTYPE_KEY:-}${HBOTS_ANYTYPE_KEY_FILE:-}" ]; then
  challenge="$(api POST /api/storage/anytype/pair | python3 -c 'import json,sys; print(json.load(sys.stdin)["challengeId"])')"
  api POST /api/storage/anytype/pair/confirm \
    "$(python3 -c 'import json,sys; print(json.dumps({"challengeId":sys.argv[1],"code":sys.argv[2]}))' "$challenge" "$HBOTS_ANYTYPE_CODE")" \
    >/dev/null
fi

api POST /api/storage/anytype/setup '{"dryRun":false}' >/dev/null
start_gateway

running="$(api GET /api/sync/status)"
printf '%s' "$running" | python3 -c 'import json,sys
status=json.load(sys.stdin)
raise SystemExit(0 if status.get("running") else 1)' \
  || { echo "sync engine is off: $running" >&2; exit 1; }

slug="$(api POST /api/bots '{"name":"Smoke"}' | python3 -c 'import json,sys; print(json.load(sys.stdin)["slug"])')"
api POST /api/sync/import '{"dryRun":false}' >/dev/null

rel="${slug}/memory/note.md"
file="$HBOTS_HOME/bots/$rel"
mkdir -p "$(dirname "$file")"
printf 'alpha\nmiddle\nomega\n' >"$file"
wait_status live 90 >/dev/null

space="$(anytype GET "/v2/spaces?ids=full" | python3 -c 'import json,sys,os
space=os.environ["HBOTS_ANYTYPE_SPACE"]
rows=json.load(sys.stdin).get("data") or []
for row in rows:
    if row.get("name")==space:
        print(row["id"])
        raise SystemExit(0)
raise SystemExit(1)')"

object="$(anytype POST "/v2/spaces/${space}/search" '{"type":"hb_file"}' | REL="$rel" python3 -c 'import json,sys,os
want=os.environ["REL"]
rows=json.load(sys.stdin).get("data") or []
for row in rows:
    props=row.get("properties") or {}
    if props.get("hb_path")==want and not props.get("hb_deleted"):
        print(row["id"])
        raise SystemExit(0)
raise SystemExit(1)')"

printf 'alpha\nLOCAL\nomega\n' >"$file"
python3 - "$object" <<'PY' >"$HBOTS_HOME/patch.json"
import hashlib, json, os, sys
text = "alpha\nREMOTE\nomega\n"
print(json.dumps({
    "ops": [{
        "op": "set_properties",
        "set": {
            "hb_content": text,
            "hb_sha": hashlib.sha256(text.encode()).hexdigest(),
        },
    }],
}))
PY
anytype PATCH "/v2/spaces/${space}/objects/${object}" "$(cat "$HBOTS_HOME/patch.json")" >/dev/null

conflict="$(wait_status conflict 90)"
cid="$(printf '%s' "$conflict" | python3 -c 'import json,sys; print(json.load(sys.stdin)["conflicts"][0]["id"])')"
api POST "/api/sync/conflicts/${cid}/resolve" '{"keep":"local"}' >/dev/null
wait_status clear 90 >/dev/null
grep -q 'LOCAL' "$file"

rm "$file"
wait_status live 90 >/dev/null
anytype POST "/v2/spaces/${space}/search" '{"type":"hb_file"}' | REL="$rel" python3 -c 'import json,sys,os
want=os.environ["REL"]
rows=json.load(sys.stdin).get("data") or []
for row in rows:
    props=row.get("properties") or {}
    if props.get("hb_path")==want:
        raise SystemExit(0 if props.get("hb_deleted") else 1)
raise SystemExit(1)'

echo "anytype smoke ok ($rel)"
