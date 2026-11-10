# Architecture

Two programs. They do not share a process.

- `hbots` (`gateway/`) is the server. It keeps Bot folders, the Vault, settings, sessions, and the Bots' tool server. Each Bot's library (files, records, and connectors) is read and written through the workspace adapter. The old Projects board stays off. A VPS runs `hbots serve` alone. A desktop app can start that process as a child. A phone or a browser is the client only and uses `HBOTS_URL`. The client never scans the disk.
- `app/` is the Exact client. The same source is the screen on the web, desktop, and mobile.

The gateway speaks to a harness backend. T3 Code is the one implemented today (`gateway/src/t3.rs`, `T3_URL`, `T3_TOKEN`). Another backend would be another module beside that one. This note does not add that module.

The client holds an hbots session. The harness token stays on the gateway.
