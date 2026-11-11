# Multiple backends: plan

Status: plan only, nothing implemented (2026-11-10). The gateway talks to one harness today, T3 Code. This plan makes the harness and the Bot folder's location swappable behind interfaces, so a second agent runtime is an addition, not a rewrite.

A linked T3 computer is specified in [the execution environment design](bot-execution-environment.md). hbots keeps this one T3 broker and selects a link T3 already has. Settings sets the default computer. A Bot's details page, and a durable child's edit sheet, set that Bot's computer and show where the conversation is running. The model list is that computer's providers. That design does not add a `Harness` implementation and does not move the Bot folder.

## What is T3-specific today

| Where | What is T3-specific |
|---|---|
| `gateway/src/main.rs:56` | `AppState.t3` is a concrete `t3::T3`, used at 17 call sites |
| `main.rs` chat, previews (around lines 826-935) | read, send, stop and previews call `orchestration.getThreadProjection`, `dispatchCommand`, `run.interrupt` |
| `chat.rs` | turns a T3 projection into the gateway's transcript, and builds `message.dispatch` |
| `provisioner.rs:25-124` | `projects.mutate`, `orchestration.launchThread`, brief delivery |
| `routines.rs:14-18` | `scheduledTasks.*` tags |
| `approvals.rs:14-15` | `secrets.answerRequest`, `runtime-request.respond` |
| `thread_meta.rs:35,161` | `<t3-thread:…>` tags and the `t3_thread_send` tool in previews |
| `vault_tools.rs:65,295,404` | browser fill over T3's preview stream, `T3_SECRETS_DIR`, `request_secret` |
| `config.rs:60-73` | startup fails without a T3 token |
| `cli.rs:221-340` | `hbots up` finds `t3`, issues a token, probes `/.well-known/t3/environment` |
| `bots.rs:106,409` | `bot.json` carries `projectId`, `mainThreadId`; `AGENTS.md` says "T3 Code thread" |

The client is already backend-neutral: `app/app.ts` calls only `/api/*`, never `/t3/*`. All the coupling is in the gateway.

"Local machine" is a second, separate assumption. `bots.rs`, `attachments.rs` and `store.rs` read and write `~/.hbots/bots/<slug>/` directly with `tokio::fs`. The harness reads that same folder as its workspace (`workspaceRoot`).

## Design

Two seams, because "which agent runtime" and "where the Bot's folder lives" vary independently.

**1. `Harness` trait** (the agent runtime), in `gateway/src/harness.rs`, with neutral types:

```rust
#[async_trait]
pub trait Harness: Send + Sync {
    fn id(&self) -> &'static str;                 // "t3"
    fn capabilities(&self) -> Capabilities;       // routines, approvals, browser_fill, attachments, stop
    async fn health(&self) -> Health;
    async fn provision(&self, bot: &BotSpec) -> Result<Binding>;      // idempotent
    async fn transcript(&self, b: &Binding) -> Result<Transcript>;
    async fn send(&self, b: &Binding, msg: Outgoing) -> Result<String>;
    async fn stop(&self, b: &Binding) -> Result<bool>;
    async fn preview(&self, b: &Binding) -> Result<Option<Preview>>;
    fn routines(&self) -> Option<&dyn Routines>;
    fn approvals(&self) -> Option<&dyn Approvals>;
    fn browser(&self) -> Option<&dyn BrowserFill>;
    fn instructions(&self, bot: &BotSpec) -> String;   // the backend's part of AGENTS.md
}
```

`Binding` is an opaque per-Bot JSON value the backend owns. For T3 it is `{projectId, threadId}`. `AppState.t3` becomes `harness: Arc<dyn Harness>`. This needs the `async-trait` crate (or native async-in-trait with boxed futures).

**2. `Workspace` trait** (where Bot folders live): `read`, `write_atomic`, `list`, `remove`, and `root_for_harness(slug)`. `LocalDisk` is the only implementation at first.

## Phases

1. **Extract, no behaviour change.** Move the T3 code to `gateway/src/backends/t3/` (`client`, `chat`, `provision`, `routines`, `approvals`, `browser`, `preview`). Handlers in `main.rs` call the trait. Done when the 145 tests and `tests/e2e.sh` pass unchanged.
2. **Capabilities.** `GET /api/config` returns `backend: {id, capabilities}`. A route the backend lacks answers 501 with a plain message, and the client hides that feature.
3. **Config and CLI.** `HBOTS_BACKEND=t3` is the default; the T3 token is required only for that backend. `hbots up --backend`, and `hbots status` shows backend health. `/t3/*` is mounted only for T3.
4. **A second implementation to prove the trait.** An in-memory `backends/mock.rs`, and one conformance test suite that runs against every backend.
5. **Per-Bot backend.** `bot.json` gains `backend` and `binding`. The gateway keeps reading and writing `projectId` and `mainThreadId` for T3, so an older gateway on the same home still works (the T10 compatibility claim). A registry holds several backends at once.
6. **`Workspace` trait.** Route `bots.rs` and `attachments.rs` through it, then add a non-local implementation. This is the largest phase: about 50 filesystem calls move.
7. **First real second backend.**

Phases 1 to 4 are one focused change each.

## Risks

- **Streaming.** Chat is polled today. The trait should get a `subscribe` stream before a second backend, or each backend will invent its own.
- **Vault browser fill** works only through T3's preview stream. Other backends would report `browser_fill: false`, and Bots would lose `vault_fill_login`.
- **Remote folders.** A harness on another machine can't reach `POST /mcp/bot` on `127.0.0.1`. `HBOTS_MCP_URL` already exists for this, but it needs a reachable address and a token.
- **Unverified T3 shapes.** The attachment record is the gateway's own shape, not a checked T3 schema (`attachments.rs:5-7`). The extraction keeps it as is.

## Decision needed

Which backend comes second? It shapes the trait:

- **Another local agent CLI** (Claude Code or Codex, headless): the gateway must own threads and transcripts itself, so `Harness` needs a storage side.
- **A remote machine or container running T3:** place the Bot with the execution environment design (one broker, an existing link). Phase 6 of this plan is a workspace that is not the local Bot folder. It is the wrong tool for choosing which linked computer runs a Bot.

Phase 1 is the same either way, so it can start first.
