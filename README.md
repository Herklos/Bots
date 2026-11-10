<div align="center">

# hbots

**Personal AI Bots in a messaging app. One Rust binary. One client for web, desktop and phone.**

Each Bot is a folder on your machine with its own memory, tools and thread.
You talk to them from anywhere. Your keys never leave home.

[Quick start](#quick-start) · [How it works](#how-it-works) · [Backends](#backends) · [Status](#status) · [Docs](docs/architecture.md)

</div>

---

## Why

- **Bots, not chats.** A Bot has a name, a face, a folder, a memory and a thread. It keeps working when you close the app.
- **Your machine, your files.** Bots are plain folders (`AGENTS.md`, `memory/`, `.mcp.json`) under `~/.hbots`. Read them, edit them, back them up with `cp`.
- **A Vault the Bots can use, not read.** Logins live in an AES-256-GCM encrypted file. Bots see an entry without its password; tools use the secret on their behalf.
- **One small server.** `hbots` is a single binary. It signs you in, guards the harness token, serves the web app and exposes the Bots' tool server. No Node, no database.
- **One client, written once.** The app is a single [Exact](https://github.com/expo/exact) source for web, macOS, iOS and Linux.
- **Built to reach you safely.** Password sign-in with lockout, Tailscale, Cloudflare Tunnel and Cloudflare Access are built in.

## Quick start

You need [Rust](https://rustup.rs) and a running harness. [T3 Code](https://github.com/pingdotgg/t3code) is the one wired today.

```sh
git clone https://github.com/Herklos/Bots && cd Bots
cargo build --release
./target/release/hbots up
```

`hbots up` writes `~/.hbots/.env`, asks T3 Code for a token, installs a background service (launchd on macOS, systemd on Linux) and prints the address:

```
hbots 0.1.0 is running: http://127.0.0.1:3780
```

Prefer a terminal-only run? `hbots up --no-service`, then `hbots serve`.

<details>
<summary><b><code>t3: command not found</code></b></summary>

The T3 Code desktop app doesn't always put its command line on your `PATH`. Try, in order (none of these are verified against every T3 Code version):

1. `npx t3 auth session issue --ttl 365d --label hbots --token-only`, if Node is installed. Save the output in `~/.hbots/t3-token`.
2. Look for a `t3` the app carries: `mdfind -name t3 | grep -i t3`, or `/Applications/T3 Code.app/Contents/Resources`. Run it by full path.
3. Install the command line from the T3 Code README.

`hbots up` looks on your `PATH`, then in `~/.t3/bin/t3`. If it finds nothing, it tells you to set `T3_TOKEN` yourself.
</details>

### The web app

The gateway serves the built client. Build it once (needs an [Exact](https://github.com/expo/exact) checkout; local path is in gitignored `.exact-path`, see `AGENTS.md`):

```sh
set -a; source .exact-path; set +a
EXACT_APP_DIR=$PWD/app bun "$EXACT_DIR/host/web/build.mjs" bots-web   # → app/target/web-dist
STATIC_DIR=$PWD/app/target/web-dist hbots serve
```

Release archives carry a `web/` folder next to the binary, and the gateway serves it with no setting.
Native apps have no page address, so build them with `HBOTS_URL=https://your-gateway`.

To install the latest published stable release:

```sh
curl -fsSL https://raw.githubusercontent.com/Herklos/Bots/main/install.sh | sh
~/.hbots/runtime/current/hbots up
```

The installer selects the archive for your platform, verifies it against `SHA256SUMS`, and
installs the binary and web files together. T3 Code must be running separately. To install a
specific release, download the script and run `sh install.sh --version VERSION`; add
`HBOTS_RELEASE_BASE=<url>` to use a mirror.

### Before you expose it

Without a password the gateway listens on localhost and signs nobody in. Set `HBOTS_PASSWORD` (or a password in Settings) before using Tailscale or a tunnel.

## How it works

```
 phone · browser · desktop           your machine
┌───────────────────────┐        ┌────────────────────────────────────┐
│  Exact client (app/)  │  HTTPS │  hbots gateway (gateway/)          │      ┌───────────────┐
│  inbox · chat · Vault │◄──────►│  sessions · Vault · Projects · MCP │◄────►│ harness       │
│  Projects · Settings  │ cookie │  Bot folders · remote access       │ token│ (T3 Code now) │
└───────────────────────┘ /bearer└────────────────────────────────────┘      └───────────────┘
                                          │
                                          ▼
                                   ~/.hbots/bots/<slug>/
```

**Does it need a server?** Yes, one small process on the machine that runs the harness. The client can't do these by itself:

1. **The harness is the agent runtime** and holds your provider logins. It runs on a machine, not in a phone or a browser.
2. **The harness token stays off the client.** It grants full control of your agents. Browsers can't set `Authorization` on a WebSocket, and a client that held the token would hand every device (and every XSS) a long-lived key. The gateway holds it and gives clients a password-gated session.
3. **Bots live on disk.** A Bot is a folder the agent process reads. A client can't write there.
4. **The Bots' tools are an HTTP server.** Agents call `POST /mcp/bot` (Vault, Projects) from that machine.
5. **Some work has no client open:** giving a new Bot its project and thread, delivering its brief.

The client does everything on screen, drafts, theme and notifications. A desktop build can start the gateway as a child process so it feels like one app (not built yet).

## Backends

The gateway talks to a **harness**: the process that actually runs the agent. The client never sees the harness, only an hbots session.

| Harness | State |
|---|---|
| [T3 Code](https://github.com/pingdotgg/t3code) | Wired today: RPC client, `/t3/*` proxy, provisioning, chat, routines. See `gateway/src/t3.rs`. |
| Others | Designed for, not written. A backend is a module beside `t3.rs`. See [docs/architecture.md](docs/architecture.md). |

Storage is a second seam, separate from the harness: where a Bot's folder (`AGENTS.md`, `memory/`, `.mcp.json`) lives.

| Storage | State |
|---|---|
| Local disk (`~/.hbots/bots/<slug>/`) | Wired today. The harness reads the same folder as its workspace. |
| Anything else (another machine, a container) | Planned. It needs a `Workspace` trait, and a remote harness needs a reachable `HBOTS_MCP_URL`. |

The plan, in seven phases, is in [docs/backends.md](docs/backends.md). The brand and design guidelines are in [design/](design/README.md).

## Features

| | |
|---|---|
| **Bots** | Folder per Bot, a main Bot, instructions for Claude and Codex (`AGENTS.md`, `.mcp.json`) |
| **Chat** | Read, send and stop; reactions, voice-call records, roster previews, file attachments |
| **Vault** | Encrypted logins, CSV import and export, used by tools without the Bot reading the password |
| **Projects** | A board with a live stream, also reachable by the Bots as MCP tools |
| **Routines** | Scheduled tasks on the harness |
| **Approvals** | Answer a Bot's request from the app |
| **Remote access** | Password, Tailscale (own listener, landing-only mode), Cloudflare quick tunnel, Cloudflare Access |
| **Install and update** | `hbots up`, checksummed releases, `hbots update`, `hbots rollback` |

## Commands

```
hbots up [--port N] [--t3-url URL] [--no-service]   set up and start in the background
hbots serve                                          run the gateway in this terminal
hbots status [--json]                                version, service and gateway
hbots update [version] [--check] [--preview] [--yes] install a release and restart
hbots rollback                                       go back to the previous version
hbots service install|uninstall|restart|status       manage the background service
```

Settings come from the environment or `~/.hbots/.env`: `T3_URL`, `T3_TOKEN` (or `T3_TOKEN_FILE`), `HOST`, `PORT`,
`HBOTS_PASSWORD`, `HBOTS_HOME`, `HBOTS_ROOT`, `HBOTS_MCP_URL`, `HBOTS_MCP_TOKEN`, `ALLOWED_ORIGINS`, `STATIC_DIR`.

## Repository

| Path | What |
|---|---|
| `gateway/` | The `hbots` binary (Rust, axum) |
| `app/` | The Exact client: one source for web, macOS, iOS and Linux |
| `tests/` | A mock T3 Code, an end-to-end browser test and a shared-home compatibility harness |
| `docs/` | [Vision](docs/vision.md), [architecture](docs/architecture.md), [native builds](docs/native.md), [plan](docs/PLAN.md) |

```sh
cargo test          # unit tests
tests/e2e.sh        # mock T3 + gateway + Chromium: create a Bot, chat, see the reply
```

## Status

**Early.** Gateway unit tests and the shared-home compat harness pass. Verified on a Mac checkout with a real T3 Code + Exact web build + Settings/Vault UI.

On a Bot: a model picker (Haiku by default), routines, a thread-family list, a library (files, pages, links, and per-Bot projects, notes, tasks, and plans), and a watch of one browser tab. Settings counts messages, attachments, vault writes, and icon fetches on this machine. Connectors are http(s) MCP servers for that Bot. The old Projects board stays off, and connected apps stay postponed.

Still open:

- macOS, iOS and Linux native Exact hosts (tools ready; full apple build not finished here)
- the release workflow on GitHub Actions
- GitHub Actions release has not run. `EXACT_REF` is pinned to `956ae33a35ffa5313d7be3cebc55ed84f9480e0a`
- mouse and typed text while taking control of a page
- connected apps, voice, and the full avatar system

The rest is tracked in [docs/PLAN.md](docs/PLAN.md).

## Credits

The idea came from [Threebot](https://github.com/RokaSwK/threebot). The client is built with [Exact](https://github.com/expo/exact). The harness wired today is [T3 Code](https://github.com/pingdotgg/t3code). See [NOTICE](NOTICE) and [LICENSE](LICENSE).
