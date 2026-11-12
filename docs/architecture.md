# Architecture

Two programs. They do not share a process.

- `hbots` (`gateway/`) is the server. It keeps Bot folders, the Vault, settings, sessions, and the Bots' tool server. Each Bot's library (files, records, and connectors) is read and written through the workspace adapter. The old Projects board stays off. A VPS runs `hbots serve` alone. A desktop app can start that process as a child. A phone or a browser is the client only and uses `HBOTS_URL`. The client never scans the disk.
- `app/` is the Exact client. The same source is the screen on the web, desktop, and mobile.

The gateway speaks to a harness backend. T3 Code is the one implemented today (`gateway/src/harness/adapters/t3.rs`, `T3_URL`, `T3_TOKEN`). Another backend would be another module beside that one. This note does not add that module.

## Agent context

hbots injects a Bot's prompt and a memory snapshot into the managed block of that Bot's `AGENTS.md`. The prompt is `## Instructions` for a main Bot. The edit page and the create wizard save that prompt for a main Bot. A subagent keeps its own prompt on the subagent page. The gateway rejects a prompt over 16 KiB, and the client shows that error. The snapshot follows the Memory bullets and is read by `gateway/src/agent_context.rs`, because `memory.rs` does not exist yet. A poller rewrites the block when those files change. Threads that are not a Bot's are not touched. An ephemeral child thread inherits nothing: it does not see this file unless it starts in the Bot's folder. There is no separate per-thread instructions field on the harness calls hbots already makes.

Each Bot's thread is created on that local T3. Choosing a linked computer, from Settings and from the Bot's own config, is specified in [the execution environment design](bot-execution-environment.md). That design keeps this one broker and selects among links T3 already has. The model list follows the computer the Bot is set to. None of that is built yet.

The client holds an hbots session. The harness token stays on the gateway.

Settings can record the Bots folder in `~/.hbots/storage.json`. The file system stays the default, and the running gateway keeps using it.
