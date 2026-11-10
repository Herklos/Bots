# hbots

hbots is one Rust binary and an Exact app. This is the messaging app and the small server
in front of a harness. T3 Code is the harness wired today. The gateway can grow other
backends later. See [docs/vision.md](docs/vision.md) and [docs/architecture.md](docs/architecture.md).

The name hbots is the product, the crate, the binary, and the configuration prefix (`HBOTS_HOME`,
`HBOTS_PASSWORD`, `HBOTS_ROOT`, `HBOTS_MCP_URL`, `HBOTS_MCP_TOKEN`, `HBOTS_URL`).

| Piece | What it is |
|---|---|
| `gateway/` → the `hbots` binary | Rust (axum). Sign-in, the harness proxy (T3 Code today), Bot folders, the encrypted Vault, the Projects board, the Bots' MCP tool server, and the web app's static files. |
| `app/` | The client, written once in [Exact](https://github.com/expo/exact)'s Contract: web today; macOS, iOS and Linux are the same source (not built yet, see below). |
| `tests/` | A mock T3 Code and an end-to-end browser test. |

On-disk layout is `~/.hbots`: `bots/<slug>/.bot/bot.json`, `vault.enc` + `.vault-key`,
`bots/.botapp/projects.json`. A `bot-vault` envelope decrypts in Rust
(checked by `gateway/tests/fixtures/node-vault.json`).

## Does it still need a server?

**Yes. One small process, on the machine that runs the harness. T3 Code is that harness today. Another backend can be added later, beside it.**
The client can't do these jobs by itself:

1. **The harness** is the agent runtime and your provider logins. It runs on a machine, not in a phone or browser. T3 Code is the one this gateway speaks to now.
2. **The harness token stays off the client.** T3 Code, the backend wired today, authenticates with a bearer token that grants full control of your agents. Browsers can't set `Authorization` on a WebSocket, so T3 offers a ticket flow, but the long-lived token still has to be somewhere. In hbots it lives only in the gateway; a client that held it would hand every device (and every XSS) a 30-day key. The gateway holds it and gives clients a password-gated session instead.
3. **Bots live on the machine's disk.** A Bot is a folder (`AGENTS.md`, `memory/`, `.mcp.json`) that the agent process reads. A client can't write there.
4. **The Bots' tools are an HTTP server.** Agents call `POST /mcp/bot` (Vault, Projects) from that machine. Something must listen.
5. **Work that happens with no client open:** giving a new Bot its project and thread on the harness (T3 Code today), delivering its brief.

What the client does alone: everything on screen, drafts, theme, and notifications. One binary also serves the web app. A desktop build could start the gateway as a
child process so it feels like one app (not built yet).

## Run it

```sh
t3 auth session issue --ttl 30d --label hbots --token-only      # paste into T3_TOKEN
T3_TOKEN=… cargo run --release                                  # http://127.0.0.1:3780
```

`t3: command not found`? The T3 Code desktop app doesn't always put its command line on your `PATH`.
Try, in this order (none of these are verified against every T3 Code version):

1. `npx t3 auth session issue --ttl 30d --label hbots --token-only`, if Node is installed.
2. Look for a `t3` the app carries: `mdfind -name t3 | grep -i t3`, or open `/Applications/T3 Code.app/Contents/Resources`.
   Run it by its full path, or link it onto your `PATH`.
3. Install the command line from the T3 Code README, then run the command above.

`hbots up` does the same lookup (your `PATH`, then `~/.t3/bin/t3`) and, when it finds nothing, tells you to
set `T3_TOKEN` yourself.

Settings: `T3_URL`, `T3_TOKEN`, `HOST`, `PORT`, `HBOTS_PASSWORD`, `HBOTS_HOME`,
`HBOTS_ROOT`, `HBOTS_MCP_URL`, `HBOTS_MCP_TOKEN`, `ALLOWED_ORIGINS`, `STATIC_DIR` (where the built web app is).
Without a password it listens on localhost only and signs nobody in; set `HBOTS_PASSWORD` before exposing it.

Build the web app (Exact checkout needed, see its README for `bun scripts/exact.mjs setup`):

```sh
EXACT_APP_DIR=$PWD/app bun <exact>/host/web/build.mjs hbots-web      # → app/target/web-dist
STATIC_DIR=$PWD/app/target/web-dist cargo run --release
```

Native apps have no page address, so build them with `HBOTS_URL=https://your-gateway`. The web build is made with a
placeholder address and the gateway rewrites it to the address each page was loaded from, because a Contract
data module can't read `location`.

## Tests

```sh
cargo test                  # 15 unit tests (auth, Vault, Bots, Projects, T3 client, MCP)
tests/e2e.sh                # mock T3 + gateway + Chromium: create a Bot, chat, see the reply
```

## Status

Ported and tested: sign-in and lockout, same-origin checks, Bot folders and the main-Bot rule, AGENTS.md and
MCP config for Claude and Codex, the Vault (encrypted file, list/create/update/delete), the Projects board (API,
live stream, MCP tools), a T3 RPC client (the harness wired today; other backends may be added), the `/t3/*` proxy, provisioning, chat read/send/stop, and an Exact app
with the inbox, chat, New Bot and sign-in. **Not ported yet** is listed with owners in [docs/PLAN.md](docs/PLAN.md):
the Vault's browser-fill tools, remote access (Tailscale, Cloudflare), Settings, routines, answering approvals,
the avatar system, installer and updater, native builds.
