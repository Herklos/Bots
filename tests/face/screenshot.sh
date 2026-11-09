#!/bin/sh
# Renders the Face gallery (app/face.contract) in light and dark, for a person to look at.
#
#   EXACT_DIR=<exact checkout> [OUT=<dir>] [CHROME=<browser>] sh tests/face/screenshot.sh
#
# It copies app/face.contract and tests/face/app.contract into a scratch app, builds it with the
# Exact web build, then drives it with scripts/agent.mjs and writes light.png and dark.png to OUT.
# Requires an Exact checkout (EXACT_DIR) with its dependencies installed, and a Chrome that can
# run headless (CHROME). The scratch app is removed unless KEEP=1.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
: "${EXACT_DIR:?set EXACT_DIR to an Exact checkout}"
OUT=${OUT:-$(mktemp -d)}
mkdir -p "$OUT"

APP=$(mktemp -d)
trap '[ "${KEEP:-0}" = 1 ] || rm -rf "$APP"' EXIT
cp "$REPO/app/face.contract" "$REPO/tests/face/app.contract" "$APP/"
cat > "$APP/app.json" <<'EOF'
{
  "name": "FaceTest",
  "short_name": "FaceTest",
  "app": { "id": "software.drakkar.facetest", "name": "Face test" },
  "host": { "web": {} }
}
EOF
cat > "$APP/app.ts" <<'EOF'
import type { Answer } from './app.contract.d.ts';

// The face test app has no data: no grants, and no source to answer.
export const grants = '';
export const answer: Answer = () => { throw new Error('the face test app has no sources'); };
EOF

(cd "$EXACT_DIR" && EXACT_APP_DIR="$APP" EXACT_WEB_DIST="$APP/target/web-dist" bun host/web/build.mjs "$(basename "$APP")-web")

cd "$APP"
EXACT_APP_DIR="$APP" bun "$EXACT_DIR/scripts/agent.mjs" web --size 560x520 \
  "clock settle" "screenshot $OUT/light.png" \
  "prefer prefers-color-scheme dark" "clock settle" "screenshot $OUT/dark.png"
echo "wrote $OUT/light.png and $OUT/dark.png"
