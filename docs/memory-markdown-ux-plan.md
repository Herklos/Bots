# Plan: Memory as editable Markdown (UX)

Give the person a Memory tab that shows and edits the same Markdown files Bots already use (`memory/MEMORY.md`, shared `.botapp/shared-memory.md`), with revision-safe saves so a Bot edit mid-edit is never overwritten silently. Inspired by threebot commit [f359550](https://github.com/RokaSwK/threebot/commit/f35955039874e31f68d0dac690e925405b45225d).

Status: plan. Nothing here is built. Facts are tagged **[verified]** or **[to verify]**. This plan is the **UX and HTTP layer** on today's files. It does not delete tools, and it does not replace [memory-ownership-plan.md](memory-ownership-plan.md) or [memory-store-plan.md](memory-store-plan.md).

## 1. Facts

### 1.1 Michel today **[verified]**

- Files: `<bots>/<slug>/memory/MEMORY.md`, `memory/log.md`, `.botapp/shared-memory.md`. Bootstrap via `write_if_missing` in lifecycle.
- Snapshot: `bots/agent_context` reads files, injects bounded tags into AGENTS.md, 20s poller refreshes managed block.
- MCP: `memory_read` / `skills_read` only (`memory_tools`). Writes are normal file edits in the Bot folder when the harness allows them.
- Bot panel tabs: `details`, `library`, `computer` (`app/bot/panel.contract`). No Memory tab.
- memory-store-plan designs a structured store, tools, API `/api/memory*`, CLI. Not implemented as that module tree yet.
- memory-ownership-plan wants user-owned facts outside the Bot folder, suggestions, immutable user lines. Also not built.
- Gap called out in ownership plan: "The person has no place to add or review facts."

### 1.2 threebot f359550 **[verified]**

- Removed gated `memory_*` write MCP, tombstones, journal, note store.
- Kept plain `MEMORY.md` + shared file; Bot edits with normal file tools.
- Gateway still copies bounded snapshots into AGENTS.md.
- Memory tab: formatted view, Edit / Save; save sends the revision it started from; conflict returns current text + new revision.
- Large net deletion of server memory machinery.

### 1.3 What we take vs leave

Take: Memory tab, formatted read, Edit/Save, revision (hash) conflict, shared + per-Bot scopes, snapshot stays.

Leave: deleting future store/ownership work; removing `memory_read`; forcing Bots to only use shell edits (they already can). Do not regress the ownership plan's direction.

## 2. Goals and non-goals

Goals

- Bot panel tab **Memory** (and a Shared memory entry in Settings or the same tab with a scope switch).
- GET returns text + revision; PUT requires `revision` and body; conflict → 409 with current text and revision.
- After a successful save, refresh AGENTS.md snapshot path (call existing refresh / rely on poller within 20s; prefer explicit refresh).
- Caps: refuse saves above a large file limit (threebot 200_000 chars is a starting point); snapshot limits unchanged.
- Sanitize saved text so it cannot break managed markers (`<!-- bot:managed:start -->` etc.) when later injected.

Non-goals

- Shipping the full MemoryStore mutate/provenance graph (memory-store-plan).
- User-owned immutable facts (ownership plan); when that lands, the tab gains a second section or read-only user block.
- Editing `memory/log.md` in v1 (read-only footnote or hide). Decision D1: hide log from the editor; show MEMORY.md only.
- WYSIWYG rich editor; Markdown textarea + rendered preview is enough.

## 3. Design

### 3.1 API

```
GET  /api/bots/{slug}/memory?scope=bot|shared
PUT  /api/bots/{slug}/memory?scope=bot|shared
```

GET 200:

```json
{ "scope": "bot", "path": "memory/MEMORY.md", "text": "…", "revision": "<sha256 hex>", "truncated": false }
```

PUT body:

```json
{ "text": "…", "revision": "<sha256 hex of text the editor loaded>" }
```

Results:

| Case | Status | Body |
|---|---|---|
| Saved | 200 | `{ revision, text }` (normalized) |
| Stale revision | 409 | `{ code: "conflict", revision, text, message }` |
| Too large / invalid | 400 | `{ code: "invalid", message }` |
| Unknown slug | 404 | existing pattern |

`scope=shared`: path `.botapp/shared-memory.md`; any Bot slug in the URL is authorized the same as "person can edit shared" (session auth). Alternatively `GET/PUT /api/memory/shared` to avoid a fake slug. **Recommended:** `/api/memory?scope=bot&slug=` and `/api/memory?scope=shared` under `gateway/src/api/memory_api.rs` so shared is not under a Bot route. Match memory-store-plan's intended `/api/memory*` naming.

Revision: sha256 of LF-normalized text (strip CR), hex. Same as threebot `revisionOf`.

Auth: same session as other Bot routes. No Bot MCP token on these routes (person only).

### 3.2 Gateway helpers

New small module `gateway/src/bots/memory_files.rs` (or under future `bots/memory/`):

- `read_file(scope) -> MemoryFile`
- `save_file(scope, text, expected_revision) -> Result`
- `sanitize_memory_text`
- `clip` unused for save; used only if we expose snapshot preview later

On successful save: `Bots::refresh_agents(slug)` for bot scope; for shared, refresh all Bots or bump a generation the poller notices (touch shared file mtime is enough for the 20s poller). Prefer explicit refresh of Bots that include shared in snapshot (all of them): may be heavy; **decision D2**: rely on poller for shared, explicit refresh for bot scope.

### 3.3 App: Memory tab

- Add tab `memory` to Bot panel tablist (details | library | memory | computer, or details | memory | library | computer). Update arrow-key cycle in `tabKey`.
- States: loading, view (rendered Markdown), editing (textarea), saving, conflict banner.
- View: `markup="markdown"` on the text; Edit button; Show path as muted caption (`memory/MEMORY.md`).
- Edit: Load revision into state; Save sends PUT; Cancel restores view from last GET.
- Conflict: banner "Ada changed this file while you were editing." Actions: Reload (take server text) or Overwrite (PUT with server revision after confirm). Default highlight Reload.
- Shared: toggle or second page in Settings → Memory (shared facts for every Bot). Same GET/PUT with scope=shared.

Strings: `panel.memory`, `memory.edit`, `memory.save`, `memory.cancel`, `memory.conflict`, `memory.reload`, `memory.overwrite`, `memory.empty`, `memory.shared`, …

### 3.4 Coexistence with later plans

| Later plan | How this UX adapts |
|---|---|
| memory-store-plan | Tab can switch to fact list UI; file editor remains an "Advanced" or raw mode |
| memory-ownership-plan | User facts section read/write via ownership API; Bot MEMORY.md editor stays for bot-owned raw file |
| Independent policy | No direct coupling |

Do not add MCP write tools in this plan. Do not remove `memory_read`.

### 3.5 Inspiration we deliberately change

| threebot | Michel |
|---|---|
| Delete write-gate memory MCP | Keep read tools; leave store plan intact |
| Single Memory.tsx app surface | Exact Bot panel tab + optional Settings shared |
| 200k char hard cap | Same starting cap; document in API errors |

## 4. Phase 0 spikes

| # | Task | Exit |
|---|---|---|
| U0.1 | Confirm poller picks up a hand-edit of MEMORY.md within 20s on a live Bot | Whether P1 needs forced refresh |
| U0.2 | Exact markdown view + textarea in a panel tab without blowing 300 lines | Split `app/bot/memory.contract` |
| U0.3 | Conflict UX copy review (en/fr) | Strings table |
| U0.4 | Whether library tab already confuses "files the Bot keeps" with memory | Tab order decision D3 |

## 5. Phases

### P1. API + revision save (S)

- `memory_api.rs`, wire router, sanitize, caps, tests (save, conflict, too large, missing file → empty text + revision of empty).
- Explicit `refresh_agents` on bot save.

### P2. Bot panel Memory tab (M)

- Tab, view/edit/save/conflict.
- Contract tests + browser verification on web build.
- en/fr strings.

### P3. Shared memory UI (S)

- Settings or tab scope switch.
- Saving shared does not require a Bot slug in the path.
- Note in UI: "Every Bot sees this."

### P4. Docs and hooks (S)

- CHANGELOG, short paragraph in memory-maintenance.md / memory-store-plan "UX already shipped".
- Link from ownership plan gap #7 as partially closed (person can edit files; ownership still pending).

## 6. Files (expected)

| Area | Files |
|---|---|
| Gateway | `api/memory_api.rs`, `bots/memory_files.rs`, router, tests |
| App | `bot/memory.contract`, `bot/panel.contract`, `bot/model.contract` (tab state), settings if shared, strings |
| Docs | this file, CHANGELOG, cross-links |

## 7. Risks and decisions

| Id | Decision | Recommendation |
|---|---|---|
| D1 | Edit log.md in v1 | No |
| D2 | Shared save refresh | Poller only |
| D3 | Tab order | details, memory, library, computer |
| D4 | Overwrite on conflict | Allowed after confirm |

Risks

- Person and Bot editing the same file: revision handles it; educate via conflict copy.
- Huge paste blocking the gateway: enforce cap.
- Ownership plan later moving user facts out of MEMORY.md: tab must not assume all durable facts live in that one file forever (caption the path).
- Panel.contract near 300 lines: new file mandatory.

## 8. Compatibility

- Additive HTTP API; no change to on-disk Markdown schema.
- Revisions are ephemeral (hash of content), not stored in `.state.json` until the store plan lands.
- compatibility skill: no Bot JSON field changes required for v1.
