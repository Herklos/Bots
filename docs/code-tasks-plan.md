# Plan: pull-request and source-control features, shown only for code work

T3 has a large set of source-control RPCs (`vcs.*`, `git.*`, `pullRequests.*`, `sourceControl.*`, `review.*`). hbots uses none of them because most Bots do not write code. This plan adds a Code tab and PR tools that appear only for a Bot that is working on a repository, so the rest of the app stays simple.

Status: plan. Nothing here is built. Facts are tagged **[verified]** or **[to verify]**.

## 1. Facts

### 1.1 T3 **[verified in upcomputer source, `packages/contracts/src`]**

- `vcs.status` takes `{ cwd }` and returns local and remote state: `isRepo`, `sourceControlProvider { kind, name, baseUrl }`, `hasPrimaryRemote`, `isDefaultRef`, `refName`, `hasWorkingTreeChanges`, `workingTree { rootPath?, files [{ path, insertions, deletions }], insertions, deletions }`, `branchChanges { baseRef, insertions, deletions }`, and `hasUpstream`, `aheadCount`, `behindCount`, `pr` (number, title, url, state, draft). A subscription input also exists (`includeRemote`); its method name is **[to verify]**.
- Other `vcs.*`: `refreshStatus`, `pull`, `listRefs`, `createRef`, `switchRef`, `createWorktree`, `removeWorktree`, `init`.
- `git.runStackedAction({ actionId, cwd, action, commitMessage?, featureBranch?, filePaths?, threadId?, projectId? })` runs commit, push and open-PR style sequences. `git.resolvePullRequest` and `git.preparePullRequestThread` exist.
- `pullRequests.*` are keyed by T3 `projectId` (list takes `projectId` or `projectIds`): `list`, `listStats`, `filterOptions`, `detail`, `checks`, `activity`, `summary`, `threadComments`, `comment`, `replyToThread`, `updateComment`, `submitReview`, `setThreadResolution`, `setLabels`, `requestReviewers`, `runAction` (actions: merge, ready, draft, close, reopen, delete-branch, update-branch, enable and disable auto-merge, revert), `stack`, `subscribeRefreshes`, `generateReviewGuide`, `reviewGuide`.
- `sourceControl.*`: `discover`, `signInStart`, `signInWait`, `signInCancel`, `lookupRepository`, `cloneRepository`, `publishRepository`, `appAccess`, `star`, `starred`. Providers include GitHub, GitLab, Bitbucket, Azure DevOps and Forgejo.
- `review.getDiffPreview`, `review.getDiffFileContents`, `review.getBranchCommits`.
- These are user actions through RPC, not agent turns, so T3 does not ask for approval; the client must.

### 1.2 hbots **[verified]**

- No code in `gateway/src` or `app/` calls them. Bots live in `~/.hbots/bots/<slug>` folders; the provisioner creates one T3 project per Bot workspace (`project.create` with `workspaceRoot`) and per subagent child. A Bot folder is not a repository by default.
- The Bot panel has three fixed tabs (`details`, `library`, `computer`) with arrow-key navigation hard-coded in `app/bot/panel.contract` (281 lines of the 300 allowed), so a conditional fourth tab needs a new component file and a small refactor of `tabKey`.
- `put_settings` already has a `beta` object with one allowed key, `projects`.
- Rust files at most 500 lines, app files at most 300.

### 1.3 Unknowns **[to verify]**

1. Which token scopes cover `vcs.*`, `git.*`, `pullRequests.*`, `sourceControl.*`, and whether the scope hbots mints today (`preview:operate`) is enough.
2. `vcs.status` on a Bot folder that contains a cloned repository one level down returns `isRepo: false`. How do we find nested repos?
3. Whether `command_execution` and `file_change` items carry a `cwd` or repository path (for activity-based detection).
4. Method names for the status stream and PR refresh stream.
5. GitHub sign-in state: how `sourceControl.discover` reports it and whether sign-in can only happen in T3's own app.
6. How the multi-user design (`docs/multi-user.md`) should gate write actions.

## 2. Goals and non-goals

Goals
- Hidden unless needed: no tab, no routes with data, no background subscriptions for a Bot that is not working on code.
- For a code Bot: a Code tab with branch state, changed files, open and recent PRs with checks, PR detail, and safe actions (commit, push, open PR, ready, merge) behind explicit confirmation.
- A chat shortcut: "Ask Ada to fix the failing checks".

Non-goals
- A diff reviewer or an editor. Open the provider page for deep review.
- Cloning or publishing from hbots in the first releases.
- Any Git operation done by the gateway itself; always through T3.

## 3. Design

### 3.1 The gate

Three layers, cheapest first, so a non-code install pays nothing:

1. Master switch `beta.code` (default off) in gateway settings. Off: no code routes return data (they return `{ "enabled": false }`), no subscriptions open, and the app renders no Code UI. Add `code` to the allowed `beta` keys in `api/settings.rs`.
2. Per-Bot mode `code: "auto" | "on" | "off"` in the Bot record (default `auto`), set in Bot details only when the master switch is on.
3. `auto` computes a `CodeSignal`, cached 60 seconds per Bot, true when any holds:
   - **repo**: `vcs.status` on the Bot's workspace root, or on a configured repo path, returns `isRepo` (unknown 2 for nested clones: allow the person to attach a repository path to the Bot, which creates or links a T3 project with that `cwd`; do not scan disks).
   - **activity** (only if unknown 3 allows): in the last 14 days the Bot's turns include a `file_change` or a `git` `command_execution` whose path is inside a repo.
   - nothing else (no keyword guessing from message text).

`code: "on"` forces the tab on (with an empty state that explains how to attach a repository); `off` forces it off even when a repo exists.

New module `gateway/src/chat/code/` (each file under 500 lines): `mod.rs`, `signal.rs` (pure: inputs are `vcs` status and optional activity facts, output `Signal`), `status.rs`, `prs.rs`, `actions.rs`, `tests.rs`. Routes in `api/code_api.rs`.

### 3.2 Read model

- `GET /api/bots/{slug}/code` returns `{ enabled, signal, repo?, prs? }`. Repo: branch, default-branch flag, ahead and behind, working-tree totals and up to 50 files, provider name and base URL, linked PR. PRs: open PRs for the repository (limit 20) with title, number, draft, checks state, review decision, updated time, from `pullRequests.list` filtered by the Bot's `projectId`.
- Caching: status 10 seconds, PR list 60 seconds. A single in-flight request per Bot. Remote refresh happens only when the tab is open.
- `GET .../code/prs/{number}` returns detail, checks and unresolved review comments (read only), bounded text lengths.
- Errors map to short states: signed out of the provider, no remote, T3 offline, rate limited.

### 3.3 Write actions

All behind a confirm sheet that names the exact effect, and all idempotent with an `actionId`:

| Action | T3 call | Confirm text |
|---|---|---|
| Commit and push | `git.runStackedAction` (commit, push) | "Commit 3 files with this message and push branch X?" |
| Open PR | `git.runStackedAction` (open PR) | "Open a PR from X into main?" |
| Mark ready or draft, close, reopen | `pullRequests.runAction` | state change named |
| Merge | `pullRequests.runAction` merge | method chosen, requires typed "merge" |
| Reply to a review comment | `pullRequests.replyToThread` | shows the text |

Every action is appended to `.botapp/code-actions.json` (who, when, what). With multi-user on, write actions need the owner role (unknown 6). Nothing is ever sent without a person pressing the confirm.

### 3.4 App

- `app/bot/code.contract` (under 300 lines): repo card, changes list, PR list, PR sheet, confirm sheet, empty and error states.
- `panel.contract` change: tabs become data-driven (`tabs` list including `code` only when `bot.code.enabled`). `tabKey` moves to cycling through that list. This also helps the Computer tab. Keep the file under 300 by extracting the tab strip into `app/bot/tabs.contract`.
- The Settings, Source control screen (sign-in status per provider) appears only when `beta.code` is on.
- No code UI in chat unless the signal is true: the activity summary (see `docs/activity-levels-plan.md`) offers "Review changes" only then.

### 3.5 Chat shortcut

"Ask <Bot> to fix this" on a PR with failing checks composes a message to the Bot's main thread: title, URL, failing check names, and the log excerpt T3 provides, bounded to 4,000 characters. It is a draft the person sends. No automatic dispatch.

### 3.6 Notifications (later)

`pullRequests.subscribeRefreshes` drives event chips: "PR #12: checks failed", "PR #12 merged", only for Bots with a true signal and only while the gateway is running.

## 4. Phases

### Phase 0: spikes (S)

- C0.1 Token scopes: call `vcs.status`, `pullRequests.list`, `git.runStackedAction` (a dry action) with the hbots token. Record errors.
- C0.2 `vcs.status` on a Bot folder, on a folder containing a nested clone, and on a real repo.
- C0.3 Dump `command_execution` and `file_change` items for a coding Bot; look for a path or `cwd`.
- C0.4 Status and refresh stream method names.
- C0.5 Provider sign-in state through `sourceControl.discover`.
- C0.6 What a Bot with a repo workspace looks like in the provisioner (can a person point a Bot at an existing repo path today?).
Exit: gate inputs decided; D1 to D3 answered.

### Phase 1: the gate [PR 1], M

- Settings key `beta.code`, Bot `code` mode, `signal.rs` pure with tests, cached signal, `GET /api/bots/{slug}/code` returning `{ enabled: false }` when off.
- Tests: switch off means zero T3 calls (assert on a mock harness), `auto` with and without a repo, `on` and `off` overrides, cache expiry.

### Phase 2: read-only Code tab [PR 2], L

- Status and PR list routes, caching, `code.contract`, data-driven tabs refactor, strings, contract tests for hidden tab, empty state, loaded, error states.
- Acceptance: with `beta.code` off the DOM has no Code tab and no code requests fire; with it on but a non-repo Bot the tab is absent.

### Phase 3: PR detail [PR 3], M

- Detail route, checks, unresolved comments, bounded text; PR sheet.

### Phase 4: write actions [PR 4], L

- Actions with confirm sheets, `actionId` idempotency, audit log, role check, merge typed confirmation. Tests: duplicate `actionId`, T3 failure surfaced verbatim but redacted, merge blocked when checks fail unless the person overrides.

### Phase 5: chat integration [PR 5], M

- "Ask the Bot to fix" draft, "Review changes" link from the activity summary, notification chips.

### Phase 6: attach a repository [PR 6], M

- UI and API to attach a path to a Bot (validated to exist and be a repo through `vcs.status`), creating or linking the T3 project. Source control sign-in guidance screen.

### Phase 7: hardening and docs [PR 7], S

- Rate limiting, offline states, README and CHANGELOG, a note on what is never done (no clone, no force push).

## 5. Test strategy

- Pure `signal.rs` tests.
- A mock `Harness` that counts calls to prove the gate costs nothing when off.
- Contract tests for the tab list and every sheet.
- A write-action test matrix with a mock that fails at each step.
- A privacy test: provider tokens and remote URLs with credentials are never returned (strip `user:pass@`).

## 6. Risks

| Risk | Mitigation |
|---|---|
| The hbots token lacks scope for these RPCs | C0.1; if so, `hbots up` mints a wider scope only when `beta.code` is turned on |
| Nested repos are not detected | Attach-a-repository flow; no disk scanning |
| A destructive action slips through | Confirm sheets, typed merge, audit log, no force operations exposed |
| UI complexity creeps into every Bot | Three-layer gate; hidden tab; zero requests when off |
| Provider sign-in only works in T3's app | Guidance screen; do not build OAuth in hbots |
| PR data is large | Bounded lists and text; cache; single in-flight request |

## 7. Decisions for the owner

| # | Question | Recommendation |
|---|---|---|
| D1 | `beta.code` default | Off |
| D2 | Allow write actions in the first release | Read-only first (Phases 1 to 3), writes after real use |
| D3 | Activity-based detection | Only if C0.3 finds reliable paths; otherwise repo attach plus manual `on` |
