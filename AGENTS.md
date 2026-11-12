# Agent notes (hbots)

## Harness model

Bots use **Claude Haiku** (`claude-haiku-5-5` via `claudeAgent`). Do not pick Opus or Sonnet for bot threads or hbots-related T3 delegates unless the user asks.

Default is set in `gateway/src/bots/provisioner/mod.rs` (`default_model`) and live settings `defaultModel`.

Where a Bot runs is specified in [docs/bot-execution-environment.md](docs/bot-execution-environment.md). Keep the local T3 as the broker and select among links it already has. Settings sets the default computer. A Bot's details page, and a durable child's edit sheet, show the current computer and can change the setting. The model list follows that computer. Do not send `environmentId` on an RPC this T3 build does not accept. Do not change `default_model()` unless the user asks.

## Repository skills

- [dev-watch](.agents/skills/dev-watch/SKILL.md): start or troubleshoot the local development watch environment.
- [release](.agents/skills/release/SKILL.md): prepare changelogs, version bumps and releases.
- [exact-client](.agents/skills/exact-client/SKILL.md): implement and verify Exact client and localized UI changes.
- [t3-integration](.agents/skills/t3-integration/SKILL.md): work on the T3 harness protocol and gateway integration.
- [native-builds](.agents/skills/native-builds/SKILL.md): build and troubleshoot native Exact clients.
- [compatibility](.agents/skills/compatibility/SKILL.md): assess persisted-data and shared-home compatibility.

## Stack

For setup, the watch loop, LAN access, and troubleshooting, see [DEV.md](DEV.md).

| Piece | What | Default |
|---|---|---|
| T3 Code | Harness (desktop app) | http://127.0.0.1:3773 |
| Exact web build | Client → `app/target/web-dist` | needs `.exact-path` |
| hbots gateway | Rust binary, serves app + API | http://127.0.0.1:3780 |

### Start (dev checkout)

1. **T3 Code** — open the desktop app once; it writes `~/.t3/bin/t3` (Electron shim into the app asar). There is no bare `t3` binary under `Contents/Resources` on current Nightly.
2. **Web app** (once, or after Contract changes):

```sh
set -a; source .exact-path; set +a
EXACT_APP_DIR=$PWD/app bun "$EXACT_DIR/host/web/build.mjs" bots-web
```

3. **Gateway** — first time: `PATH="$HOME/.t3/bin:$PATH" ./target/release/hbots up --no-service`  
   Then: `./target/release/hbots serve`  
   (`cargo build --release` if the binary is missing.)
4. **macOS app** — `scripts/run-macos.sh` starts the gateway if needed and opens the macOS app (see `docs/native.md`).

Open http://127.0.0.1:3780. Settings: `~/.hbots/.env` + `~/.hbots/t3-token`.

### Watch

From the repo root, with T3 Code already running:

```sh
bun scripts/dev.mjs
```

For access from another device, run `bun scripts/dev.mjs --lan`. A proxy on 8765 points each
browser at the gateway on the host it used, so LAN, Tailscale and `<hostname>.local` addresses all
work from one run (Exact moves to 8766). LAN mode accepts browser requests from any origin in the debug gateway;
use it only on a trusted network. Release builds do not enable this behavior.

That starts both and rebuilds each when its own code changes.

- **Gateway.** Debug build at http://127.0.0.1:3780. A change under `gateway/` or `design/`, or in the workspace `Cargo.toml`, rebuilds `target/debug/hbots` and restarts `serve`. An `hbots` process already listening on 3780 is stopped once that binary is ready.
- **Frontend.** Exact's JS dev loop at http://127.0.0.1:8765. A change to the Contract, `app.ts`, strings, or assets rebuilds the page and reloads it. The loop sets `HBOTS_URL` to the gateway, so the page calls port 3780. Files land in `app/target/web-dev` (gitignored), separate from `app/target/web-dist`.

Ctrl-C stops both. T3 Code on 3773 is left running. The script reads `EXACT_DIR` from the environment or from `.exact-path`. The one-shot release start above is still the path for a packed binary. While this watch is running, open http://127.0.0.1:8765.

Env prefix is `HBOTS_*` (`HBOTS_HOME`, `HBOTS_PASSWORD`, `HBOTS_NETWORK_ACCESS`, `HBOTS_TAILSCALE_PATH`, `HBOTS_CLOUDFLARED_PATH`, `HBOTS_TAILNET_PORT`, `HBOTS_TAILNET_USERS`, `HBOTS_PUBLIC_LANDING_ONLY`, …). Legacy unprefixed Tailscale/tunnel names still work. `HBOTS_NETWORK_ACCESS` is `local`, `lan`, or `tailscale`. When it is set, it overrides Settings → Access → Network and locks that control. See [DEV.md](DEV.md).

**A6:** `busy_threads` reads one `orchestration.subscribeShell` snapshot, counts threads a restart would interrupt, and interrupts the subscription (`gateway/src/release.rs`, `gateway/src/t3.rs`). Routine list does the same for `scheduledTasks.subscribe`. Auto-update still waits when that read fails, including a `snapshot-required` reply.

Release: GitHub repo variable `EXACT_REF` is `956ae33a35ffa5313d7be3cebc55ed84f9480e0a` (the workflow still falls back to `main` if the variable is unset). Pack locally with `HBOTS_VERSION=… sh scripts/pack-release.sh`; install with `install.sh --version … --from <tarball> --sums SHA256SUMS`.

macOS native client: `EXACT_APP_DIR=$PWD/app bun "$EXACT_DIR/host/apple/build.mjs"` → `app/target/clients/…/macos/hbots.app` (gitignored under `app/target`).

## Exact checkout

Exact is not in this repo. Path is in **`.exact-path`** (gitignored):

- `EXACT_DIR` — absolute
- `EXACT_DIR_REL` — relative to repo root (`../../contrib/exact`)

Contract colour docs: `$EXACT_DIR/docs/contract-for-agents.md`.

## Gateway layout

`gateway/src` is split by domain. Each domain folder holds its logic, one file per concern, and backend-specific code in dedicated files. Implementations of a backend live under that domain's `adapters/` folder.

- `main.rs`: entry point only. `api/`: HTTP handlers and the router, one file per area.
- `storage/`: `workspace.rs` is the trait the rest of the gateway uses. `settings/` is the saved choice, with `filesystem.rs` and `anytype.rs` for each backend. `adapters/` has `filesystem/`, `anytype/` and `memory`.
- `harness/`: the agent runtime. `adapters/t3.rs` is the T3 Code client; `providers` lists its models.
- `bots/`, `chat/`, `projects/`, `vault/`, `network/`, `platform/`: the other domains. T3-specific code inside them sits in files named `t3.rs` or `browser.rs`.

**File length rule:** a `.rs` file is at most 500 lines, tests included. Move tests to a sibling `tests.rs` (or `tests/` with one file per topic), and split a large module into a folder with one file per concern. `scripts/check-rs-length.sh` enforces it in CI. Do not raise the limit or add an allowlist; split the file.

## Chrome strings

On-screen wording lives in `app/strings/en.json` and `app/strings/fr.json`. The host locale picks the table: `fr` uses French, anything else uses English. New chrome (labels, buttons, placeholders, empty states, aria-labels, titles, and errors the client shows) goes through those tables. Do not add a new hardcoded English sentence in the contract. The names hbots and T3 Code stay as written. Bot messages, names a person typed, Vault secrets, and gateway text the client does not remap stay as stored.
