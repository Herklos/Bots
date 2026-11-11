<div align="center">

# hbots

**Your personal AI Bots, together in one messaging app.**

Run Bots on your own machine, give each one a workspace, and talk to them from the web.

[Get started](#quick-start) · [Features](#features) · [How it works](#how-it-works) · [Development](DEV.md) · [Changelog](CHANGELOG.md)

</div>

## What is hbots?

hbots connects a messaging interface to [T3 Code](https://github.com/pingdotgg/t3code), the agent harness that runs your Bots. Each Bot has its own name, instructions, workspace and conversation. The gateway runs on your machine, keeps the T3 token away from clients, and serves the web app.

The client is built with [Exact](https://github.com/expo/exact). Release archives currently include the gateway and web app for macOS and Linux; native clients are still in development.

## Quick start

Start the T3 Code desktop app, then install the latest release on your Mac or Linux machine:

```sh
curl -fsSL https://raw.githubusercontent.com/Herklos/Bots/main/install.sh | sh
~/.hbots/runtime/current/hbots up
```

The installer verifies the archive against `SHA256SUMS`. `hbots up` configures the gateway and T3 token, installs a background service, and prints the address. Open **http://127.0.0.1:3780** on the same machine.

For a terminal-only setup, run `hbots up --no-service`, then `hbots serve` using the installed binary. To install a specific release, download `install.sh` and run `sh install.sh --version VERSION`.

**Before enabling remote access**, set a password in Settings or with `HBOTS_PASSWORD`. See [DEV.md](DEV.md) for local development and troubleshooting.

## Features

- **Personal Bots:** create and customize Bots, choose models and approval modes, and give each Bot its own instructions and workspace. Claude Haiku is the default model.
- **Conversations:** chat with individual Bots or groups, mention Bots, attach files and voice clips, reply, react, and pin messages. Stop a run or answer an approval request from the app.
- **Bot details:** see routines, usage, child threads, and a library of files, pages and links. Manage per-Bot MCP connectors and watch or interact with a Bot's browser through T3 Code.
- **Routines:** schedule work through the harness and review or run a Bot's routines.
- **Vault:** keep logins encrypted, import or export CSV, and let Bot tools use saved credentials without exposing the password to the Bot.
- **Remote access:** set a password, then choose this device, the local network, or Tailscale in Settings.
- **Installation and updates:** install checksummed releases, run a background service, update, and roll back.

See [the project plan](docs/PLAN.md) for ongoing work and limitations.

## How it works

```text
 Browser / Exact client                     Machine running hbots
┌──────────────────────────┐       ┌────────────────────────────────────┐
│ Chats · groups · Vault   │       │ hbots gateway                      │
│ Bot details · Settings   │◄─────►│ Web app · API · sessions · Vault   │
└──────────────────────────┘       │ Bot tools (MCP) · T3 token         │
           hbots session           └──────────────┬───────────┬─────────┘
                                                  │           │
                               workspace interface│           │harness connection
                                                  ▼           ▼
                                    ┌─────────────────┐  ┌──────────────────┐
                                    │ Workspace store │  │ T3 Code harness  │
                                    │ Local disk today│  │ Threads · runs   │
                                    └─────────────────┘  │ Provider logins  │
                                                         └──────────────────┘
```

The **gateway** serves the client and API, manages sessions and the Vault, and connects to the harness. The **harness** runs Bot threads and holds provider logins. T3 Code is the harness currently wired into hbots.

The **workspace** is a separate storage concern. The `Workspace` interface backs Bot library records, files and MCP connectors; the running gateway currently uses its local-disk adapter. Other parts of the gateway still access local files directly, and the in-memory adapter is for tests. Additional storage and harness backends are planned, not selectable production options today. See [architecture](docs/architecture.md) and the [backend plan](docs/backends.md) for details.

By default, Bot folders live under `~/.hbots/bots/`. The gateway holds the T3 token; clients use an hbots session instead. For access beyond localhost, set a password, then choose local network or Tailscale in Settings → Access → Network. Restart hbots to apply a listener change.

## Commands

```text
hbots up [--port N] [--t3-url URL] [--no-service]   Set up and start hbots
hbots serve                                          Run the gateway in the foreground
hbots status [--json]                                Show service and gateway status
hbots update [version] [--check] [--preview] [--yes] Check or install a release
hbots rollback                                       Restore the previous version
hbots service install|uninstall|restart|status       Manage the background service
```

Configuration comes from the environment or `~/.hbots/.env`. Common settings include `T3_URL`, `T3_TOKEN` or `T3_TOKEN_FILE`, `HOST`, `PORT`, `HBOTS_PASSWORD`, `HBOTS_NETWORK_ACCESS`, and `HBOTS_HOME`.

`HBOTS_NETWORK_ACCESS` is `local`, `lan`, or `tailscale`. When it is set, it overrides Settings → Access → Network and the control is locked. `local` listens on this device. `lan` listens on every IPv4 interface over HTTP and requires a password. `tailscale` uses the existing Tailscale Serve HTTPS route and also requires a password. `HOST=127.0.0.1`, which `hbots up` writes, does not lock the control. Any other `HOST` is a legacy bind address and does. A saved choice is used the next time the gateway starts.

## Development and status

See [DEV.md](DEV.md) for prerequisites, the watch loop, LAN access, tests and troubleshooting. The [native builds guide](docs/native.md) describes the current macOS, iOS and Linux client status; native phone builds and distribution are not yet release artifacts.

The project is early. The published release workflow builds the gateway and web assets for macOS and Linux. Other harnesses, storage backends and native client distribution need further work.

| Path | Contents |
|---|---|
| [`gateway/`](gateway/) | Rust gateway and CLI |
| [`app/`](app/) | Exact client and localized strings |
| [`tests/`](tests/) | Integration and compatibility tests |
| [`docs/`](docs/) | Architecture, plans and native build notes |
| [`design/`](design/) | Brand and design guidelines |

## Credits

Inspired by [Threebot](https://github.com/RokaSwK/threebot), built with [Exact](https://github.com/expo/exact), and powered by [T3 Code](https://github.com/pingdotgg/t3code). See [NOTICE](NOTICE) and [LICENSE](LICENSE).
