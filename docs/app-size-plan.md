# Plan: bring every app file under 300 lines, then split the rest of app/

Status: in progress. Written for an agent that starts from a clean checkout. Read the whole plan before running anything.

## 0. Decisions (recorded by the owner)

1. New Bot composer: keep the current app behaviour and the current e2e expectation until the product owner decides. No change to either in this work.
2. Screen-only state: may move into screen components, one state at a time, with the owner's approval recorded in the PR description for each.
3. Push a feature branch for review as work completes. Do not push to `main` until the section 6 gates pass on the rebased result and the owner approves.

## 0a. Rules

- Work in your own git worktree. Never edit the main checkout.
- Do not push to `main` until the verification gates in section 6 pass on the rebased result, and the owner has approved the push. Push a feature branch first.
- Commit messages end with `Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>`.
- Behaviour must not change. Every refactor step keeps the compiled plan identical unless the step says otherwise. A changed node count needs a written reason.
- Do not delete someone else's uncommitted work. The main checkout may have unrelated changes.
- The Exact checkout is at `/Users/user/Documents/dev/Contrib/exact` (see `.exact-path`). The compiler is `$EXACT_DIR/target/debug/contract`.

## 1. Where things stand

Branch `app-size-cap` holds 12 commits on top of `8de403d`:

| Commit | What |
|---|---|
| 1d24a7c | `scripts/check-size.mjs` and `scripts/size-baseline.json`, the 300-line rule |
| 106ebec | `ui/shared.contract` split; root styles and types moved to `ui/` |
| 2130052 | `bot/` and `chat/` split one file per component |
| 762895d | Root members moved into one `members` file per feature (14 files) |
| c6bf76f | Vault list screen (`vault/screen.contract`) |
| e7bd5ac | Computer overlay (`computer/overlay.contract`) |
| a5f71e9 | Sign-in screen (`session/lock.contract`) |
| 6097620 | Toast (`shell/toast.contract`) |
| d20b66c | e2e harness fixes (mock `server.getConfig`, first-run test ids, new Bot walk) |
| a8939d2 | Four misplaced comments moved from `BotPanel` to `InboxRow` |
| ad12fa2 | Inbox route (`inbox/screen.contract`) |
| 44a295c | Chat route (`chat/screen.contract`) |

`main` has moved on: 23 commits since `8de403d`, 6 of them touching `app/`. Those change `app/app.contract` (361 lines), `app/app.contract.d.ts`, `app/app.ts`, `app/chat/chat.contract`, `app/group/group.contract`, `app/strings/*.json`, `app/ui/shared.contract` (added `Room.answerPolicyEffective` and `Room.chief`), and move `design/src/lib.rs` into `design/src/engine/`.

A cherry-pick of the branch onto `main` conflicts in `app/app.contract` (one large hunk), `app/app.contract.d.ts`, `app/ui/shared.contract` and `design/src/lib.rs`. It was aborted. Nothing was pushed.

Current size of `app/app.contract`: 810 lines. Target: 300 or fewer.

## 2. Step A: rebase, by re-porting (do this first)

Do not resolve the 12 commits one by one. The split steps are line-range edits, and main has moved those lines. Re-port instead:

1. `git worktree add -b app-size-cap-v2 <path> origin/main`.
2. Re-apply the leaf split from main's current `app/app.contract`. Use `scripts/app-split/` tools (section 8). Take the same declaration groups as commit 106ebec, but read the current file. Keep main's new `Room` fields and any new styles.
3. Re-apply the `bot/` and `chat/` component split (2130052). Re-derive component boundaries with `scripts/app-split/cut.ts`. It cuts at each component's leading comment block, which is what made the first attempt work.
4. Re-apply the members move (762895d) with `scripts/app-split/members.ts`. Check first which names main added: any new state, derive, resource, mutation, action or task must be listed. An unlisted member stays in the root, which is allowed.
5. Re-apply the four screen extractions (c6bf76f, e7bd5ac, a5f71e9, 6097620) and the inbox and chat extractions (ad12fa2, 44a295c) with `scripts/app-split/extract3.ts`. These depend on main's new Settings and group behaviour; check each branch header with `grep -n` before running the tool.
6. Move the `design/src/lib.rs` colours region path to wherever main now keeps the `colours` region (`design/src/engine/*.rs` or the new `lib.rs`). Confirm with `cargo test -p hbots-design` and `cargo run -p hbots-design -- sync --check`. The sync must report no diff.
7. Regenerate `app/app.contract.d.ts` with `contract types app/app.contract -o app/app.contract.d.ts` after the last edit, and commit it alone.
8. Run the gates in section 6 after each step and commit each step separately.

If re-porting a step is harder than expected, stop and report. Do not guess at main's intent.

## 3. Step B: finish phase 4 (route screens)

The root's route branches are inbox (done), chat (done), group, newGroup, settings (259 lines), vault add/edit. Plus the root `if locked` and `toast` (done).

Each extraction uses `scripts/app-split/extract3.ts`:

```
bun scripts/app-split/extract3.ts <app dir> '<branch header line>' '<next branch line>' <ComponentName> <rel file> <comma names> [props]
```

Get the names with `scripts/app-split/names.ts` (it lists root members the branch reads). Remove local variables (for example `title`, `name`) from the list. `extract3.ts` handles `pending()`/`failed()` on resources and mutations by adding root derives, and it discovers each injected type by probing (about one second per round).

After each extraction:
- Fix `e.params.X` by hand: replace with an existing root derive that holds the same value (`slug` for the chat route), or pass a prop.
- Check the child's `use` lines (the regenerator adds them; check no `use` names the root file).
- Name components with a `Screen` suffix when a shape has the same name (`ChatScreen`, not `Chat`).

### The provide budget (the real blocker)

Each screen's `provide` list costs more than its call saves. After the chat step the root grew from 759 to 810 lines. The fix is to localise screen-only members:

- A state, derive or action that only one screen reads, and that no root task, mutation, resource or other screen reads, can move into that screen's component. Children may own state, derives and actions. Resources and mutations and tasks cannot move.
- A state that moves changes lifetime: it resets when the route is left. Confirm each one with the owner before moving it. The default is to stay in the root.
- Write the list first. Use the reference graph approach: for each root member, collect which root-view branches and which members read it.

Target after phase 4: `app/app.contract` at 300 lines or fewer, with each route screen at 300 or fewer, and `settings` split into five components (user, notifications, models, access, about) at 100 to 150 lines each.

## 4. Phase 5: `app/app.ts`

`app/app.ts` (880 lines on main, check) has an import cycle: `host/*.ts` import `API` and `fill` from `../app.ts`, which imports them back.

1. Create `app/host/config.ts` with `HBOTS_URL` and `API`. Change `host/*.ts` to import from it.
2. Move the generated `// design:begin palette` region into `app/host/palette.ts`, with `export { fill }` after the region. Update `design/src/lib.rs` (or `design/src/engine/`) so the region's file path matches. The design sync must show no diff.
3. Split the `sources` map into `app/sources/<domain>.ts`, one per domain (session and vault, bots, groups, chat and voice, settings, library and usage, computer). Each exports a `Pick<Sources, ...>`. Each file at most 300 lines.
4. `app/app.ts` keeps `grants`, `answer` and the composition. Module-level mutable state stays in `host/*`, behind setters (an imported binding cannot be assigned).

Gates: `contract types --check` current, web build succeeds, the design sync has no diff, and `tests/e2e.sh` passes up to where it passes today (section 7).

## 5. Phase 6: `app/app.test.contract`

`app/app.test.contract` has 1,116 lines and a parse error at line 253 (a `key` step). Do not fix the parse error in this phase; move the tests first.

1. Move the 61 tests into `app/tests/<feature>.test.contract` files. Each file starts with `locale "en-US"`. Groups: chat, group, settings, bots, library-computer, mentions, drafts-voice, missing-bot, wide.
2. Each file must stay at 300 lines or fewer.
3. Check each file with `contract test app/tests/<f>.test.contract`. The parse error will show in the file that holds line 253. Record it; do not change its meaning.
4. Check that the agent driver can take the new paths (precedent: `scripts/test-sidebar.mjs`).

## 6. Verification gates (all phases)

Run before each commit:

- `bun scripts/check-size.mjs` exits 0.
- `bun scripts/check-contract.mjs` exits 0.
- `contract types app/app.contract --check` reports current.
- `contract build app/app.contract --errors-only`: slots, derives, resources, actions and nodes match the step's expectation. A pure move keeps all five equal to the previous step. A new `provide` or derive changes the derive count; it must be explained. Node counts have been 6417 on the branch; re-measure on main first.
- `cargo test -p hbots-design` and `cargo run -p hbots-design -- sync --check` (after any `design/` change).
- The web build: `set -a; source .exact-path; set +a; EXACT_APP_DIR="$PWD/app" bun "$EXACT_DIR/host/web/build.mjs" bots-web`.

`contract fmt --check` already fails on `main` for `app.contract` and `bot.contract`. Do not use it as a gate. Do not reformat files in this work.

Size rule: `scripts/size-baseline.json` lists the files still over 300. It may not grow. When a file drops under 300, remove its entry. The baseline must be empty at the end (phase 7).

## 7. Browser and e2e verification

Tooling: Playwright 1.48.2 and Chromium are in `scratchpad/pw` on the machine where the work started. Reinstall on another machine with `npm i playwright@1.48.2 && npx playwright install chromium`, in a directory outside the repo.

`tests/e2e.sh` needs `app/target/web-dist`, which the web build creates. Run it with `NODE_PATH=<pw>/node_modules E2E_OUT=<dir> bash tests/e2e.sh`.

Current status (measured on `main` and on the branch, identical):
- The mock, the gateway and the first-run screen work.
- Creating a Bot works through the walk (name, colour, shape, template, message, review).
- Opening the new Bot's chat fails: the chat resource answers 409 "no conversation yet", and the composer (`data-testid="composer"`) never appears. The e2e expects a composer on a new Bot.

Decision needed from the product owner before the e2e can pass: should a new Bot's chat show the composer before its first message? Until that is answered, do not change the app or the e2e expectation for this. Record screenshots of each screen from the current state anyway, light and dark, so a screen can be compared before and after a move.

Visual checks for each extracted screen: screenshot before the extraction (on `main`) and after (on the branch), at 400x800, and compare them. Use `scripts/app-split/` or `tests/e2e.cjs` as the driver, and keep the screenshots under `scratchpad`, not in the repo.

## 8. Tools (committed under `scripts/app-split/`)

- `members.ts <appdir> <feature> <name,...>`: moves named root members into `<feature>/model.contract` with `members Bots`.
- `cut.ts <src> <out...>`: splits a file at each component's leading comment block.
- `names.ts <appdir> '<header>' '<next>'`: lists root members a view branch reads.
- `extract3.ts`: extracts one branch into a component with probed types (section 3).
- `reuse.ts <appdir> [retired files]`: regenerates `use` lines for every contract file from a declaration map, and carries package imports (`@exact/*`, `exact:content`) from the root into the files that name them.
- `mergeprovide.ts <file>`: folds all `provide` blocks of the root into one.

Known pitfalls:
- `contract fmt --uses` does not restore missing names; use `reuse.ts`.
- `reuse.ts` must exclude the `Bots` component (it would import the root into its own members).
- A child cannot read `pending()` or `failed()` on injected values; the root derives booleans instead.
- `routes nav` stays in the root file.
- Names that are also shapes need a suffix (`ChatScreen`), or `reuse.ts` imports the wrong declaration.
- Run the extractor from a clean root: it reads the branch from the file, so a half-extracted root gives empty name lists.

## 9. Deliverables

- A feature branch pushed to origin, with one commit per step and a PR description listing the gates and their output.
- Screenshots (before and after) for each extracted screen.
- The size baseline empty, and `app/app.contract`, `app/app.ts`, `app/app.test.contract` and every app file at 300 lines or fewer.
- Docs updated: `AGENTS.md`, `DEV.md`, `.agents/skills/exact-client/SKILL.md`, `design/README.md`, `docs/memory-maintenance.md` where they name `app/app.contract` or `app/app.ts`, and a new `app/README.md` with the layout and the inject-versus-props rule.
- A written decision, from the owner, on the composer question and on moving state into screens.

## 10. Stop conditions

Stop and report (do not continue on a guess) when:
- A step changes the compiled plan in a way the plan did not expect.
- A moved state changes behaviour in a way the owner has not approved.
- Main changes the same lines again while the work is in progress.
- A gate fails and the cause is not clear after one attempt.
