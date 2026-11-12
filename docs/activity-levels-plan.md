# Plan: tool-call display levels

Let the person choose how much of a Bot's work shows in the chat: nothing (today), only problems, a one-line summary per turn, or every call. Borrowed from upcomputer's "Tool calls in chat" setting (hide all, hide successful, Zen), adapted to a messaging app where the quiet view stays the default.

Status: plan. Nothing here is built. Facts are tagged **[verified]** (read in this repo or in upcomputer's source) or **[to verify]** (needs a Phase 0 spike).

## 1. Facts

### 1.1 What hbots shows today **[verified]**

- `gateway/src/harness/adapters/t3/transcript/project.rs` (`project_turns`) keeps only `user_message` and `assistant_message` as messages, plus requests (`approval_request`, `user_input_request`, `secret_request`) and a few events from `event_for`. Every other turn item is dropped: the comment says "Anything else in a turn is not for the person".
- `event_for` turns only these into chips: `dynamic_tool` named `t3_thread_send`, `schedule_task`, `update_scheduled_task`, `delete_scheduled_task`; a `user_message` with `senderThreadId` or `scheduledTaskId`; `subagent`; a terminal provider `error`.
- `tool_of(item)` takes the last `__` segment of `toolName`, so `mcp__t3-code__schedule_task` is `schedule_task`.
- The transcript JSON is `{ messages, events, requests, working, activeRunId }`, merged by `ordinal`. `with_tokens` and the memory owner stream read it too, so a new row kind must not appear inside `messages`.
- App rows (`app/chat/log.contract`, 129 lines): `day`, `event`, `message`, `pending`, `request`, `working`. Events are small chips whose text comes from `eventVerb` in `app/ui/fns.contract`.
- Size rules: Rust files at most 500 lines (`project.rs` is 387, so new code goes in a new file), app files at most 300 (`app/chat/model.contract` is 245, `app/ui/shapes.contract` is 296).
- Settings are one JSON document patched through `PUT /api/settings` (`gateway/src/api/settings.rs`, per-key validation).

So today's behaviour equals upcomputer's "Hide all". The work is adding louder levels, off by default.

### 1.2 What T3 offers **[verified from upcomputer's source, a T3 fork]**

- Turn item `type` values seen in `orchestrationV2.ts`: `reasoning`, `plan`, `file_change`, `command_execution`, `web_search`, `dynamic_tool` (also `subagent`, `error`, requests, messages). MCP tool calls arrive as `dynamic_tool` with `toolName` `mcp__<server>__<tool>`. Exact field names for input, output and failure per type are **[to verify]**.
- Upcomputer's levels (`docs/user/activity-log.md`): default shows everything; Hide successful keeps a call only if it fails; Hide all shows "Thinking"; Zen also hides the agent's in-between messages so a turn shows your message and the final reply. Approvals, questions and errors always show. It is a per-device display setting, applied in the client.
- Its summaries read "Ran 2 commands and sent messages to 3 threads", with counts per destination. Failed calls never count as success, and waiting on a thread does not mean it finished.

## 2. Goals and non-goals

Goals
- Four levels: `quiet` (today), `problems`, `summary`, `detail`. Default `quiet`.
- Optional "final reply only" toggle (Zen) for chatty providers.
- The level is applied in the gateway so a quiet client never receives tool output.
- No change to `messages`, `events`, token lines or the memory owner stream.

Non-goals
- Editing or re-running tool calls from the chat.
- A raw log viewer. Detail is a bounded, redacted view.
- Group rooms in the first release (see Phase 6).

## 3. Design

### 3.1 Levels

| Level | Shows |
|---|---|
| `quiet` | Nothing extra (today). |
| `problems` | A red chip for each failed call or a turn that ended in error. |
| `summary` | One collapsed line per turn, under the Bot's reply: "Ran 3 commands, edited 2 files, searched the web". Expands to the list of calls without output. |
| `detail` | The list expands to one row per call with its one-line target (command, path, URL, query) and a bounded output, fetched on demand. |

`finalOnly` (Zen) is a separate boolean: it hides every assistant message in a turn except the last once the turn has ended. While streaming, the live message shows.

### 3.2 Projection

New file `gateway/src/harness/adapters/t3/transcript/activity.rs` (pure functions, no I/O):

```text
pub(crate) fn activity(items: &[Value], level: Level) -> Vec<Value>
```

Output rows are siblings of `events`, merged by `ordinal` in the app:

```json
{ "id": "activity-<first item id>", "kind": "activity", "ordinal": 41, "at": "...",
  "status": "ok|failed|running",
  "counts": { "commands": 3, "edits": 2, "searches": 1, "browser": 0, "memory": 0, "vault": 1, "tools": 2 },
  "calls": [ { "id": "...", "label": "Ran a command", "target": "cargo test", "status": "ok" } ] }
```

Grouping: consecutive non-message items between two messages form one group, keyed by the first item. Items already shown as events (`t3_thread_send`, the three routine tools, `subagent`) are excluded to avoid a double line.

Mapping table (extend after Phase 0):

| Item | Count key | Target |
|---|---|---|
| `command_execution` | commands | command line, first 120 chars |
| `file_change` | edits | path |
| `web_search` | searches | query |
| `dynamic_tool` `preview_*`, browser tools | browser | URL when present |
| `dynamic_tool` `memory_read`, `skills_read` | memory | none |
| `dynamic_tool` `vault_*` | vault | none, never any input or output |
| `dynamic_tool` `projects_*` | tools | none |
| other `dynamic_tool` | tools | humanised tool name |
| `reasoning`, `plan` | not counted | not shown |

Failure: status in `failed`, `error`, `declined`, or an `error` field present. Running: the same `LIVE` set `project_turns` already uses.

### 3.3 Detail and redaction

- `summary` ships counts and labels only. `detail` ships `target` and a lazy output fetch: `GET /api/chat/{slug}/activity/{id}` returns `{ input, output, truncated }`, each capped at 2 KiB, text only.
- Redaction: never return input or output for `vault_*`; pass output through the same secret scrub the gateway uses for logs (**[to verify]** what exists; if nothing, add a small one: `Bearer`, `password=`, long base64 runs, `sk-` and `ghp_` prefixes).
- Output of image or binary results is replaced by "image" or "file".

### 3.4 Setting

`activityLevel` (`quiet|problems|summary|detail`) and `finalOnly` (bool) in gateway settings, validated like `groupAnswerPolicy`. The transcript endpoint also accepts `?activity=` so a device can override. Whether the app can persist a per-device choice is **[to verify]** (no local-storage use found in `app/`); until then the setting is global.

### 3.5 App

- New component `app/chat/activity.contract`: collapsed row (icon, summary text, chevron) and expanded list. A new `else if m.kind == "activity"` branch in `ChatLog`.
- Strings in `app/strings` for verbs and plurals. Plural handling follows what `app/strings` already does for counts **[to verify]**.
- Keyboard: the row is a button with `aria-expanded`; Enter and Space toggle.
- Settings control: a `Segment` group in Settings, Chat section (name **[to verify]** against `app/settings/screen.contract`).

## 4. Phases

### Phase 0: spikes (S, no production code)

- A0.1 Dump a real thread projection (a Bot that used shell, edits, search, MCP tools). List every distinct `type`, `status` and tool name, and the fields carrying input, output and error. Save redacted fixtures under `gateway/src/harness/adapters/t3/transcript/fixtures/`.
- A0.2 Find every caller of `transcript`, `transcript_for`, `group_transcript` and `with_tokens` (chat API, SSE or polling, usage, memory). Confirm adding a sibling `activity` key breaks none.
- A0.3 Confirm `ordinal` is present and monotonic on non-message items so grouping by ordinal is stable. Check ties with events.
- A0.4 Check whether Exact has local persistence for a per-device setting.
- A0.5 Check the existing log scrubber (grep for redaction in `platform/`).
Exit: mapping table finalised, fixtures committed, decisions D1 to D3 answered.

### Phase 1: gateway grouping and summary [PR 1], M

- `activity.rs` with `Level`, `group`, `summarise`; unit tests over fixtures: grouping across messages, failed call, running call, excluded event tools, vault items without detail, empty turn.
- Settings keys and validation in `api/settings.rs`; default `quiet`.
- Transcript wiring: add `activity` only when level is not `quiet`.
- Tests: with `quiet`, output is byte-identical to today for every fixture (golden test). `messages` and `events` unchanged at every level.
Acceptance: golden test green, file under 500 lines, `scripts/check-rs-length.sh` passes.

### Phase 2: app summary row [PR 2], M

- `activity.contract`, `ChatLog` branch, strings, settings control, `.test.contract` cases: collapsed, expanded, failed colour, running spinner, keyboard.
- Order: the row sits after the turn's last message by ordinal; while streaming it sits above the live message.
Acceptance: app test contract passes; no file over 300 lines.

### Phase 3: detail level and lazy output [PR 3], M

- Route, 2 KiB caps, redaction, copy button, "Output was cut" note, running calls update by polling the same transcript.
- Tests: redaction cases, oversize output, binary output, a call id from another Bot returns 404.

### Phase 4: problems level [PR 4], S

- Failed-only chips; link a failure to the same expanded row. Approval and question cards are unchanged.

### Phase 5: final-reply-only [PR 5], S

- Gateway marks `hidden` on in-between assistant messages of finished turns (kept in the payload so reply and pin targets still resolve) and the app skips them. Search (Find) shows them, as upcomputer does.

### Phase 6: group rooms [PR 6], M (optional)

- Per-worker activity inside a room: `group_transcript` has no sender line for tools, so this needs a per-Bot thread read. Do only if people ask.

### Phase 7: docs and default [PR 7], S

- README features line, CHANGELOG, a note on privacy of tool output. Revisit the default after a release of real use.

## 5. Test strategy

- Fixture-driven pure tests in Rust. Golden test for `quiet`.
- App contract tests for each row state.
- A "no leak" test: a fixture with a `vault_fill_password` call and a command printing a token yields neither at any level.

## 6. Risks

| Risk | Mitigation |
|---|---|
| Item shapes differ across T3 versions | Fixtures from two versions; unknown shapes fall into `tools` with a generic label |
| Secrets in command output | Redaction plus detail-only plus 2 KiB cap |
| Noise swamps the chat | Default `quiet`; summary is one line |
| Double lines with existing events | Excluded tool list, tested |
| Payload growth | No output at `summary`; cap 200 groups per page |

## 7. Decisions for the owner

| # | Question | Recommendation |
|---|---|---|
| D1 | Default level | `quiet` now; reconsider `summary` after real use |
| D2 | Global setting or per device | Global first; per device only if Exact can persist it |
| D3 | Include rooms | No, Phase 6 only on demand |
