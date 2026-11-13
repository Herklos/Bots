# Plan: small items

Five small features borrowed from upcomputer. Each is independent and can ship alone. They are ordered by value for effort.

1. Switching provider in the middle of a chat, with T3's context handoff.
2. A cap on approval modes.
3. Prompt deep links.
4. Files attached to question answers.
5. A diagnostics and triage helper.

Status: the approval cap, provider-switch confirmation, prompt links, and the diagnostics report are in the app. Files attached to question answers stay a separate slice. The answer command has no verified attachment field, so this change does not add one. Facts below are tagged **[verified]** or **[to verify]**.

## 0. Shared facts

- **[verified]** Gateway settings are one JSON document patched by `PUT /api/settings` with per-key validation (`gateway/src/api/settings.rs`). `defaultRuntimeMode` accepts `approval-required`, `auto`, `full-access`. A beta flag object exists (`beta.projects` is the only allowed key today).
- **[verified]** A Bot's `runtimeMode` defaults to `approval-required` in the provisioner and routine builders (`bots/provisioner`, `chat/routines/mod.rs`).
- **[verified]** Answers to approvals and questions go through `chat/approvals.rs` (`outbound`, `command`), carrying a decision, answers or feedback. Secrets use a separate path.
- **[verified]** Attachments are handled by `chat/attachments.rs` (405 lines, close to the 500 cap) and turned into the dispatch command parts by `command_part`.
- **[verified]** The CLI has `up`, `serve`, `status`, `update`, `rollback`, `service` (`platform/cli/mod.rs`). README documents them.
- **[verified]** `thread.model-selection.set` is a command Michel already dispatches (`harness/adapters/t3`).
- **[verified in upcomputer docs]** Providers can switch mid-thread: "Up.computer transfers conversation context when you switch providers" as a budgeted text selection, not a summary and not provider state. Prompt links (`/new?prompt=&repo=&provider=&model=&pr=`) open a draft and never send. Managed policy sets a maximum permission mode, enforced by the T3 server for every client.

## 1. Switch provider mid-chat

### Facts to verify

1. Does `thread.model-selection.set` accept a model from a different provider on an existing thread, or does it need a new thread? `ServerProvider.continuation` (a capability descriptor) hints at what a provider can resume.
2. How T3 reports a handoff (an item in the projection?) and what happens when the target has no room (T3 says it starts a fresh provider conversation with a bounded selection of saved history).
3. Whether the Bot model page (`pickModel`) already lists models across providers and permits the cross-provider choice.

### Design

- Gateway: `PUT /api/bots/{slug}/model` already exists in some form; extend validation to allow a provider change only when the target provider is enabled, installed and signed in (from `providers::list`).
- App: when the picked model's provider differs from the current one, show a confirm sheet: "Ada will move from Claude to Codex. The conversation carries over as a summary of recent messages, not as the old provider's memory of it. Files and tool state do not carry over."
- After the switch, show an event chip "Switched to Codex" from the model-set command's confirmation.

### Phases

- P0 spike: switch a real Bot between two providers; record the projection items, errors, and what the new provider sees. Decide D1.
- P1 gateway validation and event (S), tests with a mock harness for success, unsupported continuation, provider signed out.
- P2 app confirm sheet and strings (S).
- P3 docs (S).

### Risks and decisions

Handoff is lossy and budgeted. D1: allow it freely or only with confirm. Recommend confirm every time.

## 2. A cap on approval modes

### Design

- New setting `maxRuntimeMode` (`approval-required` < `auto` < `full-access`), default `full-access` (no cap). Validated in `api/settings.rs` with the same enum check.
- Enforcement points, each a call to one pure function `clamp_mode(requested, max) -> mode`:
  1. Bot creation and edit (provisioner, `bots_api`).
  2. Routine creation and update (`new_routine`, the routine editor path, and the routine-safety plan's clamp).
  3. Subagent creation (`subagent_tools`).
  4. Default for new Bots (`defaultRuntimeMode` cannot exceed the cap).
- Existing Bots above the cap are not silently rewritten at startup: the Bot list shows "Above the limit" and the next turn dispatch uses the clamped mode (T3 takes the mode per command, **[to verify]** that `message.dispatch` or the thread config carries `runtimeMode`).
- Optional T3 enforcement: T3 has a managed policy file with `limits.maxRuntimeMode` (`up policy check`, enforced by the server for all clients, **[verified in upcomputer docs]**). Michel documents it; it does not write it.

### Phases

- P0 spike: where the runtime mode is sent on dispatch, and what T3 does when asked for more than a policy allows.
- P1 `clamp_mode`, setting, validation, tests (S).
- P2 wire the four enforcement points and the "Above the limit" label (M).
- P3 Settings control and copy (S).

Decision D2: should the cap also apply to existing Bots immediately. Recommend yes at dispatch time, with the label.

## 3. Prompt deep links

### Design

A link opens the app with a draft ready, and never sends:

```text
https://<gateway>/?bot=<slug>&prompt=<text>          (optional: &mode=draft)
```

- Parameters: `bot` (validated with `valid_slug`; unknown means show the Bot picker), `prompt` (at most 8,000 characters, URL-decoded, plain text, never HTML), nothing else. No model, mode, repo or file parameters.
- The page reads the parameters at load, strips them from the address bar after copying them into the composer draft, and requires a signed-in session first (the login redirect preserves the query string).
- "Copy prompt link" in the message menu (the menu already has Copy) and in the composer. The link is built client-side from the current origin; if the origin is localhost the UI warns that it only works on this device.
- Native: a later `hbots://new?...` scheme, not in scope.

### Phases

- P0 spike: how the Exact web build reads the query string on load and after login (`app/host/session.ts`); whether a draft can be set from outside (`app/drafts`).
- P1 parse and validate in the app host (S), including length and bad-encoding tests.
- P2 composer fill and focus, strip the URL (S).
- P3 copy-link actions and strings (S).

Risks: a link as a phishing vector ("send this to your Bot"). It only fills a draft, shows the Bot, and the person presses Send. Decision D3: show a banner "Draft from a link" until sent. Recommend yes.

## 4. Files attached to question answers

### Design

When a Bot asks a question (a `user_input_request`) the answer field accepts files. The answer `command` in `chat/approvals.rs` gains an `attachments` list of the same records `chat/attachments.rs` produces, uploaded first through the existing attachment path. Upcomputer says the agent receives paths to the saved files together with the answer.

- **[to verify]** the T3 command shape for answering a question with attachments (`orchestration.dispatchCommand` response type and where attachments go), and whether T3 saves them on the environment's machine. Upcomputer allows up to 100 files per set of answers and the usual limits.
- Use the existing caps (Michel' limits in `chat/attachments.rs`) and the same MIME checks.
- Because `attachments.rs` is 405 lines, put question-answer glue in a new `chat/answer_files.rs`.
- App: a paperclip in the question card (`request` row in `app/chat/log.contract`), per-question attachment chips, upload progress, remove, retry. Questions with only fixed choices do not show it.

### Phases

- P0 spike: dispatch an answer with an attachment against a real question; capture the command JSON and what the Bot sees.
- P1 gateway: parse, validate, send (M), tests: size, type, missing upload, a stale request id.
- P2 app: card UI, strings, contract tests (M).

## 5. Diagnostics and triage helper

### Design

Upcomputer's triage starts an agent that reads logs and state, checks source and known issues, and asks before posting. Michel needs a smaller thing first: a redacted diagnostics bundle, then an optional "ask a Bot to look at it".

- `hbots doctor [--json]` (new CLI command next to `status`): reports version, install path, service state, gateway port and network mode, T3 URL reachability, T3 token validity and scopes (without printing the token), provider list health from `server.getConfig`, storage path and free space, the last 200 log lines.
- Redaction pass: tokens (`Bearer`, `T3_TOKEN`), the password, session ids, vault paths' contents, emails, and home-directory names replaced by `~`. Tests with a fixture of leaky lines.
- App: Settings, "Diagnostics" button shows the same report with Copy and Save. Optional second step: "Ask a Bot to help" creates a message in a chosen Bot's chat with the redacted bundle attached as a text file and a prompt linking `DEV.md` troubleshooting. It never posts to GitHub; the person decides.

### Phases

- P0 decide the report fields and the redaction list (S).
- P1 `doctor` command, redaction, tests (M). Files stay under 500 lines: `platform/cli/doctor.rs` and `platform/redact.rs`.
- P2 `GET /api/diagnostics` and the Settings screen (S).
- P3 "Ask a Bot" attach flow (S).

## 6. Order and sequencing

1. Approval cap (S, no unknowns beyond one spike).
2. Provider switch (S, one spike).
3. Prompt links (S).
4. Diagnostics (M).
5. Question files (M, depends on a spike).

## 7. Decisions for the owner

| # | Question | Recommendation |
|---|---|---|
| D1 | Provider switch confirmation | Confirm every time |
| D2 | Approval cap applies to existing Bots | Yes, at dispatch, with a label |
| D3 | Banner on draft from a link | Yes |
| D4 | Diagnostics "Ask a Bot" step | After the report ships |
