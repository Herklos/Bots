# Architecture

Two programs. They do not share a process.

- `hbots` (`gateway/`) is the server. It keeps Bot folders, the Vault, settings, sessions, and the Bots' tool server. Each Bot's library (files, records, and connectors) is read and written through the workspace adapter. The old Projects board stays off. A VPS runs `hbots serve` alone. A desktop app can start that process as a child. A phone or a browser is the client only and uses `HBOTS_URL`. The client never scans the disk.
- `app/` is the Exact client. The same source is the screen on the web, desktop, and mobile.

The gateway talks to the agent runtime through the `Harness` trait in `gateway/src/harness/mod.rs`. `AppState.harness` is `Arc<dyn Harness>`. Startup builds that value from `T3_URL` and `T3_TOKEN`. T3 Code is the runtime Michel runs. `MockHarness` is for tests. A second production runtime is not built.

`Harness` only groups capabilities. Each one is its own trait: transcript, turns, previews, routines, approvals, provisioning, browser, and relay (the `/t3` proxy). Domain code calls those traits. It does not name T3 RPCs. `gateway/src/harness/adapters/t3/` is the only place that does. `scripts/check-t3-boundary.sh` fails when one of those names appears anywhere else under `gateway/`. The folder map is [gateway layout](gateway-layout.md).

A runtime that lacks a capability returns `HarnessError::Unsupported`, which the API reports as 501. A connection that cannot be opened, or a call the runtime rejected, is reported as 502 with the message the gateway already showed. Neutral values (`ThreadSnapshot`, `ShellSnapshot`, `UserMessage`, `Frame`, `BrowserTab`, and the provision and approval values) live in `gateway/src/harness/snapshot.rs`. The runtime's own projection stays in the adapter.

## Agent context

Michel injects a Bot's prompt and a memory snapshot into the managed block of that Bot's `AGENTS.md`. The prompt is `## Instructions` for a main Bot. The edit page and the create wizard save that prompt for a main Bot. A subagent keeps its own prompt on the subagent page. The gateway rejects a prompt over 16 KiB, and the client shows that error. The snapshot follows the Memory bullets and is read by `gateway/src/agent_context.rs`, because `memory.rs` does not exist yet. A poller rewrites the block when those files change. Threads that are not a Bot's are not touched. An ephemeral child thread inherits nothing: it does not see this file unless it starts in the Bot's folder. There is no separate per-thread instructions field on the harness calls Michel already makes.

Each Bot's thread is created on that local T3. Choosing a linked computer, from Settings and from the Bot's own config, is specified in [the execution environment design](bot-execution-environment.md). That design keeps this one broker and selects among links T3 already has. The model list follows the computer the Bot is set to. None of that is built yet.

The client holds a Michel session. The harness token stays on the gateway.

Settings can record the Bots folder in `~/.hbots/storage.json`. The file system stays the default, and the running gateway keeps using it.
