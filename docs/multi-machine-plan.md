# Plan: multi-machine product layer (threebot-inspired)

Michel already has a placement design in [bot-execution-environment.md](bot-execution-environment.md): bind a Bot to a linked T3 computer, refuse local RPCs on a remote id, then place behind `beta.environments` once forwarding works. This plan is the **product layer** inspired by threebot commit [1e6da7a](https://github.com/RokaSwK/threebot/commit/1e6da7abb553af08441df33a5aa3a6af6598bfd7): a Machines overview, badges, a way to open remote threads, and outage alerts into Bot chats.

Status: plan. Nothing here is built. Facts are tagged **[verified]** or **[to verify]**. Placement PRs stay owned by `bot-execution-environment.md`; this doc does not replace them.

## 1. Facts

### 1.1 Michel today **[verified]**

- Design doc with PR 1 to 7 for store, list, picker, isolate, place, stay/move, subagent sheet (`docs/bot-execution-environment.md`).
- Lineage already stores `remoteChild: { threadId, environmentId }` and refuses open with "That thread stays on the other machine."
- Projects board links accept `environmentId` on thread and t3project links; overview can list machines as board items.
- Local T3 well-known: `environmentId`, `label`, `platform.machine`, `capabilities.peerLinks`.
- `peerLinks.list` is the link source; Michel session can list (empty on the verification machine). Pairing stays in T3.
- Agents.md: do not send `environmentId` on an RPC this T3 build does not accept; local T3 remains the broker.

### 1.2 threebot 1e6da7a **[verified from commit]**

- `server/machines.mts`: discover local descriptor + peer links + route files; probe `/.well-known/t3/environment`; classify status `online` / `asleep` (laptop unreachable) / `unreachable` / `expired`; cache; optional health probe for a peer threebot.
- `server/machine-alerts.mts`: after two consecutive misses (~2 minutes), find Bots with active `remoteChild` work on that environment; post once per Bot to its main thread; post again when back; persist outage state.
- UI: `MachinesPane`, `MachineBadge`, `RemoteThread` (read a remote thread in-app), Settings and Details hooks, Chat badge.
- Tests and fixture coverage for machines and alerts.

### 1.3 Gaps this plan closes

1. No first-class Machines surface in the Exact app (only projects-board machines and lineage "remote" labels).
2. No badge on a Bot or chat that names the computer.
3. No in-app viewer for a remote child thread (only a refusal sentence).
4. No outage notification into the Bot that delegated work.
5. No asleep vs unreachable distinction for laptops.

## 2. Goals and non-goals

Goals

- A Machines page (or Settings section + inbox strip) listing this computer and each linked peer with status, label, T3 version when known.
- Bot details and chat header show which computer the conversation is on (`boundEnvironmentId` / local).
- When lineage shows a remote child, the person can open a **read-only** remote transcript in Michel once forwarding allows reads; until then, deep-link into T3 if a URL exists, else keep today's sentence.
- Outage watcher: when a linked machine drops while a Bot has active remote children there, message that Bot's main thread once, and once when it returns.
- All of the above stay behind sensible defaults: listing works whenever `peerLinks.list` works; placement and remote read stay gated by `bot-execution-environment.md` R1 / `beta.environments`.

Non-goals

- Replacing T3 pairing UI (no `peerLinks.link` from Michel unless a later plan says so).
- Copying Bot folders across machines.
- Running a second Michel gateway as the remote runtime (threebot's peer health probe is optional inspiration only).
- Rewriting the placement PR sequence in the environments doc.

## 3. Relationship to `bot-execution-environment.md`

| Concern | Owner doc |
|---|---|
| `desiredEnvironmentId`, `boundEnvironmentId`, fallback, tick skip/place | environments doc PR 1 to 7 |
| `GET /api/environments` shape, url stripping, cache | environments PR 2 (extend here for status fields) |
| Machines page, badges, outage alerts, remote transcript UI | **this plan** |
| Projects board machine items | already in projects; keep consistent labels |

Shared types: one gateway `Machine` value used by `/api/environments`, alerts, and the app.

## 4. Design

### 4.1 Machine model

```json
{
  "environmentId": "…",
  "label": "Studio Mac",
  "local": true,
  "status": "online",
  "laptop": false,
  "lastSeenAt": "2026-…",
  "t3Version": "0.1.9-…",
  "lastError": null
}
```

Status enum: `online` | `asleep` | `unreachable` | `expired`. Mapping **[verified in threebot]**: reachable → online; expired → expired; else laptop → asleep, else unreachable.

Probe: well-known on each link url (timeout 5s), cache probedAt for a short TTL (threebot uses a probe interval; start with 60s). Drop raw urls from any client-facing JSON (environments doc already requires this).

### 4.2 API

- Extend `GET /api/environments` (environments PR 2) with `status`, `laptop`, `lastSeenAt`, `t3Version`, `lastError`. Keep `forwarding` boolean.
- `GET /api/machines` may be an alias of the same payload for a dedicated page; prefer one endpoint to avoid drift.
- `GET /api/machines/{environmentId}/threads/{threadId}` (later phase): proxy a read-only projection when `forwarding_supported()` and the link is online; else 409 with `environment_forward` or `environment_unavailable`.

### 4.3 Outage alerts

Module `gateway/src/harness/adapters/t3/machine_alerts.rs` (or `gateway/src/bots/machine_alerts.rs`):

- Input each tick (or on the environments refresh timer): current `Machine[]`, and a function `work(environmentId) -> [{ slug, mainThreadId, threads: [{ threadId, title }] }]` derived from each Bot's main-thread projection `subagents` / lineage remote children with active statuses.
- Misses: increment when status is `asleep` or `unreachable`; ignore `expired` (person must renew the link).
- Threshold: 2 misses (align with threebot). Persist `.botapp/machine-outages.json` with `{ label, since, misses, notified: [{ slug, mainThreadId }], done? }`.
- Notify: `message.dispatch` into the Bot main thread as a framed system-style user message the transcript already shows, copy:

  > From Michel: {label} went to sleep with these threads running: … They are paused until it is back.

  Use `asleep` vs `went offline` from status. Thread links: prefer `t3-thread://` or Michel deep links **[to verify]** what the client opens today.
- Recovery: delete outage, post "… is back online."
- Cap: at most one notify and one recovery per outage per Bot. Failures to post are retried next tick; successes are never repeated.

Do not notify for local machine. Do not notify Bots with zero remote threads on that id.

### 4.4 App surfaces

1. **Machines page** (Exact): list from `/api/environments`, status chip, label, version. Empty: "Link a computer in T3 Code → Settings → Environments." No pairing from Michel in v1.
2. **MachineBadge**: small chip on Bot details header and optional chat subtitle when `boundEnvironmentId` is set or placement is remote. Local: hide badge or show "This computer" only when beta is on.
3. **Lineage row**: if `remote`, button "Open on that computer" → remote transcript route when reads work; else copy that explains the thread stays there, with a link to Machines.
4. **Remote transcript view**: read-only message list, no composer (or composer disabled with note). Reuse chat log components with `inRoom=false` and no send.

Strings: all through `en.json` / `fr.json` (`machines.*`, `chat.environmentDown` already planned in environments doc).

### 4.5 Inspiration we deliberately change

| threebot | Michel |
|---|---|
| Probe peer threebot `:3780/api/health` | Optional later; default off (we do not assume another Michel on the peer) |
| `RemoteThread` full chat chrome | Read-only until product asks for remote send |
| Machines as primary nav | Page under Settings or a top-level item next to Projects; decision D1 |
| Alert copy "From Threebot:" | "From Michel:" |

## 5. Phase 0 spikes

| # | Task | Exit |
|---|---|---|
| M0.1 | Re-read `peerLinks.list` on a machine that has at least one link; save redacted fixture | Real link object shape |
| M0.2 | Confirm environments PR 2 status in tree (implemented vs still design-only) | Section 9 note |
| M0.3 | Whether `t3-thread://v2/{env}/{thread}` opens from Exact / system | Choose link form for alerts |
| M0.4 | Projection fixture with `remoteChild` active statuses | Alert `work()` unit tests |
| M0.5 | Exact nav: new top-level vs Settings-only | Decision D1 |

## 6. Phases

### P1. Status fields on environments list (S)

Depends on environments doc PR 2 landing or lands as that PR's extension.

- Status / laptop / lastSeenAt / t3Version / lastError.
- Unit tests with fixtures (reachable, expired, laptop unreachable).
- No UI yet beyond what PR 3 already needs for the picker.

### P2. Machines page + badge (M)

- Exact Machines page + strings.
- Badge on Bot details when non-local.
- Loading / empty / offline states.
- Browser verification of the page.

### P3. Outage alerts (M)

- Persistence, observe loop hooked to environments refresh.
- Dispatch notify + recovery.
- Tests: two misses, notify once, back online, expired ignored, no local notify.
- CHANGELOG + short SECURITY note that alerts reveal thread titles to the Bot.

### P4. Remote transcript read (M)

Depends on environments R1 / PR 5 style forwarding for `getThreadProjection` with `environmentId`.

- API + Exact read-only view from lineage.
- Until R1: UI shows Machines deep link / explanation only (no fake local projection).

### P5. Polish (S)

- Optional peer Michel health probe behind a setting.
- French copy pass.
- Docs: link from environments doc "Product layer" section to this file.

## 7. Risks and decisions

| Id | Decision | Recommendation |
|---|---|---|
| D1 | Machines as top-level nav vs Settings subsection | Settings → Computers page first; promote if used |
| D2 | Alert as user_message vs events row | user_message into main thread (Bot sees it); keep out of one-to-one filter if needed **[to verify]** transcript filters |
| D3 | Remote send from Michel | Not in this plan |

Risks

- Alert storms if projections flap: require 2 misses and persistent `notified` list.
- Thread titles in alerts may be sensitive: only the owning Bot's thread receives them.
- Implementing UI before environments PR 2: badge has nothing to show; sequence P1 after or with that PR.
- Violating Agents.md by sending unsupported `environmentId`: P4 stays behind `forwarding_supported()`.

## 8. Compatibility

- New `.botapp/machine-outages.json`: local only, not synced (same class as `.bot/local.json`).
- API additive fields on environments JSON: old clients ignore unknowns.
- See [compatibility skill](.agents/skills/compatibility/SKILL.md) when adding Bot fields; badges should use existing `placement` from environments PR 3 if present.
