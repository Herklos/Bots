# Anytype schema for Michel

Companion to [the Anytype backend plan](anytype-backend.md). Status: design only (2026-11-11, second revision after an audit against the Anytype API spec and live probes).

All types live in one dedicated, private Anytype space. Every type and property key starts with `hb_`, so nothing collides with the `aa_*` prototype schema or the person's `anytype-agents` org types. The shape is adapted from those org types (agent, team, subteam, subagent, harness profile, computer, tool profile, vault grant, prompt, memory, skill, log, suggestion, schedule, group, project, task) and mapped onto what Michel does.

## Design rules

These come from probes and the OpenAPI spec, not taste.

1. **Keys are normalized by Anytype and are permanent.** A requested type key `hbprobe2` came back as `hbprobe_2`, so digits are split off. Use no digits in keys (`hb_sha`, not `hb_sha256`), pass an explicit key on every create, and **always read the created key back** and keep the mapping in `hb_meta`. Never derive a key from a display name at runtime.
2. **Additive migrations only.** Deleting a type or property does not purge it: objects keep their values, the type stops being listable or filterable, and it can only be restored in the Anytype app. An object with a discussion cannot be deleted permanently at all. Never rename a key, add a new one and migrate.
3. **Select only for closed sets we own.** v2 cannot rename, recolor or delete select options. Anything that follows an outside system (T3 provider ids, model names, runtime modes) is `text`.
4. **Exact data goes in text properties.** A string is capped at 1 MiB and a structured request body at 1 MiB, so keep inline content at or under 256 KiB and spill larger content to a file. An object holds at most 128 properties and 2,048 blocks, a document body 10 MiB, a patch 512 operations, and one insert or replace 256 blocks.
5. **Page-backed types** (prompt, memory, skill, and the body of a few others) keep canonical content in body blocks so a person can edit them in Anytype. The gateway reads `anyblock` JSON and renders markdown itself. `format=md` is never used on a read path.
6. **Links are `objects` properties,** so backlinks and queries work. A type must list the bundled properties it wants queryable (`backlinks`, `links`, `tag`, `created_date`, `creator`, `last_modified_date`, `last_modified_by`); a type created with an explicit property list does not carry them by default.
7. **Property merge is per property and last writer wins.** Multi-property updates are not atomic across machines. Content must not depend on a sibling property to be valid (see the plan, "Concurrency model").
8. **Common properties** on most types: `hb_lifecycle` (select `draft`, `live`, `archived`), `hb_rev` (number, advisory write counter), `hb_updated` (number, unix ms, advisory), `hb_device` (text, last writer, advisory), `hb_notes` (text). Change detection uses `last_modified_date` plus the content itself, because a human edit in the Anytype app changes neither `hb_updated` nor `hb_rev`.
9. **Layouts do real work.** `profile` for Bots (avatar centered), `todo` for tasks (checkbox in the app), `note` for notes (no title row), `bookmark` for links (URL preview), `basic` for the rest, `set` and `collection` only for generated containers.
10. **Never stored:** passwords, tokens, bearer headers, the Vault key, session secrets, the T3 token, anything from `.mcp.json` or `.codex/config.toml`. Capability records (who may use which login) are allowed. Credentials are not.
11. **Per-device data is not in these types.** It lives in `hb_device` or stays on disk in `.botapp/` and `.bot/local.json`. The storage backend choice itself (backend, folder, Anytype host, key) is local to the machine in `~/.hbots/storage.json` and is never stored in Anytype, because the store cannot hold the setting that says where the store is.
12. **Every type ships with** a default template (body skeleton), a default view, an icon, and a generated query. Views are addressed by minted ids, so `hb_meta` stores the view ids per query.

## Core types

| Type | Layout, default view | Purpose | Key properties | Replaces or maps to | Phase |
|---|---|---|---|---|---|
| `hb_meta` | basic | Singleton. Schema version, key map, view ids, migration log, minimum gateway version | `hb_schema_version`, `hb_min_gateway`, `hb_keys_json`, `hb_views_json`, `hb_migrations_json` | none | P2 |
| `hb_device` | basic, table | One gateway on one machine | `hb_slug` (device id), `hb_hostname`, `hb_os`, `hb_gateway_version`, `hb_anytype_version`, `hb_last_seen` (number), `hb_enabled_bots` (objects), `hb_urls` (text, the gateway's reachable addresses), `hb_status`, `hb_stream_ok` (checkbox) | `~/.hbots/device-id` and new state | P3 |
| `hb_settings` | basic | Singleton. Shared preferences | `hb_user_name`, `hb_main_bot` (objects), `hb_default_model_profile`, `hb_avatar_model_profile` (objects), `hb_auto_review` (checkbox), `hb_default_runtime_mode` (text), `hb_group_answer_policy` (select `last`, `everyone`, `chief`), `hb_rules_allow_json`, `hb_rules_ask_json`, `hb_sections_json`, `hb_beta_json` | shared part of `settings.json`, `main-bot.json`, `apps.json` | P4 |
| `hb_bot` | profile, gallery | A Bot. The central object | see below | shared fields of `.bot/bot.json` | P4 |
| `hb_file` | basic, table | A synced file, byte exact | `hb_path`, `hb_dir`, `hb_content`, `hb_blob` (files), `hb_sha`, `hb_size`, `hb_mime`, `hb_mtime`, `hb_deleted` (checkbox), `hb_owner_bot` | everything through `Workspace` | P3 |
| `hb_conflict` | basic, kanban by state | An unresolved disagreement | `hb_path`, `hb_owner_bot`, `hb_conflict_state` (select `open`, `resolved`, `dismissed`), `hb_device_a`, `hb_device_b`, `hb_sha_a`, `hb_sha_b`, `hb_copy_a`, `hb_copy_b` (files), `hb_resolution` (text), `hb_detected_at` (date) | `.conflict-` files, now first class and visible in Anytype | P3 |

### `hb_bot`

| Property | Format | Note |
|---|---|---|
| `hb_bot_id` | text | stable uuid. Identity. The slug is a label that can collide across machines (see plan) |
| `hb_slug` | text | folder name; unique by rule, not by constraint |
| `name` (object name) | | display name |
| `hb_label`, `hb_shape`, `hb_color` | text | face drawing, values follow the client |
| `hb_role` | select `bot`, `child` | |
| `hb_parent` | objects `hb_bot` | child Bots (`parentSlug`) |
| `hb_team` | objects `hb_team` | sidebar section or org grouping |
| `hb_short_description` | text | one line mission |
| `hb_lifecycle` | select | `draft`, `live`, `archived` |
| `hb_autonomy` | select `read_only`, `propose`, `change_with_gates` | maps to the approval mode |
| `hb_system_prompt` | objects `hb_prompt` | canonical prompt, replaces `bot.json.prompt` |
| `hb_brief` | text | child brief as delivered at spawn |
| `hb_model_profile` | objects `hb_model_profile` | with `hb_model_json` (text) as an inline override |
| `hb_tool_profile`, `hb_computer`, `hb_connectors`, `hb_subagents` | objects | catalog links |
| `hb_messaging_groups` | objects `hb_group` | |
| `hb_memory_pages` | objects `hb_memory` | |
| `hb_skills_query`, `hb_logs_query`, `hb_agenda_query`, `hb_suggestions_query`, `hb_files_query` | objects (query) | created with the Bot |
| `hb_primary`, `hb_pinned`, `hb_hidden`, `hb_hide_from_sidebar`, `hb_notifications` | checkbox | |
| `hb_avatar` | files | also set as the object icon at create time |
| `hb_active_device` | objects `hb_device` | the Bot's home machine: the one machine that runs it |
| `hb_handover_state` | select `active`, `draining`, `released` | handover state machine (plan, "Multiple machines") |
| `hb_handover_target` | objects `hb_device` | |
| `hb_allowed_devices` | objects `hb_device` | machines allowed to take it over (default: all) |

Stays on the machine, never synced, in `.bot/local.json`: `projectId`, `mainThreadId` (T3 binding), `lastSeenAt`, `workspaceRoot`, per-device computer override, routine to T3 task id map.

Body: a generated overview page. Short description, links to the prompt and memory, and embedded live views of its queries (see "Bot page" below).

## Catalog types

Reusable definitions the Bots link to, as in the org schema.

| Type | Layout, view | Purpose | Key properties | Maps to | Phase |
|---|---|---|---|---|---|
| `hb_team` | basic, gallery | Group of Bots (sidebar section, HQ, pod) | `hb_slug`, `hb_kind` (select `section`, `hq`, `pod`), `hb_lead` (objects bot), `hb_mission`, `hb_order` (number), `hb_parent_team` | `settings.sections` | P7 |
| `hb_subagent` | basic, table | A spawn route a parent Bot may start | `hb_parent_bot`, `hb_task_key`, `hb_model_profile`, `hb_scope`, `hb_toolsets`, `hb_deliver_back`, `hb_intent`, `hb_brief_template`, `hb_system_prompt` | `subagent_create`, child `subagent` and `brief` | P7 |
| `hb_model_profile` | basic, table | Reusable model stack | `hb_slug`, `hb_intent`, `hb_instance_id` (text), `hb_model` (text), `hb_options_json`, `hb_fallbacks_json`, `hb_effort` (text), `hb_notes` | `bot.json.model`, `defaultModel`, `avatarDrawingModel` | P4 |
| `hb_harness` | basic, table | Runtime catalog entry | `hb_slug` (`t3` today), `hb_vendor`, `hb_docs_url`, `hb_capabilities_json` | the backend plan's `Harness` | P7 |
| `hb_computer` | basic, table | Where a Bot's conversation runs | `hb_slug`, `hb_environment_id` (T3 link id), `hb_host_device` (objects), `hb_surface`, `hb_scope`, `hb_permission_mode` | `bot-execution-environment.md` | P7 |
| `hb_tool_profile` | basic, table | Approval and tool policy | `hb_slug`, `hb_intent`, `hb_approval_mode` (text), `hb_rules_allow_json`, `hb_rules_ask_json`, `hb_hard_deny`, `hb_browser_fill` (checkbox), `hb_connectors` (objects), `hb_timeout_seconds` | `rules`, `defaultRuntimeMode` | P7 |
| `hb_connector` | basic, table | An MCP connector | `hb_slug`, `hb_transport` (select `http`, `stdio`), `hb_url`, `hb_command`, `hb_args_json`, `hb_auth_ref` (name of a secret kept on the device), `hb_enabled`, `hb_scope` (select `global`, `bot`), `hb_owner_bot` | `.bot/connectors.json` | P4 |
| `hb_vault_grant` | basic, table | Capability only: which Bots may use which saved login | `hb_site_origins`, `hb_vault_handle`, `hb_bots` (objects), `hb_team`, `hb_lifecycle` | Vault, new policy layer | P7 |

## Knowledge and activity types

| Type | Layout, view | Purpose | Key properties | Maps to | Phase |
|---|---|---|---|---|---|
| `hb_prompt` (page-backed) | basic, list | System prompt, welcome, shared rules, library, draft | `hb_prompt_kind` (select `system`, `welcome`, `shared_rules`, `library`, `draft`, `brief`, `other`), `hb_version`, `hb_lifecycle`, `hb_owner_bot`, `hb_owner_group`, `hb_audience` | `bot.json.prompt`, prompt part of `AGENTS.md`, `shared-memory.md` | P5 |
| `hb_memory` (page-backed) | basic, list | One memory note or fact | `hb_memory_kind` (select `fact`, `preference`, `profile`, `decision`, `lesson`, `resource`, `operating_note`, `playbook`, `exclusion`, `learning`, `other`), `hb_memory_scope` (select `bot`, `team`, `org`, `human`), `hb_topic`, `hb_observed_at`, `hb_valid_from`, `hb_valid_to`, `hb_source_bot`, `hb_source_url`, `hb_supersedes`, `hb_derived_from`, `hb_applies_to`, `hb_confidence` (select `low`, `medium`, `high`), `hb_last_reviewed`, `hb_tombstone` (checkbox), `hb_path` | `memory/*.md`, notes tree, shared memory; fits the memory-maintenance design | P5 |
| `hb_skill` (page-backed) | basic, list | A skill's `SKILL.md` | `hb_skill_key`, `hb_skill_kind` (select `procedure`, `template`, `guide`), `hb_skill_scope` (select `global`, `team`, `bot`), `hb_skill_status` (select `draft`, `active`, `deprecated`, `archived`), `hb_version`, `hb_intent`, `hb_owner_bot`, `hb_parent_skill`, `hb_related_skills`, `hb_path` | `skills/<key>/SKILL.md`; other files stay `hb_file` | P5 |
| `hb_log` | basic, calendar on `hb_logged_at` | One meaningful action | `hb_owner_bot`, `hb_logged_at` (date), `hb_summary`, `hb_log_kind` (select `activity`, `routine`, `maintenance`, `handoff`, `error`), `hb_run_ref` (text), `hb_notes` | `memory/log.md` entries, routine events | P5 |
| `hb_suggestion` | basic, kanban by status | Improvement feedback | `hb_suggestion_kind` (select `model_fit`, `new_subagent`, `new_skill`, `tool`, `memory`, `prompt`, `other`), `hb_suggestion_status` (select `draft`, `proposed`, `accepted`, `done`, `declined`), `hb_owner_bot`, `hb_proposed_by`, `hb_summary`, `hb_related_objects` | new workflow | P7 |
| `hb_schedule` | basic, calendar on `hb_next_run` | A routine definition that follows the Bot between machines | `hb_owner_bot`, `hb_schedule_key`, `hb_schedule_kind` (select `recurring`, `oneshot`, `event`), `hb_schedule_expr`, `hb_timezone`, `hb_next_run` (date), `hb_prompt_text` (text, exact), `hb_thread_mode` (select `continue`, `new`), `hb_model_pin`, `hb_paused`, `hb_paused_reason`, `hb_lifecycle` | T3 `scheduledTasks.*` (T3 executes; the T3 task id stays in `.bot/local.json`) | P4 |
| `hb_approval` | basic, table | Audit trail of an approval, question or secret request and its answer | `hb_owner_bot`, `hb_device`, `hb_approval_kind` (select `command`, `file`, `network`, `secret`, `question`, `other`), `hb_summary`, `hb_decision` (select `approved`, `denied`, `answered`, `expired`), `hb_decided_by`, `hb_decided_at` (date), `hb_request_ref` (text) | T3 `runtime-request`, `secrets.answerRequest` (history only) | P7 |
| `hb_usage_day` | basic, calendar | Per device, per day counters, one object each | `hb_device`, `hb_day` (date), `hb_counts_json` | `usage.json` rollup (the live counters stay local) | P7 |

## Conversation and records

| Type | Layout, view | Purpose | Key properties | Maps to | Phase |
|---|---|---|---|---|---|
| `hb_group` | basic, gallery | A multi-Bot room (shared part) | `hb_slug`, `hb_members` (objects bot), `hb_chief` (objects bot), `hb_answer_policy` (select `default`, `last`, `everyone`, `chief`), `hb_host_device` (objects), `hb_mission`, `hb_shared_notes`, `hb_welcome_prompt`, `hb_color`, `hb_shape`, `hb_parent_team`, `hb_lifecycle` | shared fields of a room in `groups.json` | P4 |
| `hb_msg_segment` | basic, table | Append-only slice of a room's messages | `hb_group`, `hb_device`, `hb_seq` (number), `hb_first_at`, `hb_last_at`, `hb_count`, `hb_sealed` (checkbox), `hb_content` (text, JSONL, exact) | `messages` of a room | P4 |
| `hb_attachment` | basic, gallery | A sent or received file | `hb_owner_bot`, `hb_file_ref` (files), `hb_name`, `hb_mime`, `hb_bytes`, `hb_message_ref` | `.bot/attachments/*` | P4 |
| `hb_project` | basic, kanban by status | Library record: project | common record properties plus `hb_repos`, `hb_lead` | `library.json` kind `project` | P4 |
| `hb_note` | note, list | Library record: note | common | kind `note` | P4 |
| `hb_task` | todo, kanban by status | Library record: task | common plus `hb_project` (objects), `hb_assignee` (objects bot) | kind `task` | P4 |
| `hb_plan` | basic, list | Library record: plan | common plus `hb_project` | kind `plan` | P4 |
| `hb_link` | bookmark, gallery | Library record: link | common plus `hb_url` | kind `link` | P4 |
| `hb_repo` | basic, table | Library record: repo | common plus `hb_remote_url`, `hb_default_branch` | kind `repo` | P4 |
| `hb_machine` | basic, table | Library record: machine | common plus `hb_host`, `hb_access` | kind `machine` | P4 |
| `hb_service` | basic, table | Library record: service | common plus `hb_endpoint`, `hb_health` | kind `service` | P4 |

Common record properties: `hb_owner_bot`, `hb_status` (select, closed set per kind from `statuses()` in code), `hb_due` (date), `hb_tags`, `hb_fields_json` (text, exact, for anything the types do not name). The eight types are generated from `RECORD_KINDS` and `statuses()`, so a new kind is one line plus its extra properties.

Built-in Anytype types (`task`, `project`, `note`, `bookmark`) were considered and rejected: the person may keep their own tasks in other spaces, status vocabularies differ, and a dedicated own type lets a generated query be exact. The layouts above give the same native experience.

## Templates

One template per type, applied by default when an object is created without a body. Examples:

- `hb_bot`: overview page (see below).
- `hb_memory`: `## Fact`, `## Why it matters`, `## Evidence`, `## Valid`.
- `hb_skill`: Goal, When to use, Procedure, Verify, Gotchas.
- `hb_prompt`: purpose, audience, text.
- `hb_log`: Summary, Context, Outcome.
- `hb_task`, `hb_project`, `hb_plan`: status, acceptance, notes.

A template must carry the type's properties in `type_settings` so a template-scoped search works (the API warns when a template sets a property its type does not list).

## Bot page (body of `hb_bot`)

Generated once, then left to the person to arrange. Short description, links to the system prompt and group, a `property` block row for autonomy and home machine, and `dataview` blocks bound to the Bot's queries: Memory, Skills, Logs, Agenda, Suggestions, Files. Whether these block types can be authored through the API is spike item S7; if not, they are added by a template made once in the app.

## Queries, collections, views, widgets

Generated by `hbots anytype setup`. Query objects are created with views, and the minted view ids are stored in `hb_meta.hb_views_json`.

| Query | Type | Filter | Views |
|---|---|---|---|
| Live Bots | `hb_bot` | `hb_lifecycle IN ("live")` | gallery, table, graph |
| Per-Bot Memory, Skills, Logs, Agenda, Suggestions, Files | memory, skill, log, schedule, suggestion, file | `hb_owner_bot IN ("<bot>")` | list; calendar for Logs and Agenda |
| Tasks board | `hb_task` | `hb_status != "done"` | kanban grouped by `hb_status`, calendar on `hb_due` |
| Projects board | `hb_project` | none | kanban grouped by `hb_status` |
| Conflicts | `hb_conflict` | `hb_conflict_state IN ("open")` | kanban by state |
| Devices | `hb_device` | none | table with `hb_last_seen` |
| Inbox | `hb_suggestion` | `hb_suggestion_status IN ("proposed")` | list |
| Recent changes | `hb_memory`, `hb_prompt`, `hb_skill` | `last_modified_date > daysAgo(7)` | list |

Collections (hand curated, order preserved): "Pinned Bots", "Favorites", and the sidebar order of a team.

Widgets (space scope, written by an owner or admin key; personal scope otherwise): Live Bots (view layout), Groups, Tasks board, Conflicts, Devices, Inbox, Bin. Widgets are per person, so a missing widget never blocks storage.

## Mapping notes

- **Memory and the maintenance design.** `docs/memory-maintenance.md` keeps facts as markdown with `@valid` ranges. `hb_valid_from` and `hb_valid_to` carry the same range so Anytype can filter on it, while the disk rendering keeps the original syntax the harness reads.
- **Routines.** T3 executes. `hb_schedule` is the definition. Each active device reconciles definitions into its own T3 scheduled tasks and keeps the T3 task id in `.bot/local.json`. Putting a per-device map on the shared object would make machines write one object at the same time.
- **Group messages.** One segment object per (room, device, sequence), so two machines never write the same object. Readers merge by timestamp and message id.
- **Group runtime state** (`threads`, `anchors`, `hops`, `lastSeenAt`) is per device and stays in `.botapp/groups.local.json`.
- **Secrets.** `hb_connector.hb_auth_ref` and similar fields name a secret kept on the device. The secret is never written to Anytype.
- **Chats and discussions are not used.** Michel creates none and ignores any a person adds. A person may still comment on an object in the app. Such an object can then only be archived, never purged, which the tombstone and Bin design already tolerates.
- **One Anytype identity for everything.** Bots are data objects, not Anytype participants. One account and one API key per machine serve every Bot and group.
- **Dates.** `date` properties link to Anytype date objects (backlinks per day). They read back as `YYYY-MM-DDT00:00:00Z`.
