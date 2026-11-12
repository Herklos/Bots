# Plan: memory with user and bot ownership

Make it a hard rule that a memory the person wrote cannot be changed or removed by a Bot, let Bots propose memories instead of writing them from background runs, and show the person who wrote what. Borrowed from upcomputer's `BotMemory` (`owner: "user" | "bot"`, user entries first in context, "Suggested by a background task run; accepting writes it as an ordinary memory").

Status: plan. This is a delta on `docs/memory-store-plan.md` and `docs/memory-maintenance.md`; read those first. Nothing here is built. Facts are tagged **[verified]** or **[to verify]**.

## 1. Facts

### 1.1 Upcomputer **[verified in `packages/tasks-contracts/src/v1/bots.ts` and `BotMcpTools.ts`]**

- `BotMemory { id, agentId, text (max 2000), source (max 500), createdAt, updatedAt, owner? }`. Missing `owner` means the bot's.
- Context selection (`botMemoryContext`): user entries first, then the bot's, each newest first, packed into a 11,998-character JSON budget. An entry too large to fit is skipped; selection continues with smaller ones.
- Model-facing JSON escapes `<` as `<` so memory text cannot close its own tag.
- Tools: `bot_memory` (paged read, `inContext` flag per entry, `nextOffset`), `bot_remember` (stable `id` per fact), `bot_forget`. The tool descriptions say a user-owned entry cannot be changed or removed: "ask the user to change them in the bot's Memory settings, and never save a memory that contradicts one." The tools are unavailable in background task runs: "put what is worth remembering in the run's result instead."
- Suggestions: a background run proposes a memory; accepting writes `targetId` as an ordinary memory.

### 1.2 hbots today **[verified]**

- Memory is Markdown: `<bot>/memory/MEMORY.md`, `memory/log.md`, and shared `.botapp/shared-memory.md`, injected as a bounded snapshot by `bots/agent_context/mod.rs` (`read_snapshot`, `render_snapshot`, tags `memory`, `memory-log`, `shared-memory`, `parent-memory`, byte caps 4, 2, 2, 2 KiB, a 20-second poller). The tag list is `TAG_PREFIXES`; the strip logic uses it.
- Read tools only: `memory_read` and `skills_read` (`bots/memory_tools/mod.rs`, `FILE_CAP` 16 KiB, truncation flag).
- The memory plan designs `MemoryStore` with `.state.json` holding `revisions`, `provenance` keyed by the sha256 of the normalised fact text with `{ author: "<slug or user>", sourceMessageId, quote, at }`, `forgotten`, a reflection bookmark, and recovery journals. Writes must carry an owner-quote as evidence. `memory_reflect` reads the owner stream, which excludes `scheduledTaskId` turns.
- A Bot's own folder is its working directory, and a Bot at `full-access` can edit any file in it directly with a shell, bypassing tools.
- Shared memory already lives outside any Bot's folder (`.botapp/shared-memory.md`), a precedent for person-owned content kept out of a Bot's reach.
- A routine run is a `user_message` with `scheduledTaskId` in the Bot's main thread (so a background run is not a separate thread).

### 1.3 Gaps this plan closes

1. `provenance.author` can say `user`, but nothing makes a user fact immutable to a Bot.
2. A Bot can bypass tools by editing `MEMORY.md` directly.
3. No way for a Bot (especially during a routine run) to propose a memory without writing it.
4. The snapshot does not order user facts first.
5. `memory_read` returns whole files, with no per-fact owner or in-context marker.
6. Memory text is not escaped against closing its own tag.
7. The person has no place to add or review facts.

### 1.4 Unknowns **[to verify]**

1. Memory plan progress: which of its phases are merged when this starts (this plan depends on its Phases 2 and 3).
2. Whether a Bot's approval mode asks before editing files outside its folder (decides how strong "outside the Bot folder" is).
3. How a tool call can tell it runs in a background routine turn (the last owner message is older than the last routine message).
4. Whether the app can show a notice from a gateway restore (events path).

## 2. Goals and non-goals

Goals
- Two owners for a fact: `user` and `bot`. A user fact cannot be edited, replaced or forgotten through a Bot tool, and a direct file edit by the Bot is detected and undone.
- Bots can propose; the person accepts, edits or dismisses.
- Context order: user facts, then bot facts, newest first, with the existing byte caps and skip-oversize rule.
- The person can add, edit and delete any fact in the Memory tab.

Non-goals
- Semantic conflict detection ("this contradicts that"). We rely on instructions and a lexical overlap hint only.
- Stopping a hostile Bot with shell access. This is integrity for a cooperative Bot with detection, not a sandbox.

## 3. Design

### 3.1 Where user facts live (decision D1)

Option A: same `MEMORY.md`, marked `[owner:user]`, protected by an integrity check. Option B: a separate person-owned file outside the Bot folder: `.botapp/memory-user/<slug>.md`, rendered into the snapshot as its own tagged block (`user-memory`, added to `TAG_PREFIXES`). **Recommended: B.** It mirrors shared memory, keeps the Bot's tool-reachable file free of user content, and makes ordering trivial (render user block first). Integrity is still checked (3.3) because the Bot may read and write outside its folder in full-access.

State for user facts goes in `.botapp/memory-user/<slug>.state.json` (revision, per-fact hash, canonical text, at), written with `write_atomic` and journalled like the rest of the store. It is never read through the `Workspace` trait that Bot tools use.

### 3.2 Fact model additions

In `MemoryStore` types (memory plan `memory/facts.rs`): `Owner { User, Bot(slug) }` stored in `provenance.author`. Add `locked: true` for user facts and `via: "accepted-suggestion"` when a suggestion was accepted. Mutate rules (memory plan `memory/mutate.rs`):

| Action | Target owner | Result |
|---|---|---|
| add | any | allowed with evidence, as in the memory plan |
| replace, forget, move | user | rejected: "That memory is yours to change. Ask <person> to edit it in Memory." |
| replace, forget | bot | allowed with evidence |
| add near-duplicate of a user fact | user | rejected with the existing fact shown (lexical overlap at 0.8 token Jaccard) |

Rejections are recorded in `.state.json` and shown to the person as a quiet notice ("Ada tried to change a memory you wrote"), at most once per fact per day.

### 3.3 Integrity check

On each snapshot read and on the 20-second poller, `MemoryStore::verify_user_facts` compares the user state's canonical text and hash with the user file. Because the file is outside the Bot folder this normally never differs. If it does (edited by hand, or by a Bot), the gateway restores from the state's canonical text, writes a journal entry, and emits a notice "Restored 1 memory you wrote". If Option A is chosen instead, the same check runs against locked lines inside `MEMORY.md`.

### 3.4 Suggestions

New tool `memory_suggest { threadId, text, why, category? }` on `/mcp/bot`. It writes only to `.botapp/memory-suggestions/<slug>.json` (separate from memory, capped at 20 pending, text at most 200 characters like a fact, `why` at most 300), never to `MEMORY.md`. It is the only memory tool available when no owner evidence exists, which is what a routine run has (the memory plan's evidence rule already rejects `memory_edit` there). Behaviour:

- Background-run detection (unknown 3): `memory_edit` already fails without owner evidence; its error message points to `memory_suggest`. No special turn detection is needed.
- Dedupe: a suggestion matching an existing fact or an earlier dismissed suggestion (hash kept 90 days) is dropped silently.
- Accept: the gateway calls the normal `mutate` with `author: bot`, `via: accepted-suggestion`, and evidence "accepted by the person at <time>" (a new evidence kind alongside owner-quote). Edit-then-accept stores the edited text. Dismiss records the hash.

### 3.5 Reading

`memory_read` gains a structured mode while keeping the file mode for compatibility:

```json
{ "facts": [ { "id": "f_9a1c", "owner": "user", "category": "preference", "text": "...", "updatedAt": "...", "inContext": true } ],
  "nextOffset": 40, "limit": 40 }
```

`id` is the first 6 hex of the fact hash prefixed `f_`. Page size 40, each fact at most 200 characters. Order: user, then bot, newest first. `inContext` marks what the injected snapshot shows.

### 3.6 Snapshot rendering

`bots/agent_context` renders `user-memory` first, then `memory`, `memory-log`, `shared-memory`, `parent-memory`. Per-block caps stay (4, 2, 2, 2 KiB plus a new 2 KiB for `user-memory`, decision D2). An oversize fact is skipped and packing continues, as upcomputer does. Memory text is escaped so `<` becomes `<` inside the block (same defence as `TAG_PREFIXES` stripping, but at render time). The model-facing text states: "Lines in <user-memory> were written by <person>. Do not contradict them. To change one, ask <person>."

### 3.7 App

Memory tab (memory plan Phase 8), extended:
- Owner chip per fact ("You", or the Bot's name) and a lock icon on user facts.
- "Add a memory" field (writes a user fact, no evidence needed because the person is the author).
- Edit and delete on every fact.
- Suggestions section above the list: each with the text, the reason, and Accept, Edit, Dismiss.
- A quiet notice list for rejected Bot edits and restores.
The tab is a new component file to stay under 300 lines.

## 4. Phases

Prerequisite: memory-store-plan Phases 1 to 3 merged.

### Phase 0: re-baseline (S)

- M0.1 Confirm which memory phases are in; adjust names below.
- M0.2 Spike unknown 2: with a Bot in approval-required and then full-access, try editing `.botapp/memory-user/x.md`.
- M0.3 Spike unknown 4 (restore notices path).
Exit: D1 and D2 answered.

### Phase 1: user store and snapshot order [PR 1], M

- `memory/user.rs`: read, write, journal, `TAG_PREFIXES` update, snapshot block first, caps, escaping.
- Tests: ordering, oversize skipped, escaping of `</user-memory>` inside text, caps with a mix of owners.

### Phase 2: write rules and integrity [PR 2], M

- Owner checks in `mutate`, lexical overlap rejection, `verify_user_facts` and restore, rejection log.
- Tests: Bot replace of a user fact fails with the message; direct file tamper is restored; restore writes one notice; Option A variant if chosen.

### Phase 3: suggestions [PR 3], M

- `memory_suggest`, store, dedupe, accept, dismiss, evidence kind `accepted`.
- Tests: routine-run turn cannot `memory_edit` but can suggest; 20-pending cap; dismissed hash suppresses repeats; accept applies `via`.

### Phase 4: structured reads [PR 4], S

- `memory_read` structured mode with paging and `inContext`; keep the file mode; update AGENTS.md bullets and tool text with the "cannot change user memories" wording.

### Phase 5: API and Memory tab [PR 5], L

- Routes: `GET/POST /api/memory/user?slug=`, `PUT/DELETE /api/memory/user/{id}`, `GET /api/memory/suggestions`, `POST .../accept`, `POST .../dismiss`, notices. App component, strings, contract tests.

### Phase 6: migration and docs [PR 6], S

- Existing facts stay bot-owned. Offer "Mark as mine" on a Bot fact (moves it to the user file). README, CHANGELOG.

## 5. Test strategy

- Pure tests for ordering, packing, escaping and overlap.
- Journal and recovery tests for the user store (kill between state and file write).
- A tamper test using the real filesystem in a temp dir.
- Privacy: user facts never appear in another Bot's snapshot (shared memory is the only cross-Bot channel and is separate).

## 6. Risks

| Risk | Mitigation |
|---|---|
| Bot edits the user file in full-access | Integrity check restores and notifies; documented as detection |
| Overlap check rejects a harmless fact | 0.8 Jaccard, and the rejection shows the existing fact |
| Suggestion spam | 20 pending cap, dedupe, dismissed hashes |
| Snapshot grows past budget | New 2 KiB cap, user block first, oversize skipped |
| Memory plan phases move | This plan names its dependencies and is re-baselined in Phase 0 |

## 7. Decisions for the owner

| # | Question | Recommendation |
|---|---|---|
| D1 | User facts in a separate file outside the Bot folder, or locked lines in `MEMORY.md` | Separate file (Option B) |
| D2 | Byte cap for the `user-memory` block | 2 KiB, revisit after use |
| D3 | Accepted suggestion owner | Bot-owned with `via: accepted-suggestion` |
