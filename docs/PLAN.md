# Michel work plan

This plan records what is implemented and what remains. “Built” means the code is present; it is not a claim that every platform or integration has been recently verified. For setup and testing, see [DEV.md](../DEV.md). For product context, see [vision](vision.md) and [architecture](architecture.md).

## Built

### Bots and conversations

- Create and manage Bots with their own instructions, workspace, model and approval mode. Claude Haiku is the default model. Bot details include routines, usage, library, connectors and child threads.
- Chat with Bots and groups. Groups keep their own messages and dispatch to the members' existing threads. An `@mention`, `@all`, or a single plain name selects recipients. Otherwise the answer policy does: last speaker (the default, including rooms saved without the field), every member, or the group's chief. Settings holds the default (`groupAnswerPolicy`); a group can override it. The composer offers mention completion.
- Send attachments and voice clips, preview recorded audio, stop runs and answer requests. Chat supports reactions, replies, pins, unread indicators and search over loaded messages.
- Show activity and routine events as localized chips. Display per-turn token usage when a message and provider turn share an ID; otherwise leave it out.
- Show member activity in groups and open local child threads from Working on. Remote children stay associated with their remote environment instead of opening as the parent. The gateway also supports sending to eligible local child threads; availability depends on the child's state.
- Draw Bot faces and shapes in the client. The separate avatar drawing flow remains open.

### Gateway and tools

- Vault storage, login management, CSV import/export, and Bot tools for saving or using credentials. Browser fill uses T3's preview connection.
- T3 scheduled routines, thread metadata, and approval, question and secret responses. Routine and shell reads use subscription snapshots; an auto-update waits if the busy-thread snapshot cannot be read.
- Bot library records, files and MCP connectors use the `Workspace` interface. The gateway wires `LocalDisk`; the in-memory implementation is used for tests. Other gateway paths still use the filesystem directly.
- Anytype can be the storage backend for tier A files: a Bot's `memory/`, `skills/`, `library/`, and `.bot/attachments/`, plus `.botapp/shared-memory.md`. The folder stays the working copy. Sync state (manifest, outbox, conflicts, history, `sync.log`) lives in `~/.hbots/sync/`, which is local to the machine and is not copied with the Bots folder. Bot records, prompts, and settings do not sync yet. See [the Anytype backend design](anytype-backend.md).
- Bot tools provide read-only memory and skills access. The larger memory store and maintenance feature below is not implemented.
- Watch a Bot's browser and separately take control, release, and send pointer or text input through T3. See the implementation for supported actions and session limitations.
- Site icons, attachments and local usage reporting. Token usage depends on data supplied by T3.
- Password sessions, and a choice of where the gateway listens: this device, the local network, or Tailscale Serve, from Settings → Access → Network. `HBOTS_NETWORK_ACCESS` overrides that choice. The Cloudflare tunnel remains available. The gateway currently uses T3 Code as its harness.

### Delivery and development

- `hbots up`, foreground serving, service management, status, update and rollback. Release archives include the gateway and web app; the installer verifies checksums.
- CI checks formatting, clippy and tests on Linux and macOS. The tag-triggered release workflow builds the web app and packages Linux and macOS archives. The [v0.1.5 release](https://github.com/Herklos/Michel/releases/tag/v0.1.5) published four platform archives and checksums.
- The development watch loop rebuilds the gateway and Exact web client; `--lan` enables access from a trusted local network. See [DEV.md](../DEV.md).
- A shared-home compatibility harness exists under `tests/compat/`. Its documented 2026-11-10 run passed between two Rust Michel processes; compatibility with the earlier Node gateway was not tested in that run.

## Remaining work

### Memory store and maintenance

Build the opt-in durable memory store, provenance, reflection, defrag and note split/merge, along with maintenance routines, CLI, API and client views. The existing `memory_read` and `skills_read` tools are read-only and do not implement this plan. Route the planned memory operations through the workspace adapter, and do not auto-enroll Bots. See [the memory design](memory-maintenance.md).

### Backends and execution

T3 Code is the only production harness. Domain code calls that harness. The T3 adapter is the only place that names T3 RPCs (`scripts/check-t3-boundary.sh`). The workspace interface covers the library and connectors, not every filesystem access. A second production runtime is not built. See [Architecture](architecture.md) and [Gateway layout](gateway-layout.md). [The backend plan](backends.md) is historical. Anytype sync of memory, skills, library, and attachment files is shipped; typed Bot stores on Anytype are not.

Per-Bot execution on a linked T3 computer remains a design, not a shipped setting. See [the execution environment design](bot-execution-environment.md). The local T3 stays the broker. Settings, on the models domain, sets the default computer for new Bots. A Bot's details page, and a durable child's edit sheet, show the computer the conversation is on and the computer it is set to, and can change that setting. The model list is the providers of the computer that Bot is set to. Until T3 can name a link on `server.getConfig`, a Bot set to a link does not show this computer's providers. A one-off child thread that T3 reports on another machine stays associated with that machine. This placement is separate from the harness abstraction in the backend plan.

### Native clients and distribution

The web app is packaged with releases. A macOS native client has been built locally, but the desktop shell that owns the gateway is not complete; iOS and Linux native clients require further build and runtime validation. Android support and signed phone distribution need additional work. Native clients currently bake the gateway URL at build time. See [native builds](native.md).

### Other open work

- Connected apps and the Executor protocol; remote usage telemetry.
- The avatar drawing flow: prompt, model call and saved image.
- The Projects beta board remains off by default; per-Bot records are available through the library.
- Continue validating browser control, voice, child threads and native UI against real harness and platform behavior. Implementation is not a substitute for end-to-end verification.

## Original task history

The initial T1–T10 task briefs and their original completion criteria are available in the [Git history of this file](https://github.com/Herklos/Michel/commits/main/docs/PLAN.md). They describe the initial port and should not be treated as a current work assignment or as proof that every original acceptance criterion was met.
