# Plan: search of a Bot's own history

Give a Bot (and the person) a way to search the Bot's long conversation, including messages far outside the current context. Borrowed from upcomputer's `bot_history_search`, built on T3's `orchestration.searchThreads` with a local fallback.

Status: plan. Nothing here is built. Facts are tagged **[verified]** or **[to verify]**.

## 1. Facts

### 1.1 T3 **[verified in upcomputer source]**

- RPC `orchestration.searchThreads` (`ORCHESTRATION_V2_WS_METHODS.searchThreads`). Input `{ query (2 to 200 chars after trim), limit? (1 to 50) }`. Result `{ matches: [{ threadId, projectId, source: "user"|"assistant", snippet (at most 240), messageCreatedAt, quality?, hits?, messageId? }] }`.
- `quality`: 1 for whole words, 0.8 for word prefix, 0.3 inside words, plus 0.25 when the words appear together as typed, else 0.1 when near. `hits` counts how many of the thread's newest messages match. Older servers omit `quality`, `hits` and `messageId`.
- It searches the whole environment. There is no project or thread filter in the input. The server's SQLite client is single-connection, so the query is bounded in scan and response size.
- Thread tags exist (`StoredThreadTags`, event `thread.tags-updated`, tag at most 64 chars, one line). Not needed here.
- Upcomputer's tool takes `{ query, index? }` and returns a bounded page around the selected match, for the bot's own permanent conversation only.

### 1.2 Michel **[verified]**

- `getThreadProjection` returns the turn items; `transcript()` turns them into messages; `projection_partial` reports `truncated`, `hasMore` or `partial`.
- `without_group_turns` hides framed prompts (`[Group "...`, `[Computer]`) and the Bot's answers to them from a one-to-one chat. Raw T3 search sees those messages.
- A scheduled routine run is a `user_message` with `scheduledTaskId` in the Bot's main thread.
- The Bot tool server `POST /mcp/bot` (`platform/mcp.rs`) routes memory tools via `bots::memory_tools::is_memory_tool`. The caller is the Bot whose `mainThreadId` equals the `threadId` argument; slug and paths in arguments are ignored. Subagents (`role` `subagent` or `hideFromSidebar`) are known and can read the parent's files.
- `bots/lineage` resolves a Bot's child threads (`thread_for`).
- Usage keeps a per-thread turn cache keyed by the thread's `updatedAt` stamp (`bots/usage/mod.rs`), a pattern to reuse.
- Rust files are capped at 500 lines; app files at 300.

### 1.3 Unknowns **[to verify]**

1. Does the T3 build in use have `orchestration.searchThreads`, and does the Michel token's scope allow it?
2. Result ordering and how many non-Bot threads crowd the 50-match limit.
3. Whether `messageId` in a match equals the `id` of the turn item in the projection.
4. How the app pages older history (is `partial` resolvable with a "load older" call?).

## 2. Goals and non-goals

Goals
- A read-only Bot tool `history_search` returning ranked snippets plus a window of messages around the selected match.
- Strict scope: the caller's own main thread and its own child threads. Never another Bot, never a group room unless decided.
- Works without the T3 method through a bounded local scan.
- Later: a "Search this chat" box for the person.

Non-goals
- Searching memory notes (that is `memory_search` in `docs/memory-store-plan.md`).
- Semantic or embedding search.
- Searching other Bots' conversations.

## 3. Design

### 3.1 Modules

New directory `gateway/src/bots/history_tools/` (each file under 500 lines):

| File | Contents |
|---|---|
| `mod.rs` | tool definitions, `is_history_tool`, entry `call` mirroring `memory_tools` |
| `scope.rs` | the set of thread ids a caller may search |
| `score.rs` | pure tokenizer and scorer (word 1.0, prefix 0.8, inside 0.3, together +0.25, near +0.1), used by the fallback and to re-rank |
| `source.rs` | `Searcher` trait with `T3Search` and `LocalScan` implementations |
| `window.rs` | builds the page around a match from a projection |
| `tests.rs` | fixtures |

Routing: `platform/mcp.rs` adds `history_tools::is_history_tool` next to the memory check.

### 3.2 Tool

```json
{ "name": "history_search",
  "description": "Search your own permanent conversation, including old messages outside your current context. Returns up to 10 ranked matches and, for the match at `index` (default 0), a bounded page of surrounding messages.",
  "inputSchema": { "type": "object",
    "properties": { "threadId": {"type":"string"}, "query": {"type":"string","minLength":2,"maxLength":200},
                    "index": {"type":"integer","minimum":0,"maximum":9} },
    "required": ["threadId","query"] },
  "annotations": { "readOnlyHint": true } }
```

Result: `{ matches: [{ index, messageId, role, at, snippet, hits, quality }], selected: { index, messages: [{ id, role, at, text }] }, truncated }`. The selected page holds at most 7 messages (3 before, the match, 3 after), each cut to 1,200 characters, the whole result under 12 KiB. Text from `scheduledTaskId` turns is labelled `routine`, never `you`.

### 3.3 Scope

`scope.rs` builds an allowlist: the caller's `mainThreadId` plus the child threads from `lineage` rows whose parent is the caller. A subagent caller gets its own thread only (decision D1 for parent history). Group room threads are excluded in v1 (decision D2). Results from any thread outside the allowlist are discarded before any further read, and the unit test asserts no snippet from another Bot can be returned even when T3 ranks it first.

### 3.4 Search path

1. `T3Search`: call `orchestration.searchThreads` with `limit 50`. Filter to the allowlist. If fewer than 3 remain and 50 came back, the Bot's matches may be crowded out: fall through to step 3.
2. Verify the survivors: fetch each distinct thread's projection once (cached by `updatedAt`, like usage) and keep a match only if its message appears in `transcript()` output, which drops framed group turns. Re-rank with `score.rs`.
3. `LocalScan`: read the allowlisted threads' projections, score every visible message, cap the scan at 5,000 messages and 2 MiB of text, newest first. Used when the T3 method is missing, errors, or is crowded out.
4. If the projection is `partial`, say so in `truncated: true` so the Bot knows older history was not scanned.

### 3.5 Instructions

The AGENTS.md managed block (`bots/agent_context`) lists the tool next to `memory_read`: use it when asked about something said long ago, before saying you do not remember. Tool names must not collide with the memory plan (`memory_search` is for notes).

### 3.6 Person-facing search (optional)

`GET /api/bots/{slug}/history?q=&limit=` returns the match list; `GET /api/bots/{slug}/history/around?id=` returns the page. The app adds a search field in the chat header and a results sheet. Jump to a message needs older history loading (unknown 4); until then results open in a read-only page view.

## 4. Phases

### Phase 0: spikes (S)

- S0.1 Call `orchestration.searchThreads` with the Michel token. Record the response, ordering, and error shape when absent.
- S0.2 Measure crowding: with several Bots and a common word, how many of the 50 matches belong to one Bot.
- S0.3 Compare `messageId` with projection item ids.
- S0.4 Check transcript paging and `partial` handling in the chat API.
Exit: path 1 or path 3 as primary decided; D1 and D2 answered.

### Phase 1: scoring and window, pure code [PR 1], M

- `score.rs`, `window.rs`, `scope.rs` with unit tests: word versus prefix versus inside, quote handling, window at start and end of history, 1,200-character cut, routine labelling, framed-turn exclusion.

### Phase 2: sources [PR 2], M

- `Searcher` trait, `T3Search`, `LocalScan`, projection cache by `updatedAt`, crowd-out fallback.
- Tests with a mock harness: T3 absent, T3 error, crowded results, partial projection, other-Bot threads ranked first.

### Phase 3: tool and instructions [PR 3], S

- Definitions, routing in `platform/mcp.rs`, caller check identical to `memory_tools`, AGENTS.md bullet, tests for bad `threadId`, subagent scope, size caps.
Ship dark if wanted: tool listed only when the setting `historySearch` is on.

### Phase 4: person-facing API and UI [PR 4], M

- Routes, result sheet in the chat header, strings, contract tests. Message jump deferred to Phase 5 if paging is not ready.

### Phase 5: jump to message [PR 5], M (optional)

- "Load around" in the chat API; scroll to ordinal; highlight.

### Phase 6: hardening and docs [PR 6], S

- Rate limit per Bot (5 per minute), query logging off, README and CHANGELOG lines.

## 5. Test strategy

- Pure scorer and window tests; no I/O.
- Mock `Harness` for T3 behaviours.
- A privacy test: two Bots share a word; Bot A's call never returns Bot B text, with T3 ranking B first.
- A size test: 10 long messages stay under the 12 KiB cap.

## 6. Risks

| Risk | Mitigation |
|---|---|
| T3 search crowds out the Bot's own matches | Allowlist filter, crowd-out fallback to local scan |
| Local scan is slow on huge threads | Caps (5,000 messages, 2 MiB), projection cache, newest first |
| Framed group prompts leak into snippets | Verify against `transcript()` output |
| Search results prompt-inject the Bot | Results are data in a tool result; label roles; the Bot's instructions already treat tool output as data |
| Method missing on stable T3 | Local scan is a complete fallback |

## 7. Decisions for the owner

| # | Question | Recommendation |
|---|---|---|
| D1 | Should a subagent search its parent's history | No, own thread only |
| D2 | Include group rooms the Bot is in | No in v1 |
| D3 | Ship the person-facing search in the first release | Tool first, UI after real use |
