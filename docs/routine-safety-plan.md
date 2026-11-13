# Plan: safer Bot-authored routines

A Bot can create a scheduled routine by itself, and that routine then runs unattended and spends the person's model quota. Make that reviewable: a routine a Bot creates starts disabled until the person approves it, and a few guardrails stop runaway or failing routines.

Borrowed from upcomputer's automations (`draft` status "so agent-authored automations can be reviewed before they are allowed to spend money unattended", `failureCount`, `catchUpPolicy`, `skipIfOpen`).

Status: plan. Nothing here is built. Facts are tagged **[verified]** or **[to verify]**.

Related: `docs/webhook-triggers-plan.md` (webhook routines are approval-required by default; this plan supplies the mechanism).

## 1. Facts

### 1.1 T3 scheduled tasks **[verified in upcomputer source, `packages/contracts/src/scheduledTask.ts`]**

- RPCs: `scheduledTasks.subscribe` (stream), `upsert`, `setEnabled`, `delete`, `runNow`, plus webhook delivery methods. Michel uses all five non-webhook ones (`harness/adapters/t3/routines.rs`).
- A task has `enabled`, `schedule` (`interval` with `everyMs` at least 60,000, `fixed_time` with `timeOfDay` and `weekdays`, `once` with `runAt`, `webhook`), `projectId`, `threadId`, `threadMode` (`new_thread`, `continue`, `fresh_session`), `modelSelection`, `runtimeMode`, `interactionMode`, `createdBy` (`user`, `agent`, `system`), `creationSource` (`web`, `mobile`, `mcp`, `provider`, `server`), `nextRunAt`, `lastRunAt`, `lastRunStatus` (`never`, `running`, `succeeded`, `failed`), `lastRunError`, `runCount`, `blockedByPolicy?`.
- Upsert accepts `requireExisting` and `commandId`. `setEnabled` takes `{ id, enabled }`.
- There is no catch-up policy, skip-if-open flag, failure counter or draft state in T3's task. Upcomputer's are in its own task system, which Michel cannot use.
- Agents create tasks through the MCP tool `schedule_task` (and update and delete tools); the T3 docs list `schedule_task` among tools "that run without an active turn".

### 1.2 Michel **[verified]**

- A Bot creates routines itself through T3's `schedule_task`. The transcript recognises it (`event_for`: `schedule_task` with a null `scheduledTaskId` is "Created routine") but does nothing else.
- The gateway lists tasks with `read_until("scheduledTasks.subscribe", ..., |_| true)`, which takes the first chunk and interrupts. `T3Client` also supports a staying-open subscribe (`orchestration.subscribeShell` is used that way).
- `chat/routines/mod.rs` has `new_routine` (task bound to the Bot's main thread, interval at least one minute, model from the Bot), `tasks_for_thread`, and a 50-entry local history file.
- `api/routines_api.rs` has `bot_routines` and `owned_routine`. The memory plan adds `tasks_for_bot` for null-`threadId` routines.
- Approval-style cards already exist: `request` rows with `approval_request` variants and an `answer` action in `app/chat/log.contract`.
- Settings validate per key in `api/settings.rs`; `defaultRuntimeMode` accepts `approval-required`, `auto`, `full-access`.

### 1.3 Unknowns **[to verify]**

1. How soon does a freshly created agent task run? (`nextRunAt` for `interval`, `once`, `fixed_time`.) This is the race window for the gateway to disable it.
2. What `createdBy` and `creationSource` does a task made by `schedule_task` carry, versus one made by the Michel UI through `new_routine`?
3. Does `scheduledTasks.subscribe` push updates while open, and in what frame shape (full list or delta)?
4. What does T3 do when a run is due while the previous one is still running, and after the machine slept? (Queue, skip, or fire once.)
5. Can an agent-authored task have a `runtimeMode` broader than its parent? T3's MCP error codes include `runtime_mode_escalation_denied`, which suggests no.
6. Can `schedule_task` be withheld from Bot threads (token scope, capability, or a T3 setting)?
7. Does `setEnabled(false)` stop a run in flight?

## 2. Goals and non-goals

Goals
- A routine created by a Bot starts disabled in practice and shows as "waiting for your OK" with a clear description of what and when.
- The person can approve, edit or delete it in one step from the chat or the routines sheet.
- Guardrails: a cap on how many a Bot can have, a floor on how often, a clamp on the run mode, and an automatic pause after repeated failures.
- A per-Bot "trust" choice for people who want today's behaviour.

Non-goals
- Preventing a Bot with full filesystem and network access from doing harm some other way. This is a review step, not a sandbox.
- Re-implementing T3's scheduler.

## 3. Design

### 3.1 Honest boundary

The gateway does not own task creation, T3's `schedule_task` does. So the baseline is a reconciler: watch the task list, and when an agent-created task appears enabled, disable it and mark it pending. That leaves a short window (unknown 1). It is acceptable for `interval`, `fixed_time` and `webhook` routines, which cannot run before their first slot. A `once` routine set a few seconds ahead can slip through; the guardrails reduce that and the docs say so. If unknown 6 shows the tool can be withheld, Phase 6 closes the gap by giving Bots a gateway-owned `routine_propose` tool instead.

### 3.2 Store

`.botapp/routine-review.json`, written with `storage::write_atomic`:

```json
{ "version": 1,
  "baseline": "2026-11-12T10:00:00Z",
  "items": { "<taskId>": { "state": "pending|approved|rejected", "slug": "ada",
                            "flaggedAt": "...", "decidedAt": null, "reason": "agent-created" } },
  "failures": { "<taskId>": 2 } }
```

On first start the store is empty and `baseline` is set to now. Every task that exists then is grandfathered (not flagged). Only tasks first seen afterwards are reviewed.

### 3.3 The watcher

New module `gateway/src/chat/routines/review.rs` (kept apart from `mod.rs`, which is 374 lines). A background task owned by `AppState`:

1. Holds one staying-open `scheduledTasks.subscribe`. On disconnect it reconnects with backoff (1 s up to 30 s) and does a full reconcile on every new snapshot.
2. For each task: skip if not bound to a known Bot (`projectId` or `threadId` in the roster) or already in the store. If `createdBy == "agent"` (and `creationSource` in `mcp`, `provider`) and the Bot's review mode is `ask`, call `setEnabled(false)`, record `pending`, and emit a notice.
3. Never touches a task the person created, or any task when the mode is `trust`.
4. All T3 writes use `requireExisting` and idempotent commands, so a reconnect does not double-act.

Rules are pure (`fn decide(task, store, bot_mode) -> Action`) so they are unit-testable without T3.

### 3.4 What the person sees

- A chat card (existing `request` row, new variant `routine_review`) in that Bot's chat: "Ada wants to run **Morning digest** every day at 09:00. It will use Claude Sonnet and work with approval-required." Buttons: **Turn on**, **Change**, **Delete**. Choices map onto the existing `answer` action, routed to `POST /api/bots/{slug}/routines/{id}/review` with `approve` or `reject`.
- A badge on the routine in the Bot's routines sheet ("Waiting for your OK") and in the sidebar unread count.
- The existing "Created routine" event chip changes to "Proposed routine" when the task is pending.
- Approve: `setEnabled(true)`, state `approved`. Reject: `delete`, state `rejected`. Change: opens the existing routine editor, and saving approves.

### 3.5 Guardrails

All are gateway-side checks in the watcher and in the routines API, each its own small function with tests:

| Guardrail | Default | Action |
|---|---|---|
| Max routines per Bot (enabled plus pending) | 10 | Extra agent-created tasks are deleted and a notice says why |
| Max pending at once | 5 | Extras deleted with a notice |
| Floor for agent-authored `interval` | 15 minutes | Upsert with the floor, notice shows the change (T3's floor is 1 minute) |
| Run mode clamp | Bot's own `runtimeMode` | If the task is broader, upsert it down; unknown 5 may make this moot |
| Failure breaker | 3 consecutive `failed` runs | `setEnabled(false)`, event "Paused after repeated failures", store `failures` reset on approve |
| Webhook routines | always `pending` for agents | per the webhook plan |

Settings (`api/settings.rs`, validated): `routineReview` global default `ask|trust`, overridable per Bot; `routineLimits` object with the numbers above and sane bounds.

### 3.6 What we cannot do

T3 has no catch-up policy or skip-if-open. Phase 0 records T3's actual behaviour. If it queues overlapping runs, propose an upstream change; Michel cannot intercept a run start. The failure breaker and the cap are the available substitutes.

## 4. Phases

### Phase 0: spikes (S)

- R0.1 Create agent tasks of each schedule type; record `nextRunAt` relative to creation.
- R0.2 Record `createdBy` and `creationSource` for `schedule_task` versus the Michel UI.
- R0.3 Open `scheduledTasks.subscribe` for a minute while editing tasks; record frames.
- R0.4 Overlap and sleep behaviour (unknown 4).
- R0.5 Runtime mode escalation (unknown 5).
- R0.6 Whether `schedule_task` can be withheld per Bot thread (unknown 6).
- R0.7 `setEnabled(false)` during a run.
Exit: the race window known; D1 to D3 answered.

### Phase 1: store and pure rules [PR 1], M

- `review.rs` types, store load and save, `decide`, guardrail functions; unit tests for each rule, baseline grandfathering, user-created tasks, trust mode, caps, floor, clamp, breaker.

### Phase 2: watcher and enforcement [PR 2], M

- Staying-open subscribe with reconnect, reconcile on snapshot, T3 writes, store updates, notices. Mock `Routines` for tests: reconnect mid-flight, duplicate snapshots, `setEnabled` failure retry, T3 offline at start.
- Never runs before the harness reports ready; never on a harness without routines.

### Phase 3: API and chat card [PR 3], M

- `POST .../review`, list endpoint returns `reviewState`, event variant, `routine_review` request variant, sheet badge, strings, contract tests.

### Phase 4: settings and per-Bot trust [PR 4], S

- Global default and per-Bot override in Bot details, with copy that says plainly what trust means.

### Phase 5: instructions and docs [PR 5], S

- AGENTS.md bullet: new routines wait for the person's approval; say so when you create one. README, CHANGELOG, a section in `docs/PLAN.md` limitations about the race window.

### Phase 6: gateway-owned proposal tool [PR 6], M (only if R0.6 allows withholding `schedule_task`)

- `routine_propose` on `/mcp/bot` creating the task disabled through `upsert`, so there is no race. Remove `schedule_task` from Bot threads' reach.

## 5. Test strategy

- Pure rule tests, table-driven.
- Watcher tests with a mock `Routines` and a scripted snapshot stream.
- A restart test: pending items survive, grandfathered items are never flagged, no duplicate cards.
- A privacy test: the notice and card never include prompt text beyond the title and schedule summary.

## 6. Risks

| Risk | Mitigation |
|---|---|
| Race lets a routine run once before it is disabled | Documented; floor on interval; Phase 6 if feasible |
| Reconciler fights the person's own edits | Only agent-created tasks and only once per task id |
| Subscription drops silently | Reconnect with backoff, full reconcile each snapshot, a heartbeat check |
| Approval fatigue | Per-Bot trust; the card is one tap and states what it will do |
| `createdBy` is not set as expected | R0.2; fall back to "created while no owner message in the last turn" only if needed |
| Gateway down while T3 runs the routine | Out of scope, noted in docs |

## 7. Decisions for the owner

| # | Question | Recommendation |
|---|---|---|
| D1 | Default review mode | `ask` |
| D2 | Interval floor for agent-authored routines | 15 minutes |
| D3 | Reject deletes the task, or leaves it disabled | Delete |
