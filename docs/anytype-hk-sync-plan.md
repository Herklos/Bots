# HK agents space → Bots space sync plan

Status: proposal (2026-12). Inputs: the schema audit table in the request, [anytype-schema.md](anytype-schema.md), [anytype-tier-bc-plan.md](anytype-tier-bc-plan.md), and the HK org type definitions (`anytype-agents` skill, `references/object-types.md`).

## Premise

HK (the `anytype-agents` space) is the org source of truth: 55+ agents, 96 subagents, 91 prompts, 280 memories, 36 skills, 25 groups, 28 schedules. Bots is the runtime space: an `m_*` schema v2 whose typed layer currently writes `m_bot` and `m_settings` only. The question is whether Bots should become a faithful sync destination for HK.

**It makes sense, and PM work items are in scope.** Identity, model stacks, groups, schedules, subagent routes and prompts are the payload that turns an HK roster into runnable hbots Bots — those deserve typed destinations. Memories and skills are page-backed (P5 codec) and large; they sync later. Logs, suggestions, the browser catalog and vault grants have no Bots consumer; they are preserved, not typed. Projects, epics, user stories and tasks sync too, but their value is only realized when the gateway wires them into bot config and agent context — a task object nobody reads is dead weight — so each PM destination ships together with the code that consumes it (Phase 4).

Two hard facts shape everything:

- **An object link never crosses spaces.** Every HK object reference (`reports_to`, `team`, `parent_agent`, …) must be re-resolved against Bots' own objects during import.
- **Property formats are permanent.** `m_welcome_prompt` on `m_group` is already `text` in v2 and can never become an objects link; anything added now must be right the first time.

## Phase 0 — ID and ownership contract (schema change is cheap, wrong mapping is not)

1. **Source identity.** Every HK-imported object carries `m_source_id` (text, the HK object id) and `m_source_space` (text) on the Bots object; `m_source_json` (text, ≤256 KiB) carries the raw unmapped remainder so import is lossless even where no typed destination exists. Uniqueness rule: at most one live object per (`m_source_id`) per type, enforced by reconcile search, not by Anytype constraints.
2. **Link register.** A local manifest (`sync/hk-links.json`, per device, like the P5 path manifest) maps HK id → Bots object id → natural key (`m_slug` / `m_bot_id`). It is a cache; the authoritative join is a `m_source_id` search, so a lost manifest heals.
3. **Ownership domains per property.** Each synced property is tagged at design time:
   - `org` — HK wins; Bots may not write it (role, team, reports_to, subagents, harness profile fields, group roster).
   - `local` — device or Bots owns it; HK values are imported once into the overlay and never overwritten (handover state, T3 bindings, `m_active_device`, paused-by-user).
   - `mirror` — either side may write; last writer wins per property (notes, paused + paused_reason, task status and priority, projects list, display strings).
   The mapping tables in Phase 2 declare the domain for every field. A property without a declared domain is not synced.

## Phase 1 — Read-only survey (before any Prepare)

One-shot dry-run command, `hbots anytype hk-survey`, that reads both spaces and prints the gap: for every HK object, which Bots type and properties it maps to, which fields fall into `m_source_json`, and which cross-space references will not resolve in pass one. No writes. Acceptance: the printed counts match the inventory lower bounds (55 agents / 96 subagents / 25 groups / 28 schedules / 91 prompts, plus real counts for projects, epics, user stories and tasks), and every field in the audit table is classified.

## Phase 2 — Schema v3 (additive Prepare)

New types (all already designed in anytype-schema.md unless noted; keys digit-free, keyed by rule 1, each with template and view):

| New type | Maps HK | Rationale |
|---|---|---|
| `m_prompt` (page-backed) | prompt | Needed for `m_system_prompt` bodies; 91 prompts |
| `m_subagent` | subagent | 96 spawn routes; properties already designed; add `m_spawn_effort` (select), `m_model_override` (text), `m_parent_bot` already present |
| `m_memory` (page-backed) | memory | 280 objects, phase 4 sync |
| `m_skill` (page-backed) | skill | 36 objects, phase 4 sync |
| `m_harness` | harness + runtime | One catalog: HK `runtime_kind` (`hermes`, `osaurus`, `grok-bot`, `noodle`) becomes the Bots harness identity; `m_slug`, `m_vendor`, `m_capabilities_json` |
| `m_log` | log | Type exists in phase 4; sync stays off by default (logs are per-action, high volume, no Bots reader) |
| `m_suggestion` | suggestion | Same: type in v3, sync optional |

New properties on existing types (additive; formats chosen once):

| Type | Add | Why |
|---|---|---|
| `m_bot` | `m_source_id`, `m_source_space`, `m_source_json`, `m_browser` (text, no Bots browser type yet), `m_board_member` (checkbox), `m_rules_version` (text), `m_delegation_enabled` (checkbox), `m_delegation_backend` (text, per design rule 3 — HK select values are outside our control), `m_profile_routes` (text, JSON list of route targets), `m_terminal_backend` (text), `m_computer_surface` (text) | the Agent→m_bot gap list |
| `m_bot` voice group | `m_voice_json` (text, contract: `{enabled, auto_tts, chat_mode, messaging_mode, stt{provider,model}, tts{provider,voice,speed}}`) | voice enablement, modes and STT/TTS overrides; one property instead of seven, contract test in the mapper |
| `m_group` | `m_source_id/-space/-json`, `m_platform` (text), `m_platform_chat_id` (text), `m_profile_slugs` (text, replaces HK `hermes_profiles`; the group `m_welcome_prompt` text property already holds rendered welcome text — the canonical `m_prompt` object is linked from the new `m_welcome_prompt_obj` objects property) | the Groups gap list |
| `m_group` routing | `m_routing_summary` (text); `m_unaddressed_routing` maps onto the existing `m_answer_policy` select — validate the value sets first; if `auto` has no option, mirror rule: `auto` stored in `m_source_json` until schema v4 | lead routing |
| `m_schedule` | `m_source_id/-space/-json`, `m_deliver` (text), `m_skills` (text), `m_workdir` (text), `m_no_agent` (checkbox), `m_script_path` (text) | the Schedules gap list; `hermes_job_id` / `hermes_job_name` stay local (same regime as T3 task ids — runtime writeback, per machine) |
| `m_model_profile` | `m_source_id/-space/-json`, `m_provider` (text), `m_harness_name` (text), `m_aux_json` (text, contract: named aux model stacks: vision, compression, web_extract, skills_hub, title_generation, approval, group_router), `m_voice_json` | provider/harness identity, named aux models, STT/TTS; `m_options_json` / `m_fallbacks_json` stay as-is with a mapper contract test |
| `m_team` | `m_source_id/-space/-json`, `m_members` (objects bots), `m_routing_summary` (text), `m_standing_instructions` (objects `m_prompt`) | HK team (kind Board/HQ) and subteam (kind pod) both land here; `m_parent_team` already exists |

### PM records: epic and user story

HK's `epic` and `user_story` become two new generated record kinds, folded into this same v3 Prepare (the record types are one line each in `RECORD_KINDS` plus their extra properties, per the schema catalog's generation rule). `RECORD_KINDS` grows from 8 to 10; filesystem `library.json` files gain two optional kinds — old files load unchanged, absent kinds render empty.

| Record type | HK source | Extra properties | `statuses()` set owns and maps |
|---|---|---|---|
| `m_epic` | epic | `m_linked_projects` (objects `m_project`, from HK `linked_projects`), `m_user_stories` (objects `m_user_story`), `m_priority` (select P0–P3) | `backlog`, `ready`, `doing`, `done`, `cancelled` ← HK `Backlog`, `Ready` (maybe), `In progress`, `Done`, `Cancelled` |
| `m_user_story` | user_story | `m_linked_projects` (objects), `m_epic` (objects `m_epic`), `m_tasks` (objects `m_task`) | same five-state set |
| `m_task` (existing) | task | add `m_kind` (select `bugfix`,`implementation`,`doc`,`design`,`review`,`chore`,`spike`), `m_epic`, `m_user_story`, `m_priority` | task keeps `todo`,`doing`,`waiting`,`done`; HK extras land in `m_fields_json`; `Cancelled` maps to `done` + `m_fields_json.cancelled=true`, never a new task status |
| `m_project` (existing) | project | add `m_owning_team` (objects `m_team`), `m_exec_policy` (select `local`,`cloud`,`mixed`,`unset`), `m_category` (text, HK's open-ended project_category), `m_one_liner` → existing `m_short_description`, `m_lead` already exists | project keeps `active`,`paused`,`idea`,`done`; HK `Active`/`Other`/`Archived` map `Archived`→`done` + lifecycle `archived` |

`m_priority` and `m_exec_policy` are selects because the sets are closed and owned by hbots (design rule 3 satisfied); HK's other free-text fields come through `m_source_json`. The existing per-kind status validation in the record store applies unchanged to the two new kinds.

Not represented as types, deliberately: HK `browser` (no Bots browser concept; raw JSON + text link), `vault_grant` (Bots has no vault), stray `page` objects (never synced).

Prepare stays idempotent, bumps schema v2 → v3, writes `m_min_gateway = 0.4.5`, and is validated on a scratch space, never on `Bots` as a test.

## Phase 3 — Typed import engine (two-pass, HK → Bots)

Direction is one-way for this phase: HK is source of truth for `org` fields; Bots reconciles them into its stores and materializes runtime state. No HK writes.

1. **Pass 1 — create.** For each HK object in dependency order (harness/runtime → model_profile → team/subteam → agent/bot → prompt → subagent → group → schedule → project → epic → user_story → task), find-or-create the Bots object by `m_source_id`, write scalar properties, stash everything unmapped in `m_source_json`. Idempotent by the existing outbox natural-key scheme (`device:type:natural:op:content-sha`, plus `source` in the key).
2. **Pass 2 — resolve links.** Every HK objects-property is re-read from the register and applied as Bots-side object links: `reports_to`/`team` → `m_parent`/`m_team`, `subagents` → `m_subagents` on `m_bot` (and `parent_agent` → `m_parent_bot` on `m_subagent`), group `members`/`lead_agent` → `m_members`/`m_chief`, `harness_profile` → `m_model_profile`, `system_prompt` → `m_system_prompt` (creates the `m_prompt` object and its body via the P5 codec once available; until then the prompt text lands in `m_prompt_text`-style text and the link is deferred). PM links follow the same rule: `linked_projects` → `m_linked_projects`, `parent_epic`/`parent_user_story` → `m_epic`/`m_user_story`, task `assignee` → `m_assignee` (matched to the adopted bot), project `owning_team`/`lead_agent` → `m_owning_team`/`m_lead`. Unresolved references are reported, never dropped silently.
3. **Transformations.** `role` text stays a select in Bots only for the closed set we own (`bot`, `child`); HK's free-text role goes to `m_short_description` prefix or `m_source_json` — decided per value in Phase 1's survey. HK `autonomy` select values map `read-only|propose|change-with-gates` → `read_only|propose|change_with_gates` (underscore normalization).
4. **Adoption.** An existing Bots bot whose `m_slug` matches an HK agent's `hermes_profile` (P for `p`, Blanche for `blanche`, …) adopts the imported object rather than being duplicated; the m_bot keeps its id, gains `m_source_id`. Collisions where two HK agents share a slug are escalated, never auto-merged.
5. **Store wiring.** Extend `AnytypeBotStore`, then add the model-profile, group, schedule and record Anytype adapters (the four stores' interfaces already exist from the Tier B/C work; records go through the existing `RecordStore` trait, which gains the two new kinds). The HK importer sits on top of the same stores, so a synced HK group or task is indistinguishable from a locally created one downstream.

## Phase 4 — PM records wired into bot config

Bot documents are JSON values in `BotStore` (no typed Rust struct), so the wiring is new shared bot-config fields plus the three consumers that read them. Nothing here is PM-specific state that only Anytype can see; the same fields work in filesystem mode.

1. **Bot config gains shared fields** (m_bot properties mirrored in the `BotStore` profile and the settings page):
   - `projects` (list of record ids): the projects the bot owns or works on; import fills it from HK (`lead_agent` on the project, `assignee` on its tasks), flips the ownership side when the person edits it in either space (`mirror` domain).
   - `execution_policy` (`local` / `cloud` / `mixed` / `unset`, from the project's `m_exec_policy`, bot config may override): feeds the existing runtime-mode logic (`m_default_runtime_mode` on settings is the global default; a project's policy narrows it for work in that project's context).
   - `default_project` (optional): which project a bot's unlinked work lands in.
2. **Agent context injection** (`bots/agent_context`, `bots/instructions.rs`): every turn's context includes a compact PM block, rendered from `RecordStore`, only for bots with a non-empty `projects` list — active projects with one-liner and execution policy, the bot's open tasks and epics (`m_assignee = this bot`, status not `done`), and hierarchy (project → epic → story → task) truncated to a budget. No context for bots without projects: the existing 3-wave context rules stay untouched.
3. **Assignment routing**: a task's `m_record_status` change to `doing` nudges the assigned bot on its home device using the same nudge path schedules use; `m_priority` orders the block; a `spike`-kind task carries its HK kind in `m_kind` so the agent knows it is research, not delivery.
4. **Board and inbox surfaces**: the tasks board and projects board queries already exist in the app; they gain epic/user-story views (kanban by status, filtered by `m_linked_projects` / `m_epic`) and the per-bot agenda view shows assigned open tasks next to schedules.
5. **Write-back scope**: bots may set only `mirror` fields — task status, priority, notes — through a new narrow tool (or the existing record edit path, gated by ownership domain); they never write `org` fields. A bot marking a task `done` on the home device pushes the record update through the same outbox; HK-side sync of that change is phase 6's reverse decision, not phase 4's.

## Phase 5 — Page-backed bodies and knowledge

Prompts first (they unblock `m_system_prompt`), then skills, then memory (280 objects — bulk import behind an explicit confirmation, since these are large bodies at ≤256 KiB each). Requires the P5 anyblock codec. Sync of logs and suggestions stays off; the types exist so a person can move them manually.

## Phase 6 — Continuous reconcile and conflict rules

1. Typed cache + stream invalidation already staged; extend it to HK source changes: a periodic HK search per imported type, diff against the register, outbox push for `org` fields. Poll interval minutes, not seconds; HK is not chat.
2. Conflicts: `org` fields edited in Bots are detected at reconcile and surfaced as `m_conflict`-style entries (person decides which space wins) rather than silently overwritten; `local` fields never conflict; `mirror` fields last-writer-wins by `last_modified_date` (the existing rule: content diff, since `m_updated`/`m_rev` don't move on app edits).
3. Deletion: an HK object archived (`lifecycle=archived`) marks its Bots twin `m_lifecycle=archived`; it is never deleted remotely (matches the "no remote tombstones" gap already documented — fix that gap as part of this phase for both directions).

## Verification

- Mock tests for every mapper (same style as `typed/tests.rs`), including `m_source_json` round-trip, the `m_welcome_prompt`-is-text permanence trap, and the status-vocabulary conversions (HK `Backlog/Ready/In progress/Done/Cancelled` → the unpublished epic set, task `Cancelled` → `done` + flag).
- Scratch-space Prepare v3 (v2 → v3 upgrade path, idempotency, min-gateway guard). Never run Prepare against the live `Bots` space from a test.
- Survey dry-run on real spaces, then a real two-pass import into a disposable copy of the Bots space, then — after review — the real import; acceptance is the inventory counts reconciling with zero unresolved references.
- Back-compat: filesystem mode untouched; the four stores' filesystem adapters keep current behavior; `library.json` with 8 kinds loads unchanged next to the 2 new ones; a bot with an empty `projects` list renders the same context block as today.
- Phase 4 acceptance: context injection shows a bot's assigned open tasks and active projects with their execution policy, a `doing` transition nudges the assignee's home device, and the injected block is absent for projectless bots.
- Full `cargo test` suite plus clippy and the 500-line / T3-boundary checks.

## Sequencing and effort

| Phase | Ships | Depends on |
|---|---|---|
| 0–1 | contract + survey | nothing; read-only |
| 2 | schema v3 Prepare (incl. epic / user-story record kinds) | phase 1 classifications |
| 3 | import engine (bots, profiles, teams, groups, schedules, subagents, PM records) | phase 2 |
| 4 | PM wiring into bot config and agent context | phase 3 |
| 5 | prompts/skills/memory bodies | P5 codec |
| 6 | reconcile + conflicts + archive handling | phases 3–5 |

Phases 0–4 are the ones that make the roster runnable: an HK agent becomes a live Bots bot with its model stack, groups, agendas, spawn routes and its project book. Phase 5 is fidelity for knowledge; phase 6 is the steady-state. Reverse sync (Bots runtime state and bot-made PM changes written back to HK, e.g. applied rules versions and task statuses) is a separate decision after phase 6 and is deliberately not in this plan.
