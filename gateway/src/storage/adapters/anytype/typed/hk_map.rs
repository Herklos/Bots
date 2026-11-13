//! The HK → Bots field mapping table for the import (see hk_import.rs).

use super::hk::{AGENT_FIELDS, Dest};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Fmt {
    Text,
    Bool,
    Json,
    /// An objects property; resolved in pass two from the register.
    Link,
}

pub(crate) struct Rule {
    pub hk: &'static str,
    pub prop: &'static str,
    pub fmt: Fmt,
}

const fn text(hk: &'static str, prop: &'static str) -> Rule {
    Rule { hk, prop, fmt: Fmt::Text }
}
const fn link(hk: &'static str, prop: &'static str) -> Rule {
    Rule { hk, prop, fmt: Fmt::Link }
}
const fn json_rule(hk: &'static str, prop: &'static str) -> Rule {
    Rule { hk, prop, fmt: Fmt::Json }
}

/// The scalar rules for one HK type. Agent rules come from `AGENT_FIELDS`
/// (see [agent_rules]); link properties are excluded there and resolved in
/// pass two.
pub(crate) fn rules(hk_type: &str) -> Vec<Rule> {
    match hk_type {
        "team" | "subteam" => vec![
            text("slug", "m_slug"),
            text("mission", "m_mission"),
            text("routing_summary", "m_routing_summary"),
            text("kind", "m_team_kind"),
            link("members", "m_members"),
            link("chief", "m_lead"),
            link("parent_team", "m_parent_team"),
        ],
        "subagent" => vec![
            text("task_key", "m_task_key"),
            text("intent", "m_intent"),
            text("scope", "m_scope"),
            text("toolsets", "m_toolsets"),
            text("spawn_effort", "m_spawn_effort"),
            text("model_override", "m_model_override"),
            text("deliver_back", "m_deliver_back"),
            text("brief_template", "m_brief_template"),
            link("parent_agent", "m_parent_bot"),
        ],
        "harness" | "runtime" => vec![
            text("slug", "m_slug"),
            text("kind", "m_harness_kind"),
            text("vendor", "m_vendor"),
            text("docs_url", "m_docs_url"),
        ],
        "harness_profile" => vec![
            text("intent", "m_intent"),
            text("model_provider", "m_provider"),
            text("model_default", "m_model"),
            text("model_effort", "m_effort"),
            text("model_fallbacks", "m_fallbacks_json"),
            text("model_options", "m_options_json"),
            text("aux_models", "m_aux_json"),
        ],
        "prompt" => vec![
            text("prompt_kind", "m_prompt_kind"),
            text("version", "m_version"),
            text("audience", "m_audience"),
            text("target", "m_notes"),
            link("owner_agent", "m_owner_bot_ref"),
            link("owner_group", "m_owner_group"),
        ],
        "memory" => vec![
            text("memory_kind", "m_memory_kind"),
            text("memory_scope", "m_memory_scope"),
            text("topic", "m_topic"),
            link("owner_agent", "m_owner_bot_ref"),
        ],
        "skill" => vec![
            text("skill_key", "m_skill_key"),
            text("skill_kind", "m_skill_kind"),
            text("skill_scope", "m_skill_scope"),
            text("skill_status", "m_skill_status"),
            text("version", "m_version"),
            text("intent", "m_intent"),
            link("owner_agent", "m_owner_bot_ref"),
        ],
        "log" => vec![
            text("summary", "m_summary"),
            text("logged_at", "m_logged_at"),
            text("kind", "m_log_kind"),
            link("owner_agent", "m_owner_bot_ref"),
        ],
        "suggestion" => vec![
            text("suggestion_kind", "m_suggestion_kind"),
            text("suggestion_status", "m_suggestion_status"),
            text("proposed_by", "m_proposed_by"),
            text("summary", "m_summary"),
            link("owner_agent", "m_owner_bot_ref"),
            link("related", "m_related"),
        ],
        "schedule" => vec![
            text("schedule_key", "m_schedule_key"),
            text("schedule_kind", "m_kind"),
            text("schedule_expr", "m_schedule_expr"),
            text("timezone", "m_timezone"),
            text("intent", "m_intent"),
            text("deliver", "m_deliver"),
            text("skills", "m_skills"),
            text("workdir", "m_workdir"),
            text("script_path", "m_script_path"),
        ],
        "group" => vec![
            text("slug", "m_slug"),
            text("mission", "m_mission"),
            text("platform", "m_platform"),
            text("platform_chat_id", "m_platform_chat_id"),
            text("hermes_profiles", "m_profile_slugs"),
            text("routing_summary", "m_routing_summary"),
            link("members", "m_members"),
            link("welcome_prompt", "m_welcome_prompt_obj"),
        ],
        "project" => vec![
            text("one_liner", "m_short_description"),
            json_rule("overview", "m_fields_json"),
            text("execution_policy", "m_exec_policy"),
            text("project_category", "m_category"),
            link("owning_team", "m_owning_team"),
        ],
        "epic" => vec![
            text("pm_priority", "m_priority"),
            text("status", "m_pm_status"),
            link("owner_agent", "m_owner_bot_ref"),
            link("linked_projects", "m_linked_projects"),
        ],
        "user_story" => vec![
            text("pm_priority", "m_priority"),
            text("status", "m_pm_status"),
            link("owner_agent", "m_owner_bot_ref"),
            link("parent_epic", "m_epic"),
            link("linked_projects", "m_linked_projects"),
        ],
        "task" => vec![
            text("kind", "m_kind"),
            text("pm_priority", "m_priority"),
            text("status", "m_pm_status"),
            link("owner_agent", "m_owner_bot_ref"),
            link("linked_projects", "m_linked_projects"),
            link("parent_epic", "m_epic"),
            link("assignee", "m_assignee"),
        ],
        _ => Vec::new(),
    }
}

/// Agent rules from `AGENT_FIELDS`: Property destinations become scalar
/// rules, except the objects properties pass two resolves.
const AGENT_LINK_PROPS: &[&str] = &[
    "m_team",
    "m_parent",
    "m_system_prompt",
    "m_model_profile",
    "m_subagents",
    "m_messaging_groups",
    "m_memory_pages",
    "m_runtime",
];

pub(crate) fn agent_rules() -> Vec<Rule> {
    let mut list = Vec::new();
    for field in AGENT_FIELDS {
        if let Dest::Property(prop) = field.dest {
            if prop.starts_with("m_") && !AGENT_LINK_PROPS.contains(&prop) {
                let fmt = if prop == "m_delegation_enabled" || prop == "m_board_member" {
                    Fmt::Bool
                } else {
                    Fmt::Text
                };
                list.push(Rule { hk: field.hk, prop, fmt });
            }
        }
    }
    list
}

/// The agent link properties, resolved in pass two.
pub(crate) fn agent_link_rules() -> Vec<Rule> {
    AGENT_LINK_PROPS
        .iter()
        .filter_map(|prop| {
            let field = AGENT_FIELDS.iter().find(|f| {
                matches!(f.dest, Dest::Property(p) if p == *prop)
            })?;
            Some(link(field.hk, prop))
        })
        .collect()
}

/// The agent fields folded into one JSON contract property each.
pub(crate) const FOLDED: &[(&str, &str)] = &[
    ("voice_enabled", "m_voice_json"),
    ("voice_auto_tts", "m_voice_json"),
    ("voice_chat_mode", "m_voice_json"),
    ("messaging_voice_mode", "m_voice_json"),
    ("stt_provider", "m_voice_json"),
    ("stt_model", "m_voice_json"),
    ("tts_provider", "m_voice_json"),
    ("tts_voice", "m_voice_json"),
    ("tts_speed", "m_voice_json"),
    ("profile_route_targets", "m_profile_routes"),
    ("profile_routes_default", "m_profile_routes"),
];
