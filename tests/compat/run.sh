#!/usr/bin/env bash
# Home-directory compatibility (T10).
#
# One home, two hbots processes. The home is ~/.hbots (HBOTS_HOME): bots/<slug>/.bot/bot.json,
# bots/.botapp/ (main-bot.json, projects.json, settings.json, session-secret), vault.enc and
# .vault-key. Each process logs in, and the other's bot_session cookie must still be accepted
# because both read the same session-secret.
#
# An earlier server checkout is not on this machine, so both sides of the cookie check are
# hbots processes. The script says so. It does not invent a result for that checkout.
#
# Usage: tests/compat/run.sh
# Env:   HBOTS_SRC     earlier server checkout (default: the scratchpad clone)
#        HBOTS_BIN     hbots binary (default: cargo build of gateway/)
#        COMPAT_WORK   scratch directory (default: a new mktemp directory)
#        COMPAT_PORT   port (default 37810)
# Exit:  0 when every check passes, 1 otherwise, 2 when setup fails.
set -u

HERE=$(CDPATH='' cd -- "$(dirname "$0")" && pwd)
REPO=$(CDPATH='' cd -- "$HERE/../.." && pwd)
HBOTS_SRC=${HBOTS_SRC:-/tmp/claude-0/-home-user-Bots/0044585a-cb11-55a6-921c-f38153a8540a/scratchpad/src/hbots}
WORK=${COMPAT_WORK:-$(mktemp -d "${TMPDIR:-/tmp}/hbots-compat.XXXXXX")}
PORT=${COMPAT_PORT:-37810}
PASS_WORD=compat-pass
PASSED=0
FAILED=0
PIDS=()

ok() { PASSED=$((PASSED + 1)); echo "PASS $1 $2"; }
bad() { FAILED=$((FAILED + 1)); echo "FAIL $1 $2${3:+ (got: $3)}"; }
info() { echo "INFO $1 $2"; }
expect() {
  if [ "$3" = "$4" ]; then ok "$1" "$2"; else bad "$1" "$2" "$3"; fi
}
jv() {
  node -e 'const j = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")); const v = eval(process.argv[2]); console.log(typeof v === "string" ? v : JSON.stringify(v));' "$1" "$2" 2>/dev/null || echo "<unreadable>"
}
leaks() { if grep -q -F -- "$2" "$1" 2>/dev/null; then echo yes; else echo no; fi; }

code() {
  local args=(-s -o "$WORK/last.body" -w '%{http_code}' -X "$1" "http://127.0.0.1:$PORT$2")
  [ -n "$3" ] && args+=(-H "Cookie: $(cat "$3")")
  [ -n "$4" ] && args+=(-H 'content-type: application/json' --data-binary "$4")
  curl "${args[@]}" 2>/dev/null || echo 000
}
login() {
  curl -s -D - -o /dev/null -X POST -H 'content-type: application/json' \
    --data-binary "{\"password\":\"$PASS_WORD\"}" "http://127.0.0.1:$PORT/api/login" \
    | tr -d '\r' | sed -n 's/^[Ss]et-[Cc]ookie: \(bot_session=[^;]*\).*/\1/p' > "$1"
}

start_at() {
  local port=$1 home=$2
  local log="$WORK/logs/hbots-$port.log"
  mkdir -p "$WORK/logs"
  env HBOTS_HOME="$home" PORT="$port" HOST=127.0.0.1 HBOTS_PASSWORD="$PASS_WORD" \
    T3_TOKEN=compat-token T3_URL=http://127.0.0.1:9 "$HBOTS_BIN" serve >"$log" 2>&1 &
  PIDS+=("$!")
  local i=0
  while [ "$i" -lt 100 ]; do
    if curl -s -o /dev/null "http://127.0.0.1:$port/api/health"; then return 0; fi
    i=$((i + 1))
    sleep 0.2
  done
  echo "hbots on port $port did not answer /api/health. Log $log:"
  tail -20 "$log"
  exit 2
}
stop() {
  local p
  if [ "${#PIDS[@]}" -gt 0 ]; then
    for p in "${PIDS[@]}"; do
      kill "$p" 2>/dev/null || true
      wait "$p" 2>/dev/null || true
    done
  fi
  PIDS=()
  sleep 0.3
}
trap stop EXIT

fixtures() {
  local home=$1
  mkdir -p "$home/bots/b-ada/.bot" "$home/bots/b-grace/.bot"
  cp "$HERE/fixtures/bot-ada.json" "$home/bots/b-ada/.bot/bot.json"
  cp "$HERE/fixtures/bot-grace.json" "$home/bots/b-grace/.bot/bot.json"
}

echo "== setup (work dir: $WORK)"
if [ -d "$HBOTS_SRC/server" ]; then
  info node "Node gateway checkout is at $HBOTS_SRC/server, but this harness does not start it. Both cookie sides below are hbots processes."
else
  info node "No Node gateway at $HBOTS_SRC/server. The Node side of each check was not run. Both cookie sides are hbots processes sharing session-secret."
fi

if [ -z "${HBOTS_BIN:-}" ]; then
  if cargo +stable version >/dev/null 2>&1; then CARGO=(cargo +stable); else CARGO=(cargo); fi
  "${CARGO[@]}" build --locked --quiet --manifest-path "$REPO/gateway/Cargo.toml" || { echo "cargo build failed"; exit 2; }
  TARGET=$("${CARGO[@]}" metadata --no-deps --format-version 1 --locked --manifest-path "$REPO/gateway/Cargo.toml" | node -pe 'JSON.parse(require("fs").readFileSync(0, "utf8")).target_directory')
  HBOTS_BIN=$TARGET/debug/hbots
fi
[ -x "$HBOTS_BIN" ] || { echo "no hbots binary at $HBOTS_BIN"; exit 2; }
echo "rust gateway: $HBOTS_BIN ($("$HBOTS_BIN" version))"

HOME_DIR=$WORK/home
APP=$HOME_DIR/bots/.botapp
rm -rf "$HOME_DIR"
fixtures "$HOME_DIR"

echo "== two hbots processes, one home"
PORT_A=$PORT
# Each process also binds PORT+1 for the Tailscale listener, so the second side skips that port.
PORT_B=$((PORT + 2))
start_at "$PORT_A" "$HOME_DIR"
PORT=$PORT_A
c=$(code GET /api/health "" ""); expect R0 "GET /api/health answers" "$c" 200
c=$(code GET /api/bots "" ""); expect R1 "GET /api/bots without a session is refused" "$c" 401
login "$WORK/rust.jar"
if [ -s "$WORK/rust.jar" ]; then ok R2 "login sets a bot_session cookie"; else bad R2 "login sets a bot_session cookie" "empty"; fi

c=$(code GET /api/bots "$WORK/rust.jar" ""); expect R3 "GET /api/bots answers" "$c" 200
expect R3b "both bot.json files are read, Ada is main" \
  "$(jv "$WORK/last.body" 'j.bots.map(b => b.slug + ":" + b.primary).join(",")')" \
  "b-ada:true,b-grace:false"

c=$(code PUT /api/bots/b-grace "$WORK/rust.jar" '{"label":"Rust label"}'); expect R4 "saving a Bot answers" "$c" 200
expect R4b "bot.json keeps mainThreadId, name and createdAt" \
  "$(jv "$HOME_DIR/bots/b-grace/.bot/bot.json" 'j.label + "|" + j.name + "|" + j.mainThreadId + "|" + j.createdAt')" \
  "Rust label|Grace|thread-grace|2026-01-02T00:00:00.000Z"
info R4 "bot.json keys after the write: $(jv "$HOME_DIR/bots/b-grace/.bot/bot.json" 'Object.keys(j).sort().join(",")')"
info R4 "difference: the fixture had name, label, shape, color, primary, mainThreadId, createdAt. The write added hidden, notifications, pinned, projectId, section and updatedAt (gateway/src/bots.rs load fills the defaults, save writes updatedAt)."

c=$(code PUT /api/main-bot "$WORK/rust.jar" '{"slug":"b-ada"}'); expect R5 "selecting the main Bot answers" "$c" 200
expect R5b "main-bot.json names Ada" "$(jv "$APP/main-bot.json" 'j.slug')" "b-ada"

c=$(code PUT /api/settings "$WORK/rust.jar" '{"userName":"Rust User","beta":{"projects":true}}'); expect R6 "saving settings answers" "$c" 200
expect R6b "settings.json keeps the name and the Projects switch" \
  "$(jv "$APP/settings.json" 'j.userName + "|" + j.beta.projects')" "Rust User|true"
expect R6c "settings.json keeps an empty webhook public base" \
  "$(jv "$APP/settings.json" 'j.webhooks.publicBase')" ""
info R6 "settings.json keys written from a two-field patch: $(jv "$APP/settings.json" 'Object.keys(j).sort().join(",")')"
info R6 "difference: the patch was userName and beta.projects. The file also stores the gateway defaults (gateway/src/main.rs settings)."

c=$(code POST /api/vault "$WORK/rust.jar" '{"type":"login","title":"Rust login","website":"https://rust.example.test","username":"rust-user","password":"rust-pw-ABC"}')
expect R7 "creating a Vault login answers" "$c" 200
VAULT_ID=$(jv "$WORK/last.body" 'j.id')
expect R7b "vault.enc is a bot-vault envelope" \
  "$(jv "$HOME_DIR/vault.enc" 'j.format + " " + j.version + " " + j.cipher')" "bot-vault 1 aes-256-gcm"
expect R7c "vault.enc holds no plaintext password" "$(leaks "$HOME_DIR/vault.enc" rust-pw-ABC)" no
file_mode() {
  # GNU: stat -c %a; BSD/macOS: stat -f %Lp
  if stat -c %a "$1" >/dev/null 2>&1; then stat -c %a "$1"; else stat -f %Lp "$1"; fi
}
expect R7d "the vault key is 32 bytes, mode 600" \
  "$(wc -c < "$HOME_DIR/.vault-key" | tr -d ' ')|$(file_mode "$HOME_DIR/.vault-key")" "32|600"
c=$(code GET "/api/vault/$VAULT_ID?reveal=1" "$WORK/rust.jar" ""); expect R7e "reveal returns the password" \
  "$(jv "$WORK/last.body" 'j.password')" "rust-pw-ABC"
c=$(code GET /api/vault "$WORK/rust.jar" ""); expect R7f "the list does not include the password" \
  "$(jv "$WORK/last.body" 'j.items[0].password === undefined')" true

c=$(code POST /api/projects/items "$WORK/rust.jar" '{"items":[{"title":"Rust item","kind":"task"}]}')
expect R8 "saving a Projects item answers" "$c" 200
ITEM=$(jv "$APP/projects.json" 'Object.keys(j.items)[0]')
expect R8b "projects.json keeps the title" \
  "$(jv "$APP/projects.json" 'j.items[Object.keys(j.items)[0]].title')" "Rust item"
c=$(code POST /api/projects/items "$WORK/rust.jar" "{\"items\":[{\"id\":\"$ITEM\",\"title\":\"Rust edit\"}]}")
expect R8c "a partial save answers" "$c" 200
expect R8d "a partial save keeps createdAt" \
  "$(jv "$APP/projects.json" 'typeof j.items[Object.keys(j.items)[0]].createdAt')" "string"
info R8 "projects.json item keys: $(jv "$APP/projects.json" 'Object.keys(j.items[Object.keys(j.items)[0]]).sort().join(",")')"
info R8 "projects.json top-level keys: $(jv "$APP/projects.json" 'Object.keys(j).sort().join(",")')"
c=$(code GET /api/projects "$WORK/rust.jar" "")
info R8 "difference: the file stores items as an object keyed by id; GET /api/projects returns items as $(jv "$WORK/last.body" 'Array.isArray(j.items) ? "an array" : typeof j.items') (gateway/src/projects.rs board). The file is compact JSON, mode $(file_mode "$APP/projects.json")."

c=$(code GET /api/session "$WORK/rust.jar" ""); expect R9 "side A is signed in" \
  "$(jv "$WORK/last.body" 'j.authenticated')" true
start_at "$PORT_B" "$HOME_DIR"
PORT=$PORT_B
c=$(code GET /api/session "$WORK/rust.jar" ""); expect R10 "side B accepts the bot_session cookie side A issued" \
  "$(jv "$WORK/last.body" 'j.authenticated')" true
login "$WORK/side-b.jar"
if [ -s "$WORK/side-b.jar" ]; then ok R10b "side B sets its own bot_session cookie"; else bad R10b "side B sets its own bot_session cookie" "empty"; fi
PORT=$PORT_A
c=$(code GET /api/session "$WORK/side-b.jar" ""); expect R10c "side A accepts the bot_session cookie side B issued" \
  "$(jv "$WORK/last.body" 'j.authenticated')" true
info R10 "both processes read $APP/session-secret; cookie name is bot_session (value not printed)"
PORT=$PORT_B
c=$(code GET /api/bots "$WORK/side-b.jar" ""); expect R10d "side B reads the bot.json side A wrote" \
  "$(jv "$WORK/last.body" 'j.bots.find(b => b.slug === "b-grace").label')" "Rust label"
c=$(code GET /api/settings "$WORK/side-b.jar" ""); expect R10e "side B reads the settings.json side A wrote" \
  "$(jv "$WORK/last.body" 'j.userName + "|" + j.beta.projects')" "Rust User|true"
c=$(code GET /api/vault "$WORK/side-b.jar" ""); expect R10f "side B reads the vault side A wrote, still redacted" \
  "$(jv "$WORK/last.body" 'j.items.length + "|" + (j.items[0].password === undefined)')" "1|true"
PORT=$PORT_A

# A process that already loaded the board keeps that copy. Break the file underneath it.
printf '%s' '{"items":' > "$APP/projects.json"
c=$(code POST /api/projects/items "$WORK/rust.jar" '{"items":[{"title":"from the live process","kind":"task"}]}')
if [ "$c" = 200 ]; then
  info R11 "difference: a process that already read projects.json overwrites a broken file from memory (gateway/src/projects.rs load, HTTP $c)"
else
  info R11 "a live process refused to save over a broken projects.json (HTTP $c)"
fi
stop
printf '%s' '{"items":' > "$APP/projects.json"
start_at "$PORT_A" "$HOME_DIR"
PORT=$PORT_A
c=$(code GET /api/projects "$WORK/rust.jar" ""); info R11 "a fresh process reading a broken projects.json: HTTP $c, items=$(jv "$WORK/last.body" 'Array.isArray(j.items) ? j.items.length : "n/a"') (gateway/src/projects.rs board)"
c=$(code POST /api/projects/items "$WORK/rust.jar" '{"items":[{"title":"should not land","kind":"task"}]}')
left=$(head -c 9 "$APP/projects.json")
if [ "$c" != 200 ] && [ "$left" = '{"items":' ]; then
  ok R11 "a fresh process does not overwrite a broken projects.json"
else
  bad R11 "a fresh process does not overwrite a broken projects.json" "HTTP $c, file starts with $left"
fi
stop

D4=$WORK/created
rm -rf "$D4"
mkdir -p "$D4/bots/b-linus/.bot"
cp "$HERE/fixtures/bot-linus.json" "$D4/bots/b-linus/.bot/bot.json"
start_at "$PORT_A" "$D4"
PORT=$PORT_A
login "$WORK/linus.jar"
code GET /api/bots/b-linus "$WORK/linus.jar" "" >/dev/null
a=$(jv "$WORK/last.body" 'j.createdAt')
sleep 1.1
code GET /api/bots/b-linus "$WORK/linus.jar" "" >/dev/null
b=$(jv "$WORK/last.body" 'j.createdAt')
on_disk=$(jv "$D4/bots/b-linus/.bot/bot.json" 'j.createdAt')
primary=$(jv "$D4/bots/b-linus/.bot/bot.json" 'j.primary')
info R12 "two reads of a bot.json with no createdAt returned $a then $b"
if [ "$a" = "$b" ] && [ "$a" = "$on_disk" ] && [ "$a" != "<unreadable>" ]; then
  ok R12 "the first read writes createdAt into bot.json and the second read returns that same value"
else
  bad R12 "the first read writes createdAt into bot.json and the second read returns that same value" "$a -> $b, disk $on_disk"
fi
info R12 "difference: the fixture omitted createdAt and set primary false. After the first read the file has createdAt $on_disk and primary $primary, plus the load defaults. gateway/src/bots.rs load stamps createdAt; reconcile persists the Bot when primary changes."
stop

echo "== summary: $PASSED passed, $FAILED failed (work dir $WORK)"
[ "$FAILED" -eq 0 ]
