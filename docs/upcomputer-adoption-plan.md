# Adopting ideas from upcomputer: index and sequencing

Upcomputer (`github.com/krl-gr/upcomputer`) is a fork of T3 Code, the same harness hbots runs on. It extends the T3 server in-process, while hbots is an external gateway that only reaches T3 over RPC and MCP. So only ideas that T3 exposes, or that the gateway can build on top of it, transfer. This index links the plans and orders them.

| Plan | Doc | Size | Depends on |
|---|---|---|---|
| Tool-call display levels | [activity-levels-plan.md](activity-levels-plan.md) | S to M | none |
| Safer Bot-authored routines | [routine-safety-plan.md](routine-safety-plan.md) | M | none (pairs with the webhook plan) |
| Provider and quota visibility | [provider-quota-plan.md](provider-quota-plan.md) | M | none |
| Search of a Bot's own history | [history-search-plan.md](history-search-plan.md) | M | none |
| Memory with user and bot ownership | [memory-ownership-plan.md](memory-ownership-plan.md) | L | memory-store-plan Phases 1 to 3 |
| HTML visual replies and MCP Apps | [html-replies-plan.md](html-replies-plan.md) | L | shares serving work with reply-images-plan |
| Pull-request features for code work only | [code-tasks-plan.md](code-tasks-plan.md) | L | none (uses the activity plan for one link) |
| Smaller items | [small-items-plan.md](small-items-plan.md) | S each | none |

## Recommended order

1. Approval cap and provider switch from the small items (one spike each).
2. Tool-call display levels. Cheapest visible win, and it unblocks the code and routine plans' chat hooks.
3. Safer routines. A real safety gap today, and the webhook plan needs the mechanism.
4. Provider and quota visibility. Turns the existing reactive "Limited" chip into a warning.
5. History search tool, then the person-facing search.
6. Memory ownership, once the memory store plan has its Phase 3.
7. HTML replies, alongside reply images.
8. Code tasks, read-only first, behind `beta.code`.

## Shared Phase 0 spikes

Several plans need the same facts. Do them once, save redacted fixtures, and reference them:

- A real thread projection with commands, edits, MCP tools, a routine run and a failed provider call (activity, history search, HTML replies, code).
- A real `server.getConfig` (provider quota, approval cap).
- The scope of the hbots T3 token against `orchestration.searchThreads`, `server.refreshProviders`, `server.getUsageSummary`, `assets.createUrl`, `vcs.*`, `pullRequests.*`.
- Exact capabilities: iframe or webview, `message` events, per-device storage, link handling on load (activity, HTML replies, prompt links).
- Whether the T3 build in use (stable versus nightly) has `html_render`, `orchestration.searchThreads`, `usageLimits` and the scheduled-task fields.

## Cross-cutting rules

- Rust files at most 500 lines (`scripts/check-rs-length.sh`), app files at most 300. Several touched files are close: `chat/attachments.rs` (405), `transcript/project.rs` (387), `chat/routines/mod.rs` (374), `app/bot/panel.contract` (281), `app/ui/shapes.contract` (296). Each plan puts new code in new files.
- No new field inside `messages`. Extra rows are siblings (`events`, `activity`, `renders`) merged by `ordinal`, so token lines and the memory owner stream stay untouched.
- Every plan has a gate or a default that leaves today's behaviour unchanged: `quiet`, `ask`, `beta.code` off, tool listed only when its setting is on.
- Privacy: no emails, fingerprints or tokens leave the gateway; tool output at detail level is capped and redacted; a Bot can read only its own history.
- Prose rule from `CLAUDE.md`: no em-dashes in docs or UI strings.

## What was left out on purpose

- Upcomputer's task board, human tasks and task-status-triggered agents (in-process T3 extensions; hbots routines cover the need).
- Computer use (native sidecar). If it ever matters, borrow its policy: app allowlist, sensitive-app denylist, observe-only mode.
- Guest links (needs T3 Connect, and overlaps `docs/multi-user.md`).
- Outside-agents MCP server for hbots (larger; revisit after the Bot tool server is stable).
- Importing existing agent sessions with `agentSessions.*` (good onboarding idea, not yet explored).
