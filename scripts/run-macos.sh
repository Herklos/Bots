#!/bin/sh
# Dev desktop entry for the macOS Exact app: make sure the gateway answers, then open the app.
# Interim until the Exact host owns the gateway child (docs/native.md, "Desktop shell").
set -eu

root=$(CDPATH='' cd -- "$(dirname "$0")/.." && pwd)
url=${HBOTS_URL:-http://127.0.0.1:3780}
health="$url/api/health"
log="$root/target/serve.log"
started=

if ! curl -fsS -m 2 "$health" >/dev/null 2>&1; then
  cd "$root"
  if [ ! -x "$root/target/release/hbots" ]; then
    cargo build --release --bin hbots
  fi
  PATH="$HOME/.t3/bin:$PATH" "$root/target/release/hbots" serve >"$log" 2>&1 &
  started=$!
  i=0
  until curl -fsS -m 2 "$health" >/dev/null 2>&1; do
    i=$((i + 1))
    if [ "$i" -ge 60 ]; then
      echo "run-macos: gateway did not answer $health; see $log" >&2
      exit 1
    fi
    sleep 1
  done
fi

# An existing bundle keeps the gateway URL it was built with (app/app.ts inlines HBOTS_URL).
bundle=$(ls -td "$root"/app/target/clients/*/*/macos/hbots.app 2>/dev/null | head -n 1 || true)
if [ -n "$bundle" ]; then
  open "$bundle"
  exit 0
fi

if [ ! -f "$root/.exact-path" ]; then
  echo "run-macos: no .exact-path and no app bundle; see AGENTS.md (Exact checkout)" >&2
  exit 1
fi
set -a
. "$root/.exact-path"
set +a

# --run blocks until the app quits; then stop the gateway this script started.
if [ -n "$started" ]; then
  trap 'kill "$started" 2>/dev/null || true' EXIT
fi
HBOTS_URL="$url" EXACT_APP_DIR="$root/app" bun "$EXACT_DIR/host/apple/build.mjs" --run
