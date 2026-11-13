//! P5 declarations added to Prepare for schema v3: HK org sync targets
//! ([anytype-hk-sync-plan.md](docs/anytype-hk-sync-plan.md)). Additive only —
//! existing v2 property keys keep their formats, and new fields on existing
//! types are attached by Prepare's add_property path. A new `m_` column is
//! never a reinterpretation of a v1 text key.
//!
//! Cross-space identity: every imported object records where it came from
//! (`m_source_id` and `m_source_space`) and keeps its unmapped source fields in
//! `m_source_json`, so an import is lossless even without a typed destination.
use super::table::{Column, Table};

const fn column(key: &'static str, format: &'static str) -> Column {
    Column {
        key,
        name: key,
        format,
        options: &[],
    }
}
const fn select(key: &'static str, options: &'static [&'static str]) -> Column {
    Column {
        key,
        name: key,
        format: "select",
        options,
    }
}
const fn objects(key: &'static str) -> Column {
    Column {
        key,
        name: key,
        format: "objects",
        options: &[],
    }
}
const fn table(
    key: &'static str,
    layout: &'static str,
    properties: &'static [&'static str],
) -> Table {
    Table {
        key,
        name: key,
        layout,
        properties,
    }
}

pub(crate) const P5_COLUMNS: &[Column] = &[
    // Cross-space source identity.
    column("m_source_id", "text"),
    column("m_source_space", "text"),
    column("m_source_json", "text"),
    // m_bot extensions (HK agent fields with no v2 destination).
    column("m_browser", "text"),
    column("m_board_member", "checkbox"),
    column("m_rules_version", "text"),
    column("m_delegation_enabled", "checkbox"),
    column("m_delegation_backend", "text"),
    column("m_profile_routes", "text"),
    column("m_terminal_backend", "text"),
    column("m_computer_surface", "text"),
    column("m_voice_json", "text"),
    objects("m_runtime"),
    objects("m_projects"),
    objects("m_default_project"),
    select("m_exec_policy", &["local", "cloud", "mixed", "unset"]),
    // m_group extensions.
    column("m_platform", "text"),
    column("m_platform_chat_id", "text"),
    column("m_profile_slugs", "text"),
    objects("m_welcome_prompt_obj"),
    column("m_routing_summary", "text"),
    // m_schedule extensions.
    column("m_deliver", "text"),
    column("m_skills", "text"),
    column("m_workdir", "text"),
    column("m_no_agent", "checkbox"),
    column("m_script_path", "text"),
    // m_model_profile extensions.
    column("m_provider", "text"),
    column("m_harness_name", "text"),
    column("m_aux_json", "text"),
    // m_team extensions (the team type itself is new in this prepare).
    objects("m_standing_instructions"),
    select("m_team_kind", &["section", "board", "hq", "pod"]),
    column("m_order", "number"),
    // m_project extensions.
    objects("m_owning_team"),
    column("m_category", "text"),
    // m_task / epic / user story extensions.
    select(
        "m_kind",
        &["bugfix", "implementation", "doc", "design", "review", "chore", "spike"],
    ),
    select("m_priority", &["P0", "P1", "P2", "P3"]),
    select(
        "m_pm_status",
        &["backlog", "ready", "doing", "done", "cancelled"],
    ),
    objects("m_linked_projects"),
    objects("m_epic"),
    objects("m_user_story"),
    objects("m_user_stories"),
    objects("m_tasks"),
    objects("m_epics"),
    // m_prompt.
    select(
        "m_prompt_kind",
        &["system", "welcome", "shared_rules", "library", "draft", "brief", "other"],
    ),
    column("m_version", "text"),
    select("m_audience", &["agent", "group", "org", "library"]),
    objects("m_owner_group"),
    // m_subagent.
    column("m_task_key", "text"),
    column("m_toolsets", "text"),
    select("m_spawn_effort", &["low", "medium", "high"]),
    column("m_model_override", "text"),
    column("m_deliver_back", "text"),
    column("m_brief_template", "text"),
    objects("m_parent_bot"),
    // m_memory.
    select(
        "m_memory_kind",
        &[
            "fact", "preference", "profile", "decision", "lesson", "resource", "operating_note",
            "playbook", "exclusion", "learning", "other",
        ],
    ),
    select("m_memory_scope", &["bot", "team", "org", "human"]),
    column("m_topic", "text"),
    column("m_observed_at", "date"),
    column("m_valid_from", "date"),
    column("m_valid_to", "date"),
    column("m_source_url", "text"),
    select("m_confidence", &["low", "medium", "high"]),
    column("m_last_reviewed", "date"),
    column("m_tombstone", "checkbox"),
    objects("m_applies_to"),
    objects("m_supersedes"),
    objects("m_derived_from"),
    // m_skill.
    column("m_skill_key", "text"),
    select("m_skill_kind", &["procedure", "template", "guide"]),
    select("m_skill_scope", &["global", "team", "bot"]),
    select(
        "m_skill_status",
        &["draft", "active", "deprecated", "archived"],
    ),
    objects("m_parent_skill"),
    objects("m_related_skills"),
    // m_harness.
    column("m_harness_kind", "text"),
    column("m_vendor", "text"),
    column("m_docs_url", "text"),
    column("m_capabilities_json", "text"),
    // m_log.
    column("m_logged_at", "date"),
    select(
        "m_log_kind",
        &["activity", "routine", "maintenance", "handoff", "error"],
    ),
    column("m_run_ref", "text"),
    // m_suggestion.
    select(
        "m_suggestion_kind",
        &[
            "model_fit", "new_subagent", "new_skill", "harness", "tool", "memory", "prompt",
            "other",
        ],
    ),
    select(
        "m_suggestion_status",
        &["draft", "proposed", "accepted", "done", "declined"],
    ),
    column("m_proposed_by", "text"),
    column("m_proposed_at", "date"),
    objects("m_related"),
];

/// Added fields on types Prepare v3 may have already created. `properties()`
/// chains these after the type's own list, and Prepare attaches any that are
/// missing through add_property ops.
pub(crate) const EXTENSIONS: &[(&str, &[&str])] = &[
    (
        "m_bot",
        &[
            "m_source_id",
            "m_source_space",
            "m_source_json",
            "m_browser",
            "m_board_member",
            "m_rules_version",
            "m_delegation_enabled",
            "m_delegation_backend",
            "m_profile_routes",
            "m_terminal_backend",
            "m_computer_surface",
            "m_voice_json",
            "m_runtime",
            "m_projects",
            "m_default_project",
            "m_exec_policy",
        ],
    ),
    (
        "m_group",
        &[
            "m_source_id",
            "m_source_space",
            "m_source_json",
            "m_platform",
            "m_platform_chat_id",
            "m_profile_slugs",
            "m_welcome_prompt_obj",
            "m_routing_summary",
        ],
    ),
    (
        "m_schedule",
        &[
            "m_source_id",
            "m_source_space",
            "m_source_json",
            "m_deliver",
            "m_skills",
            "m_workdir",
            "m_no_agent",
            "m_script_path",
        ],
    ),
    (
        "m_model_profile",
        &[
            "m_source_id",
            "m_source_space",
            "m_source_json",
            "m_provider",
            "m_harness_name",
            "m_aux_json",
            "m_voice_json",
        ],
    ),
    (
        "m_team",
        &[
            "m_source_id",
            "m_source_space",
            "m_source_json",
            "m_routing_summary",
            "m_standing_instructions",
        ],
    ),
    (
        "m_project",
        &[
            "m_source_id",
            "m_source_space",
            "m_source_json",
            "m_owning_team",
            "m_exec_policy",
            "m_category",
        ],
    ),
    (
        "m_task",
        &[
            "m_source_id",
            "m_source_space",
            "m_source_json",
            "m_kind",
            "m_priority",
            "m_epic",
            "m_user_story",
        ],
    ),
];

const BOT_PROMPT: &[&str] = &[
    "m_prompt_kind",
    "m_version",
    "m_audience",
    "m_owner_bot_ref",
    "m_owner_group",
    "m_source_id",
    "m_source_space",
    "m_source_json",
    "m_lifecycle",
    "m_rev",
    "m_updated",
    "m_device",
    "m_notes",
];

const SUBAGENT: &[&str] = &[
    "m_parent_bot",
    "m_task_key",
    "m_intent",
    "m_scope",
    "m_toolsets",
    "m_spawn_effort",
    "m_model_override",
    "m_deliver_back",
    "m_brief_template",
    "m_system_prompt",
    "m_source_id",
    "m_source_space",
    "m_source_json",
    "m_lifecycle",
    "m_rev",
    "m_updated",
    "m_device",
    "m_notes",
];

const MEMORY: &[&str] = &[
    "m_owner_bot_ref",
    "m_memory_kind",
    "m_memory_scope",
    "m_topic",
    "m_observed_at",
    "m_valid_from",
    "m_valid_to",
    "m_source_url",
    "m_confidence",
    "m_last_reviewed",
    "m_tombstone",
    "m_applies_to",
    "m_supersedes",
    "m_derived_from",
    "m_path",
    "m_source_id",
    "m_source_space",
    "m_source_json",
    "m_lifecycle",
    "m_rev",
    "m_updated",
    "m_notes",
];

const SKILL: &[&str] = &[
    "m_owner_bot_ref",
    "m_skill_key",
    "m_skill_kind",
    "m_skill_scope",
    "m_skill_status",
    "m_version",
    "m_intent",
    "m_parent_skill",
    "m_related_skills",
    "m_path",
    "m_source_id",
    "m_source_space",
    "m_source_json",
    "m_lifecycle",
    "m_rev",
    "m_updated",
    "m_device",
    "m_notes",
];

const HARNESS: &[&str] = &[
    "m_slug",
    "m_harness_kind",
    "m_vendor",
    "m_docs_url",
    "m_capabilities_json",
    "m_source_id",
    "m_source_space",
    "m_source_json",
    "m_lifecycle",
    "m_rev",
    "m_updated",
    "m_notes",
];

const LOG: &[&str] = &[
    "m_owner_bot_ref",
    "m_logged_at",
    "m_summary",
    "m_log_kind",
    "m_run_ref",
    "m_source_id",
    "m_source_space",
    "m_source_json",
    "m_lifecycle",
    "m_notes",
];

const SUGGESTION: &[&str] = &[
    "m_owner_bot_ref",
    "m_suggestion_kind",
    "m_suggestion_status",
    "m_proposed_by",
    "m_proposed_at",
    "m_summary",
    "m_related",
    "m_source_id",
    "m_source_space",
    "m_source_json",
    "m_lifecycle",
    "m_notes",
];

const EPIC: &[&str] = &[
    "m_owner_bot_ref",
    "m_record_status",
    "m_due",
    "m_tags",
    "m_fields_json",
    "m_lifecycle",
    "m_rev",
    "m_updated",
    "m_device",
    "m_notes",
    "m_pm_status",
    "m_priority",
    "m_linked_projects",
    "m_user_stories",
];

const USER_STORY: &[&str] = &[
    "m_owner_bot_ref",
    "m_record_status",
    "m_due",
    "m_tags",
    "m_fields_json",
    "m_lifecycle",
    "m_rev",
    "m_updated",
    "m_device",
    "m_notes",
    "m_pm_status",
    "m_priority",
    "m_linked_projects",
    "m_epic",
    "m_tasks",
];

const TEAM: &[&str] = &[
    "m_slug",
    "m_team_kind",
    "m_mission",
    "m_lead",
    "m_members",
    "m_order",
    "m_parent_team",
    "m_routing_summary",
    "m_standing_instructions",
    "m_source_id",
    "m_source_space",
    "m_source_json",
    "m_lifecycle",
    "m_rev",
    "m_updated",
    "m_device",
    "m_notes",
];

pub(crate) const P5_TABLES: &[Table] = &[
    table("m_team", "basic", TEAM),
    table("m_prompt", "basic", BOT_PROMPT),
    table("m_subagent", "basic", SUBAGENT),
    table("m_memory", "basic", MEMORY),
    table("m_skill", "basic", SKILL),
    table("m_harness", "basic", HARNESS),
    table("m_log", "basic", LOG),
    table("m_suggestion", "basic", SUGGESTION),
    table("m_epic", "todo", EPIC),
    table("m_user_story", "todo", USER_STORY),
];

/// Extra properties the generated record kinds validate per kind.
pub(crate) fn extensions(table_key: &str) -> &'static [&'static str] {
    EXTENSIONS
        .iter()
        .find(|(key, _)| *key == table_key)
        .map(|(_, properties)| *properties)
        .unwrap_or(&[])
}
