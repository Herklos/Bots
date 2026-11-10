# Agent notes (hbots)

## Harness model

Bots use **Claude Haiku** (`claude-haiku-5-5` via `claudeAgent`). Do not pick Opus or Sonnet for bot threads or hbots-related T3 delegates unless the user asks.

Default is set in `gateway/src/provisioner.rs` (`default_model`) and live settings `defaultModel`.

## Stack

| Piece | What | Default |
|---|---|---|
| T3 Code | Harness (desktop app) | http://127.0.0.1:3773 |
| Exact web build | Client → `app/target/web-dist` | needs `.exact-path` |
| hbots gateway | Rust binary, serves app + API | http://127.0.0.1:3780 |

### Start (dev checkout)

1. **T3 Code** — open the desktop app (CLI also at `~/.t3/bin/t3`).
2. **Web app** (once, or after Contract changes):

```sh
set -a; source .exact-path; set +a
EXACT_APP_DIR=$PWD/app bun "$EXACT_DIR/host/web/build.mjs" bots-web
```

3. **Gateway** — first time: `PATH="$HOME/.t3/bin:$PATH" ./target/release/hbots up --no-service`  
   Then: `./target/release/hbots serve`  
   (`cargo build --release` if the binary is missing.)

Open http://127.0.0.1:3780. Settings: `~/.hbots/.env` + `~/.hbots/t3-token`.

Env prefix is `HBOTS_*` (`HBOTS_HOME`, `HBOTS_PASSWORD`, `HBOTS_TAILSCALE_PATH`, …). Legacy `TAILSCALE_PATH` still works.

Release: set GitHub repo variable `EXACT_REF` to a pinned Exact commit/tag (workflow defaults to `main`). Pack locally with `HBOTS_VERSION=… sh scripts/pack-release.sh`; install with `install.sh --version … --from <tarball> --sums SHA256SUMS`.

## Exact checkout

Exact is not in this repo. Path is in **`.exact-path`** (gitignored):

- `EXACT_DIR` — absolute
- `EXACT_DIR_REL` — relative to repo root (`../../contrib/exact`)

Contract colour docs: `$EXACT_DIR/docs/contract-for-agents.md`.
