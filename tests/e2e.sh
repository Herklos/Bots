#!/usr/bin/env bash
# End to end, with no T3 Code installed: a mock T3 (tests/mock-t3.ts), the real gateway serving the
# Exact web build, and Chromium creating a Bot and chatting with it. Needs bun, cargo, Playwright.
#   tests/e2e.sh            builds the web app first unless app/target/web-dist exists
set -euo pipefail
cd "$(dirname "$0")/.."
out="${E2E_OUT:-$(mktemp -d)}"; mkdir -p "$out"
[ -d app/target/web-dist ] || { echo "Build the web app first: EXACT_APP_DIR=\$PWD/app bun <exact>/host/web/build.mjs bots-web" >&2; exit 2; }
cargo build -q
export T3_TOKEN=e2e-token PORT=3790 STATIC_DIR="$PWD/app/target/web-dist" THREEBOT_HOME="$(mktemp -d)" T3_URL=http://127.0.0.1:3791
bun tests/mock-t3.ts 3791 >"$out/mock.log" 2>&1 & mock=$!
./target/debug/bots >"$out/gateway.log" 2>&1 & gw=$!
trap 'kill $mock $gw 2>/dev/null || true' EXIT
for _ in $(seq 40); do curl -sf localhost:3790/api/health >/dev/null && break; sleep 0.25; done
BOTS_E2E_URL=http://127.0.0.1:3790 node tests/e2e.cjs "$out"
echo "screenshots and logs: $out"
