# Anytype schema for Michel

P4/P5 implementation sequence: [Tier B/C plan](anytype-tier-bc-plan.md).

Companion to [the Anytype backend plan](anytype-backend.md). Status: design only (2026-11-11, second revision after an audit against the Anytype API spec and live probes).

All types live in one dedicated, private Anytype space. Every type and property key starts with `m_`, so nothing collides with the `aa_*` prototype schema or the person's `anytype-agents` org types. The shape is adapted from those org types (agent, team, subteam, subagent, harness profile, computer, tool profile, vault grant, prompt, memory, skill, log, suggestion, schedule, group, project, task) and mapped onto what Michel does.

## Design rules

These come from probes and the OpenAPI spec, not taste.

1. **Keys are normalized by Anytype and are permanent.** A requested type key `hbprobe2` came back as `hbprobe_2`, so digits are split off. Use no digits in keys (`m_sha`, not `m_sha256`), pass an explicit key on every create, and **always read the created key back** and keep the mapping in `m_meta`. Never derive a key from a display name at runtime.
2. **Additive migrations only.** Deleting a type or property does not purge it: objects keep their values, the type stops being listable or filterable, and it can only be restored in the Anytype app. An object with a discussion cannot be deleted permanently at all. Never rename a key, add a new one and migrate.
3. **Select only for closed sets we own.** v2 cannot rename, recolor or delete select options. Anything that follows an outside system (T3 provider ids, model names, runtime modes) is `text`.
4. **Exact data goes in text properties.** A string is capped at 1 MiB and a structured request body at 1 MiB, so keep inline content at or under 256 KiB and spill larger content to a file. An object holds at most 128 properties and 2,048 blocks, a document body 10 MiB, a patch 512 operations, and one insert or replace 256 blocks.
5. **Page-backed types** (prompt, memory, skill, and the body of a few others) keep canonical content in body blocks so a person can edit them in Anytype. The gateway reads `anyblock` JSON and renders markdown itself. `format=md` is never used on a read path.
6. **Links are `objects` properties,** so backlinks and queries work. A type must list the bundled properties it wants queryable (`backlinks`, `links`, `tag`, `created_date`, `creator`, `last_modified_date`, `last_modified_by`); a type created with an explicit property list does not carry them by default.
7. **Property merge is per property and last writer wins.** Multi-property updates are not atomic across machines. Content must not depend on a sibling property to be valid (see the plan, "Concurrency model").
8. **Common properties** on most types: `m_lifecycle` (select `draft`, `live`, `archived`), `m_rev` (number, advisory write counter), `m_updated` (number, unix ms, advisory), `m_device` (text, last writer, advisory), `m_notes` (text). Change detection uses `last_modified_date` plus the content itself, because a human edit in the Anytype app changes neither `m_updated` nor `m_rev`.
9. **Layouts do real work.** `profile` for Bots (avatar centered), `todo` for tasks (checkbox in the app), `note` for notes (no title row), `bookmark` for links (URL preview), `basic` for the rest, `set` and `collection` only for generated containers.
10. **Never stored:** passwords, tokens, bearer headers, the Vault key, session secrets, the T3 token, anything from `.mcp.json` or `.codex/config.toml`. Capability records (who may use which login) are allowed. Credentials are not.
11. **Per-device data is not in these types.** It lives in `m_device` or stays on disk in `.botapp/` and `.bot/local.json`. The storage backend choice itself (backend, folder, Anytype host, key) is local to the machine in `~/.hbots/storage.json` and is never stored in Anytype, because the store cannot hold the setting that says where the store is.
12. **Every type ships with** a default template (body skeleton), a default view, an icon, and a generated query. Views are addressed by minted ids, so `m_meta` stores the view ids per query.

## Core types

| Type | Layout, default view | Purpose | Key properties | Replaces or maps to | Phase |
|---|---|---|---|---|---|
| `m_meta` | basic | Singleton. Schema version, key map, view ids, migration log, minimum gateway version | `m_schema_version`, `m_min_gateway`, `m_keys_json`, `m_views_json`, `m_migrations_json` | none | P2 |
| `m_device` | basic, table | One gateway on one machine | `m_slug` (device id), `m_hostname`, `m_os`, `m_gateway_version`, `m_anytype_version`, `m_last_seen` (number), `m_enabled_bots` (objects), `m_urls` (text, the gateway's reachable addresses), `m_status`, `m_stream_ok` (checkbox) | `~/.hbots/device-id` and new state | P3 |
| `m_settings` | basic | Singleton. Shared preferences | `m_user_name`, `m_main_bot` (objects), `m_default_model_profile`, `m_avatar_model_profile` (objects), `m_auto_review` (checkbox), `m_default_runtime_mode` (text), `m_group_answer_policy` (select `last`, `everyone`, `chief`), `m_rules_allow_json`, `m_rules_ask_json`, `m_sections_json`, `m_beta_json` | shared part of `settings.json`, `main-bot.json`, `apps.json` | P4 |
| `m_bot` | profile, gallery | A Bot. The central object | see below | shared fields of `.bot/bot.json` | P4 |
| `m_file` | basic, table | A synced file, byte exact | `m_path`, `m_dir`, `m_content`, `m_blob` (files), `m_sha`, `m_size`, `m_mime`, `m_mtime`, `m_deleted` (checkbox), `m_owner_bot` | everything through `Workspace` | P3 |
| `m_conflict` | basic, kanban by state | An unresolved disagreement | `m_path`, `m_owner_bot`, `m_conflict_state` (select `open`, `resolved`, `dismissed`), `m_device_a`, `m_device_b`, `m_sha_a`, `m_sha_b`, `m_copy_a`, `m_copy_b` (files), `m_resolution` (text), `m_detected_at` (date) | `.conflict-` files, now first class and visible in Anytype | P3 |

### `m_bot`

| Property | Format | Note |
|---|---|---|
| `m_bot_id` | text | stable uuid. Identity. The slug is a label that can collide across machines (see plan) |
| `m_slug` | text | folder name; unique by rule, not by constraint |
| `name` (object name) | | display name |
| `m_label`, `m_shape`, `m_color` | text | face drawing, values follow the client |
| `m_role` | select `bot`, `child` | |
| `m_parent` | objects `m_bot` | child Bots (`parentSlug`) |
| `m_team` | objects `m_team` | sidebar section or org grouping |
| `m_short_description` | text | one line mission |
| `m_lifecycle` | select | `draft`, `live`, `archived` |
| `m_autonomy` | select `read_only`, `propose`, `change_with_gates` | maps to the approval mode |
| `m_system_prompt` | objects `m_prompt` | canonical prompt, replaces `bot.json.prompt` |
| `m_brief` | text | child brief as delivered at spawn |
| `m_model_profile` | objects `m_model_profile` | with `m_model_json` (text) as an inline override |
| `m_tool_profile`, `m_computer`, `m_connectors`, `m_subagents` | objects | catalog links |
| `m_messaging_groups` | objects `m_group` | |
| `m_memory_pages` | objects `m_memory` | |
| `m_skills_query`, `m_logs_query`, `m_agenda_query`, `m_suggestions_query`, `m_files_query` | objects (query) | created with the Bot |
| `m_primary`, `m_pinned`, `m_hidden`, `m_hide_from_sidebar`, `m_notifications` | checkbox | |
| `m_avatar` | files | also set as the object icon at create time |
| `m_active_device` | objects `m_device` | the Bot's home machine: the one machine that runs it |
| `m_handover_state` | select `active`, `draining`, `released` | handover state machine (plan, "Multiple machines") |
| `m_handover_target` | objects `m_device` | |
| `m_allowed_devices` | objects `m_device` | machines allowed to take it over (default: all) |

Stays on the machine, never synced, in `.bot/local.json`: `projectId`, `mainThreadId` (T3 binding), `lastSeenAt`, `workspaceRoot`, per-device computer override, routine to T3 task id map.

Body: a generated overview page. Short description, links to the prompt and memory, and embedded live views of its queries (see "Bot page" below).

## Catalog types

Reusable definitions the Bots link to, as in the org schema.

| Type | Layout, view | Purpose | Key properties | Maps to | Phase |
|---|---|---|---|---|---|
| `m_team` | basic, gallery | Group of Bots (sidebar section, HQ, pod) | `m_slug`, `m_kind` (select `section`, `hq`, `pod`), `m_lead` (objects bot), `m_mission`, `m_order` (number), `m_parent_team` | `settings.sections` | P7 |
| `m_subagent` | basic, table | A spawn route a parent Bot may start | `m_parent_bot`, `m_task_key`, `m_model_profile`, `m_scope`, `m_toolsets`, `m_deliver_back`, `m_intent`, `m_brief_template`, `m_system_prompt` | `subagent_create`, child `subagent` and `brief` | P7 |
| `m_model_profile` | basic, table | Reusable model stack | `m_slug`, `m_intent`, `m_instance_id` (text), `m_model` (text), `m_options_json`, `m_fallbacks_json`, `m_effort` (text), `m_notes` | `bot.json.model`, `defaultModel`, `avatarDrawingModel` | P4 |
| `m_harness` | basic, table | Runtime catalog entry | `m_slug` (`t3` today), `m_vendor`, `m_docs_url`, `m_capabilities_json` | the backend plan's `Harness` | P7 |
| `m_computer` | basic, table | Where a Bot's conversation runs | `m_slug`, `m_environment_id` (T3 link id), `m_host_device` (objects), `m_surface`, `m_scope`, `m_permission_mode` | `bot-execution-environment.md` | P7 |
| `m_tool_profile` | basic, table | Approval and tool policy | `m_slug`, `m_intent`, `m_approval_mode` (text), `m_rules_allow_json`, `m_rules_ask_json`, `m_hard_deny`, `m_browser_fill` (checkbox), `m_connectors` (objects), `m_timeout_seconds` | `rules`, `defaultRuntimeMode` | P7 |
| `m_connector` | basic, table | An MCP connector | `m_slug`, `m_transport` (select `http`, `stdio`), `m_url`, `m_command`, `m_args_json`, `m_auth_ref` (name of a secret kept on the device), `m_enabled`, `m_scope` (select `global`, `bot`), `m_owner_bot` | `.bot/connectors.json` | P4 |
| `m_vault_grant` | basic, table | Capability only: which Bots may use which saved login | `m_site_origins`, `m_vault_handle`, `m_bots` (objects), `m_team`, `m_lifecycle` | Vault, new policy layer | P7 |

## Knowledge and activity types

| Type | Layout, view | Purpose | Key properties | Maps to | Phase |
|---|---|---|---|---|---|
| `m_prompt` (page-backed) | basic, list | System prompt, welcome, shared rules, library, draft | `m_prompt_kind` (select `system`, `welcome`, `shared_rules`, `library`, `draft`, `brief`, `other`), `m_version`, `m_lifecycle`, `m_owner_bot`, `m_owner_group`, `m_audience` | `bot.json.prompt`, prompt part of `AGENTS.md`, `shared-memory.md` | P5 |
| `m_memory` (page-backed) | basic, list | One memory note or fact | `m_memory_kind` (select `fact`, `preference`, `profile`, `decision`, `lesson`, `resource`, `operating_note`, `playbook`, `exclusion`, `learning`, `other`), `m_memory_scope` (select `bot`, `team`, `org`, `human`), `m_topic`, `m_observed_at`, `m_valid_from`, `m_valid_to`, `m_source_bot`, `m_source_url`, `m_supersedes`, `m_derived_from`, `m_applies_to`, `m_confidence` (select `low`, `medium`, `high`), `m_last_reviewed`, `m_tombstone` (checkbox), `m_path` | `memory/*.md`, notes tree, shared memory; fits the memory-maintenance design | P5 |
| `m_skill` (page-backed) | basic, list | A skill's `SKILL.md` | `m_skill_key`, `m_skill_kind` (select `procedure`, `template`, `guide`), `m_skill_scope` (select `global`, `team`, `bot`), `m_skill_status` (select `draft`, `active`, `deprecated`, `archived`), `m_version`, `m_intent`, `m_owner_bot`, `m_parent_skill`, `m_related_skills`, `m_path` | `skills/<key>/SKILL.md`; other files stay `m_file` | P5 |
| `m_log` | basic, calendar on `m_logged_at` | One meaningful action | `m_owner_bot`, `m_logged_at` (date), `m_summary`, `m_log_kind` (select `activity`, `routine`, `maintenance`, `handoff`, `error`), `m_run_ref` (text), `m_notes` | `memory/log.md` entries, routine events | P5 |
| `m_suggestion` | basic, kanban by status | Improvement feedback | `m_suggestion_kind` (select `model_fit`, `new_subagent`, `new_skill`, `tool`, `memory`, `prompt`, `other`), `m_suggestion_status` (select `draft`, `proposed`, `accepted`, `done`, `declined`), `m_owner_bot`, `m_proposed_by`, `m_summary`, `m_related_objects` | new workflow | P7 |
| `m_schedule` | basic, calendar on `m_next_run` | A routine definition that follows the Bot between machines | `m_owner_bot`, `m_schedule_key`, `m_schedule_kind` (select `recurring`, `oneshot`, `event`), `m_schedule_expr`, `m_timezone`, `m_next_run` (date), `m_prompt_text` (text, exact), `m_thread_mode` (select `continue`, `new`), `m_model_pin`, `m_paused`, `m_paused_reason`, `m_lifecycle` | T3 `scheduledTasks.*` (T3 executes; the T3 task id stays in `.bot/local.json`) | P4 |
| `m_approval` | basic, table | Audit trail of an approval, question or secret request and its answer | `m_owner_bot`, `m_device`, `m_approval_kind` (select `command`, `file`, `network`, `secret`, `question`, `other`), `m_summary`, `m_decision` (select `approved`, `denied`, `answered`, `expired`), `m_decided_by`, `m_decided_at` (date), `m_request_ref` (text) | T3 `runtime-request`, `secrets.answerRequest` (history only) | P7 |
| `m_usage_day` | basic, calendar | Per device, per day counters, one object each | `m_device`, `m_day` (date), `m_counts_json` | `usage.json` rollup (the live counters stay local) | P7 |

## Conversation and records

| Type | Layout, view | Purpose | Key properties | Maps to | Phase |
|---|---|---|---|---|---|
| `m_group` | basic, gallery | A multi-Bot room (shared part) | `m_slug`, `m_members` (objects bot), `m_chief` (objects bot), `m_answer_policy` (select `default`, `last`, `everyone`, `chief`), `m_host_device` (objects), `m_mission`, `m_shared_notes`, `m_welcome_prompt`, `m_color`, `m_shape`, `m_parent_team`, `m_lifecycle` | shared fields of a room in `groups.json` | P4 |
| `m_msg_segment` | basic, table | Append-only slice of a room's messages | `m_group`, `m_device`, `m_seq` (number), `m_first_at`, `m_last_at`, `m_count`, `m_sealed` (checkbox), `m_content` (text, JSONL, exact) | `messages` of a room | P4 |
| `m_attachment` | basic, gallery | A sent or received file | `m_owner_bot`, `m_file_ref` (files), `m_name`, `m_mime`, `m_bytes`, `m_message_ref` | `.bot/attachments/*` | P4 |
| `m_project` | basic, kanban by status | Library record: project | common record properties plus `m_repos`, `m_lead` | `library.json` kind `project` | P4 |
| `m_note` | note, list | Library record: note | common | kind `note` | P4 |
| `m_task` | todo, kanban by status | Library record: task | common plus `m_project` (objects), `m_assignee` (objects bot) | kind `task` | P4 |
| `m_plan` | basic, list | Library record: plan | common plus `m_project` | kind `plan` | P4 |
| `m_link` | bookmark, gallery | Library record: link | common plus `m_url` | kind `link` | P4 |
| `m_repo` | basic, table | Library record: repo | common plus `m_remote_url`, `m_default_branch` | kind `repo` | P4 |
| `m_machine` | basic, table | Library record: machine | common plus `m_host`, `m_access` | kind `machine` | P4 |
| `m_service` | basic, table | Library record: service | common plus `m_endpoint`, `m_health` | kind `service` | P4 |

Common record properties: `m_owner_bot`, `m_status` (select, closed set per kind from `statuses()` in code), `m_due` (date), `m_tags`, `m_fields_json` (text, exact, for anything the types do not name). The eight types are generated from `RECORD_KINDS` and `statuses()`, so a new kind is one line plus its extra properties.

Built-in Anytype types (`task`, `project`, `note`, `bookmark`) were considered and rejected: the person may keep their own tasks in other spaces, status vocabularies differ, and a dedicated own type lets a generated query be exact. The layouts above give the same native experience.

## Templates

One template per type, applied by default when an object is created without a body. Examples:

- `m_bot`: overview page (see below).
- `m_memory`: `## Fact`, `## Why it matters`, `## Evidence`, `## Valid`.
- `m_skill`: Goal, When to use, Procedure, Verify, Gotchas.
- `m_prompt`: purpose, audience, text.
- `m_log`: Summary, Context, Outcome.
- `m_task`, `m_project`, `m_plan`: status, acceptance, notes.

A template must carry the type's properties in `type_settings` so a template-scoped search works (the API warns when a template sets a property its type does not list).

## Bot page (body of `m_bot`)

Generated once, then left to the person to arrange. Short description, links to the system prompt and group, a `property` block row for autonomy and home machine, and `dataview` blocks bound to the Bot's queries: Memory, Skills, Logs, Agenda, Suggestions, Files. Whether these block types can be authored through the API is spike item S7; if not, they are added by a template made once in the app.

## Queries, collections, views, widgets

Generated by `hbots anytype setup`. Query objects are created with views, and the minted view ids are stored in `m_meta.m_views_json`.

| Query | Type | Filter | Views |
|---|---|---|---|
| Live Bots | `m_bot` | `m_lifecycle IN ("live")` | gallery, table, graph |
| Per-Bot Memory, Skills, Logs, Agenda, Suggestions, Files | memory, skill, log, schedule, suggestion, file | `m_owner_bot IN ("<bot>")` | list; calendar for Logs and Agenda |
| Tasks board | `m_task` | `m_status != "done"` | kanban grouped by `m_status`, calendar on `m_due` |
| Projects board | `m_project` | none | kanban grouped by `m_status` |
| Conflicts | `m_conflict` | `m_conflict_state IN ("open")` | kanban by state |
| Devices | `m_device` | none | table with `m_last_seen` |
| Inbox | `m_suggestion` | `m_suggestion_status IN ("proposed")` | list |
| Recent changes | `m_memory`, `m_prompt`, `m_skill` | `last_modified_date > daysAgo(7)` | list |

Collections (hand curated, order preserved): "Pinned Bots", "Favorites", and the sidebar order of a team.

Widgets (space scope, written by an owner or admin key; personal scope otherwise): Live Bots (view layout), Groups, Tasks board, Conflicts, Devices, Inbox, Bin. Widgets are per person, so a missing widget never blocks storage.

## Mapping notes

- **Memory and the maintenance design.** `docs/memory-maintenance.md` keeps facts as markdown with `@valid` ranges. `m_valid_from` and `m_valid_to` carry the same range so Anytype can filter on it, while the disk rendering keeps the original syntax the harness reads.
- **Routines.** T3 executes. `m_schedule` is the definition. Each active device reconciles definitions into its own T3 scheduled tasks and keeps the T3 task id in `.bot/local.json`. Putting a per-device map on the shared object would make machines write one object at the same time.
- **Group messages.** One segment object per (room, device, sequence), so two machines never write the same object. Readers merge by timestamp and message id.
- **Group runtime state** (`threads`, `anchors`, `hops`, `lastSeenAt`) is per device and stays in `.botapp/groups.local.json`.
- **Secrets.** `m_connector.m_auth_ref` and similar fields name a secret kept on the device. The secret is never written to Anytype.
- **Chats and discussions are not used.** Michel creates none and ignores any a person adds. A person may still comment on an object in the app. Such an object can then only be archived, never purged, which the tombstone and Bin design already tolerates.
- **One Anytype identity for everything.** Bots are data objects, not Anytype participants. One account and one API key per machine serve every Bot and group.
- **Dates.** `date` properties link to Anytype date objects (backlinks per day). They read back as `YYYY-MM-DDT00:00:00Z`.
