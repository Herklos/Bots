# Migration plan: splitting `app/` into feature folders

Status: plan, nothing migrated yet. Line numbers are approximate, taken from `main` at the time of writing.

## Problem

`app/app.contract` is 3,975 lines. One component, `Bots` (lines 910 to 2769, about 1,860 lines), holds 75 states, 26 resources, 52 mutations, 94 derives, 175 actions, 22 tasks and a 680-line view. `app/app.ts` is 1,348 lines, and its `sources` map alone is 680 lines. Exact's own cap is 1,500 lines per file.

## Decisions

- Screens read root context with `provide` / `inject` (not prop bundles), subject to the phase 0 spike.
- Hard cap of 600 lines per file, soft cap of 400.
- `face.contract` and `glyph.contract` stay where they are.
- Fixing the syntax error in `app.test.contract` (line 564) is not part of this plan. The authored tests are therefore not a gate here.

## What Exact allows

Verified against the Exact docs and a spike (`contract build` on a scratch app with a subfolder `members` file, a `../` `use`, and inject of state, an action and a resource value).

| Rule | Consequence |
|---|---|
| Only the root component owns resources, mutations and tasks. A child owns state, derives and actions. | The 28 resources and 22 tasks stay on `Bots`. |
| `members "./x.contract"` in the root, with a `members Bots` block in that file (state, derive, resource, mutation, action, task). Every file sees every member, and the program is the one-file program. | Splitting the root is a cut and paste with no wiring between feature files. |
| `members` files may live in subfolders. | Feature folders work. |
| Each file has its own names. Every shape, `fn`, style and component a file uses needs a `use` line. `contract fmt --uses app/app.contract` writes them. | `use` lines are mechanical. |
| No `export` keyword, no import cycles, fonts are app-wide. | Layers must form a DAG. |
| A component has one `view`. | The view is split into child components, not members files. |
| `provide` in the root and `inject` in a child work for state, actions and resource values. | Screens avoid 50-prop lists. |
| A child can call `t()` and own state (`ChatMessage` already does). | Safe for extracted screens. |

Not yet verified (phase 0 spike): `pending()` / `failed()` on an injected resource, `top(nav)` / `stack(nav)` in a child, a `provide` list of about 150 names, a `.ts` module in a subfolder using `import.meta.env` and JSON imports.

## Target layout

```
app/
  app.contract              entry: uses, routes nav, root Bots (props, provide, view skeleton)   ~300
  app.test.contract         unchanged
  face.contract glyph.contract   unchanged
  app.ts                    entry only: grants, answer, composes the source modules             ~60
  app.json  app.tamber  package.json  assets/  strings/
  lib/                      pure declarations: no state, no resources
    shapes/   common  bot  chat  room  settings  usage  computer
    fns/      text  mentions  drafts  tabs  models  colours   (colours holds design:begin colours)
    styles/   layout  controls  lists  settings  chat
  ui/                       leaf components, props only
    working-dots  faces  mention-menu  inbox-row  chat-message  screen-card  model-picker  login-sheet
  features/<name>/
    model.contract          `members Bots`: state, derive, resource, mutation, action, task
    screen.contract         components that inject root context
    parts.contract          sub-components, only if the screen passes ~250 lines
  src/                      TypeScript data modules
```

Features: `shell` (poll, fit, toast, scheme, wide), `session` (lock, login, password), `scroll` (follow tasks, jump to end), `drafts`, `voice`, `inbox`, `bots` (create, welcome, rename, sub-agents, panel), `chat` (composer, search, reactions, pins, reply), `mentions`, `group`, `settings` (user, notifications, models, access, updates), `vault`, `usage`, `computer`, `library`.

## Conventions

- Soft cap 400 lines, hard cap 600, enforced by a script (phase 6).
- Leaf components in `ui/` take props only. Screens inject root context, and their inject block documents their dependencies.
- State never moves into a child in this migration. A child's state dies on unmount, which would change behaviour (drafts, scroll, panel tabs).
- Children receive values, never resource or mutation handles. The root derives booleans and error strings (`signingIn = pending(signedIn)`, like the existing `signInError`).
- Members files are named `model.contract`, never `*.test.contract`.

## Source to target map

| Current | Lines | Goes to |
|---|---|---|
| `style` blocks | 15 to 405 (and 3183, 3203) | `lib/styles/*` |
| `shape` declarations | 418 to 836 | `lib/shapes/*` |
| `fn` helpers | 817 to 903 | `lib/fns/*`; the `design:begin colours` region moves with its markers |
| `keyframes dot` | 905 | `ui/working-dots` |
| Root states, resources, mutations, derives | 911 to 1178 | feature `model.contract` files |
| Tasks `poll`, `fit`, `scheme`, `fadeToast` | 1179 to 1195 | `shell/model` |
| Scroll tasks and jump actions | 1218 to 1256, 1771 to 1778 | `scroll/model` |
| Worker-hold tasks | 1257 to 1272 | `group/model` |
| Drafts | 1204 to 1210, 1365 to 1378 | `drafts/model` |
| Voice | 1200, 1392 to 1478 | `voice/model` |
| Composer, attach, send, retry | 1645 to 1722, 1756, 1780 | `chat/model` |
| Reactions, pin, reply, answer, search | 1379 to 1391, 1724 to 1770 | `chat/model` |
| Roster, search, hidden, row menu | 1498 to 1512, 1744 to 1755 | `inbox/model` |
| Create bot, welcome | 1481 to 1531 | `bots/model` |
| Bot panel actions, sub-agents | 1788 to 1872, 1957 to 1977 | `bots/model` |
| Group | 1532 to 1574 | `group/model` |
| Mentions | 1575 to 1613 | `mentions/model` |
| Settings, update, health, theme, network | 1873 to 1935 | `settings/model` |
| Notifications | 1797 to 1818 | `settings/model` |
| Vault | 1936 to 1956 | `vault/model` |
| Computer and its tasks | 1984 to 2088 | `computer/model` |
| View: lock screen | 2091 to 2104 | `session/screen` |
| View: inbox | 2106 to 2200 | `inbox/screen` |
| View: chat | 2201 to 2372 | `chat/screen` |
| View: group | 2373 to 2450 | `group/screen` |
| View: settings | 2451 to 2652 | `settings/screen`, one component per domain |
| View: vault | 2653 to 2687 | `vault/screen` |
| View: new bot, new group | 2688 to 2756 | `bots/screen`, `group/screen` |
| View: computer overlay, toast | 2757 to 2768 | `computer/screen`, `shell/screen` |
| Existing child components | 2769 to 3975 | `ui/*`, or the owning feature (`BotPanel`, `EditBotPage` to `bots`; `UsageView` to `usage`; `BotShelf` to `library`; `GroupPanel` to `group`) |

## `app.ts` split

- `app.ts` keeps `export const grants` and `export const answer`, and builds `const sources: Sources = { ...session, ...bots, ...chat, ... }`.
- Each `src/<domain>.ts` exports a `Pick<Sources, ...>`, so the compiler checks coverage against the generated `app.contract.d.ts`.
- `src/api.ts`: `API`, `call`, `read`, `reply`, `localize`, `say`, `loadSentence`, error constants.
- Other modules: `format.ts` (`when`, `clock`, `dayLabel`, `compact`, `fileSize`), `palette.ts` (BRAND, SWATCHES, `fill`, keeps `design:begin palette`), `fixtures.ts` (the `FIXTURE_*` helpers), `notify.ts` (`postUnread`, `shutNotice`, notice prefs), `drafts.ts`, `chat-rows.ts`, `usage.ts`.
- Domain modules: `session`, `bots`, `groups`, `chat`, `voice`, `settings`, `vault`, `computer`, `usage`, `library`.
- The type import becomes `../app.contract.d.ts` from `src/`.

## Phases

Each phase ends with every gate green and lands as its own PR.

Gates used throughout (the oracle):

- `contract build app/app.contract --errors-only` stats: 149 slots, 94 derives, 28 resources, 217 actions, 4909 nodes (nodes may differ by a few after view extraction, because uses are inlined). Bytes are a soft check (970,139 baseline).
- `bun scripts/check-contract.mjs` passes and `contract types app/app.contract --check` is current.
- `contract fmt --check` passes on every moved file.
- Web build output size within 1% of baseline.
- From phase 4 on: `tests/e2e.sh` runs, plus screenshot diffs of inbox, chat, group, settings tabs, vault and the computer overlay, light and dark, narrow and wide.

### Phase 0: baseline and spikes (no behaviour change)

1. Commit or regenerate the stray `app/app.contract.d.ts` edit (`resetAt` on `Row`) so the baseline is clean.
2. Record the oracle numbers above and the web-dist size.
3. Spikes on the real app:
   - Extract `VaultScreen` with inject. Check `pending()` on an injected resource, `stack(nav)` in a child, and a 150-name `provide`.
   - Check a `.ts` module in `src/` with `import.meta.env.HBOTS_URL` and a JSON import.
   - If inject fails at scale, fall back to prop bundles for screens and rewrite phase 4.
4. Add `scripts/split-contract.mjs`, driven by a manifest of start marker, end marker and destination file, so the moves can be rerun on a fresh `main`. Many `cursor/*` PRs are in flight and git cannot rebase moved code. Announce a short freeze on `app/*` while phases 1 to 3 land.

### Phase 1: pure declarations

- Move styles, shapes and `fn`s into `lib/`, then run `contract fmt --uses app/app.contract`.
- Update `design/src/lib.rs` `outputs()`: region `colours` of `app/app.contract` becomes `app/lib/fns/colours.contract`.
- `cargo test -p hbots-design` passes and `hbots-design sync` produces no diff.

### Phase 2: leaf components to `ui/`

They already take props, so this is a move plus `use` lines.

### Phase 3: split the root's members

- One feature per commit, easiest first: `computer`, `voice`, `drafts`, `usage` and `library`, `vault`, `settings`, `bots`, `group` and `mentions`, `chat`, `scroll`, `inbox`, `session`, `shell`.
- Each commit adds one `members "./features/x/model.contract"` line to `Bots` and removes the moved lines. Resources move with their feature.
- Exact promises the program is identical, so the oracle stats must match exactly.
- After this phase `Bots` is about 950 lines, mostly view.

### Phase 4: extract the view

- Root view becomes `main`, then `if locked` / `LockScreen`, else `for e in stack(nav)` / screen.
- Add a `provide` list on `Bots`, grouped by feature with comments, and an `inject` block per screen.
- Pass `e.params.*` as props. A child under `for` owns state keyed to the row.
- Order: vault (the spike), new-bot and new-group, group, inbox, settings (five domain components), chat, lock screen.
- Done when the root file is 300 lines or fewer.

### Phase 5: split `app.ts`

- `api.ts` and `format.ts` first, then `palette.ts` with the design region. Update `design/src/lib.rs` (region `palette` of `app/app.ts` becomes `app/src/palette.ts`).
- Then the domain modules, one per commit.

### Phase 6: tooling and docs

- `scripts/dev.mjs` line 246 watches `app` non-recursively, so edits in subfolders would not trigger `check-contract`. Add `{ recursive: true }`. The pre-commit hook regex and `check-contract.mjs` need no change.
- Add the 600-line cap check to `check-contract.mjs`.
- Update `AGENTS.md`, `DEV.md`, `.agents/skills/exact-client/SKILL.md`, `design/README.md`, `design/docs/*.md` and `docs/memory-maintenance.md` where they name `app/app.contract` or `app/app.ts`.
- Add `app/README.md`: layout, the inject-vs-props rule, "state stays in members files".

## Risks

| Risk | Mitigation |
|---|---|
| Inject, `pending()` or `nav` behave differently in children at scale | Phase 0 spike on a real screen, prop-bundle fallback |
| Merge conflicts with open PRs | Scripted rerunnable moves, moves-only commits, short freeze |
| State lifetime changes | State stays in members files |
| Behaviour drift | Oracle stats, e2e and screenshot diffs |
| Design sync breaks | Update `outputs()` paths in the same PR, `hbots-design sync` shows no diff |
| Watch loop misses subfolder edits | Recursive watch (phase 6) |
| `strings/en.json` and `fr.json` | Left as they are; already prefix-namespaced |

## Out of scope

- Fixing `app.test.contract` line 564 (`[syntax-expected-newline]`). The authored tests do not run today and CI does not run them.
- Moving `face.contract` and `glyph.contract`, splitting `app.test.contract`, moving state into child components, extracting a local `@hbots/ui` package.
