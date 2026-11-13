//! The HK space (`anytype-agents`) source contract: which org types and agent
//! fields this gateway syncs, where they land in Bots, and what stays raw.
//! See docs/anytype-hk-sync-plan.md. Read-only for HK: this module never
//! writes to the source space.

use super::super::client::{AnytypeClient, AnytypeError};
use serde_json::{json, Value};

/// An HK org type we recognize, with its Bots destination type. Types marked
/// `""` have no Bots type; their objects are preserved through `m_source_json`
/// on the referencing object only (never materialized).
pub(crate) struct HkType {
    pub hk_key: &'static str,
    pub bots_key: &'static str,
}

pub(crate) const HK_TYPES: &[HkType] = &[
    HkType { hk_key: "agent", bots_key: "m_bot" },
    HkType { hk_key: "team", bots_key: "m_team" },
    HkType { hk_key: "subteam", bots_key: "m_team" },
    HkType { hk_key: "subagent", bots_key: "m_subagent" },
    HkType { hk_key: "harness", bots_key: "m_harness" },
    HkType { hk_key: "runtime", bots_key: "m_harness" },
    HkType { hk_key: "harness_profile", bots_key: "m_model_profile" },
    HkType { hk_key: "computer", bots_key: "m_computer" },
    HkType { hk_key: "tool_profile", bots_key: "m_tool_profile" },
    HkType { hk_key: "prompt", bots_key: "m_prompt" },
    HkType { hk_key: "memory", bots_key: "m_memory" },
    HkType { hk_key: "skill", bots_key: "m_skill" },
    HkType { hk_key: "schedule", bots_key: "m_schedule" },
    HkType { hk_key: "group", bots_key: "m_group" },
    HkType { hk_key: "project", bots_key: "m_project" },
    HkType { hk_key: "epic", bots_key: "m_epic" },
    HkType { hk_key: "user_story", bots_key: "m_user_story" },
    HkType { hk_key: "task", bots_key: "m_task" },
    // No Bots consumer; preserved as raw JSON, never typed.
    HkType { hk_key: "browser", bots_key: "" },
    HkType { hk_key: "vault_grant", bots_key: "" },
    HkType { hk_key: "log", bots_key: "m_log" },
    HkType { hk_key: "suggestion", bots_key: "m_suggestion" },
];

/// Where one HK agent field lands in Bots.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Dest {
    /// A Bots property; conversion decided by the mapper.
    Property(&'static str),
    /// Folded into one JSON contract property.
    Folded(&'static str),
    /// No typed destination; kept in `m_source_json`.
    Raw(&'static str),
    /// Kept on the machine; never synced.
    Local,
}

pub(crate) struct HkField {
    pub hk: &'static str,
    pub dest: Dest,
    pub note: &'static str,
}

/// Agent → m_bot. The audit table's gap list, field by field. Anything an
/// agent object carries that is not listed here lands in `m_source_json`.
pub(crate) const AGENT_FIELDS: &[HkField] = &[
    HkField { hk: "name", dest: Dest::Property("name"), note: "object name" },
    HkField { hk: "role", dest: Dest::Property("m_short_description"), note: "HK role is free text; Bots m_role is a closed select" },
    HkField { hk: "team", dest: Dest::Property("m_team"), note: "object link, resolved in pass two" },
    HkField { hk: "reports_to", dest: Dest::Property("m_parent"), note: "resolved to the bot's parent" },
    HkField { hk: "lifecycle", dest: Dest::Property("m_lifecycle"), note: "select, same vocabulary" },
    HkField { hk: "autonomy", dest: Dest::Property("m_autonomy"), note: "dashes become underscores" },
    HkField { hk: "hermes_profile", dest: Dest::Property("m_slug"), note: "the adoption key" },
    HkField { hk: "short_description", dest: Dest::Property("x"), note: "mission line" },
    HkField { hk: "harness_profile", dest: Dest::Property("m_model_profile"), note: "link to m_model_profile" },
    HkField { hk: "subagents", dest: Dest::Property("m_subagents"), note: "link to m_subagent objects" },
    HkField { hk: "delegation_enabled", dest: Dest::Property("m_delegation_enabled"), note: "" },
    HkField { hk: "delegation_backend", dest: Dest::Property("m_delegation_backend"), note: "HK select becomes text" },
    HkField { hk: "profile_route_targets", dest: Dest::Folded("m_profile_routes"), note: "object ids as a JSON list" },
    HkField { hk: "profile_routes_default", dest: Dest::Folded("m_profile_routes"), note: "merged with targets under one contract" },
    HkField { hk: "system_prompt", dest: Dest::Property("m_system_prompt"), note: "link to m_prompt; body sync waits for the codec" },
    HkField { hk: "messaging_group", dest: Dest::Property("m_messaging_groups"), note: "link to m_group" },
    HkField { hk: "memory_pages", dest: Dest::Property("m_memory_pages"), note: "link to m_memory" },
    HkField { hk: "runtime", dest: Dest::Property("m_runtime"), note: "link to m_harness" },
    HkField { hk: "terminal_backend", dest: Dest::Property("m_terminal_backend"), note: "select becomes text" },
    HkField { hk: "computer_surface", dest: Dest::Property("m_computer_surface"), note: "select becomes text" },
    HkField { hk: "board_member", dest: Dest::Property("m_board_member"), note: "" },
    HkField { hk: "applied_rules_version", dest: Dest::Property("m_rules_version"), note: "mirror" },
    HkField { hk: "browser", dest: Dest::Raw("no Bots browser type; the link id is kept"), note: "" },
    HkField { hk: "computer", dest: Dest::Raw("no Bots computer type yet; the link id stays raw"), note: "" },
    HkField { hk: "tool_profile", dest: Dest::Raw("no Bots tool-profile type yet"), note: "" },
    HkField { hk: "voice_enabled", dest: Dest::Folded("m_voice_json"), note: "voice contract: enabled" },
    HkField { hk: "voice_auto_tts", dest: Dest::Folded("m_voice_json"), note: "voice contract: auto_tts" },
    HkField { hk: "voice_chat_mode", dest: Dest::Folded("m_voice_json"), note: "voice contract: chat_mode" },
    HkField { hk: "messaging_voice_mode", dest: Dest::Folded("m_voice_json"), note: "voice contract: messaging_mode" },
    HkField { hk: "stt_provider", dest: Dest::Folded("m_voice_json"), note: "voice contract: stt.provider" },
    HkField { hk: "stt_model", dest: Dest::Folded("m_voice_json"), note: "voice contract: stt.model" },
    HkField { hk: "tts_provider", dest: Dest::Folded("m_voice_json"), note: "voice contract: tts.provider" },
    HkField { hk: "tts_voice", dest: Dest::Folded("m_voice_json"), note: "voice contract: tts.voice" },
    HkField { hk: "tts_speed", dest: Dest::Folded("m_voice_json"), note: "voice contract: tts.speed" },
    HkField { hk: "voice_notes", dest: Dest::Raw("apply notes stay raw"), note: "" },
    HkField { hk: "map_path", dest: Dest::Raw("no Bots property; the org map stays in HK"), note: "" },
    HkField { hk: "memory_review_off", dest: Dest::Raw("Bots review cadence is a global setting"), note: "" },
    HkField { hk: "analytics_enabled", dest: Dest::Raw("no Bots analytics"), note: "" },
    HkField { hk: "avatar", dest: Dest::Property("m_avatar"), note: "files" },
    HkField { hk: "skills_page", dest: Dest::Local, note: "queries are per space; the import creates Bots queries instead" },
    HkField { hk: "logs_page", dest: Dest::Local, note: "" },
    HkField { hk: "agenda_page", dest: Dest::Local, note: "" },
    HkField { hk: "suggestions_page", dest: Dest::Local, note: "" },
    HkField { hk: "tag", dest: Dest::Local, note: "built-in tag property" },
    HkField { hk: "links", dest: Dest::Local, note: "built-in link list" },
    HkField { hk: "backlinks", dest: Dest::Local, note: "built-in backlinks" },
];


pub(crate) fn classify_agent(properties: &Value) -> Counted {
    let mut counted = Counted::default();
    let Some(map) = properties.as_object() else {
        return counted;
    };
    let names: Vec<&str> = map.keys().map(String::as_str).collect();
    for key in names {        let rule = AGENT_FIELDS.iter().find(|rule| key == rule.hk);
        match rule {
            Some(rule) => counted.bump(rule.dest),
            None => counted.unmapped.push(key.to_string()),
        }
    }
    counted
}

#[derive(Default)]
pub(crate) struct Counted {
    pub property: Vec<&'static str>,
    pub folded: Vec<&'static str>,
    pub raw: Vec<&'static str>,
    pub local: Vec<&'static str>,
    pub unmapped: Vec<String>,
}

impl Counted {
    fn bump(&mut self, dest: Dest) {
        match dest {
            Dest::Property(name) => self.property.push(strip_dest(name)),
            Dest::Folded(name) => self.folded.push(strip_dest(name)),
            Dest::Raw(_) => self.raw.push("raw"),
            Dest::Local => self.local.push("local"),
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "typed": self.property.len(),
            "folded": self.folded.len(),
            "raw": self.raw.len(),
            "local": self.local.len(),
            "unmapped": self.unmapped,
        })
    }
}

fn strip_dest(name: &'static str) -> &'static str {
    name.strip_prefix("m_").unwrap_or(name)
}

/// The read-only survey: HK type counts with their Bots destinations, and the
/// field classification of one real agent. Never writes to either space.
pub(crate) async fn survey(bots: &AnytypeClient, hk: &AnytypeClient) -> Value {
    let mut types = Vec::new();
    for entry in HK_TYPES {
        let count = match hk.search(&json!({ "type": entry.hk_key })).await {
            Ok(rows) => rows.len() as i64,
            Err(_) => -1,
        };
        types.push(json!({ "hk": entry.hk_key, "bots": entry.bots_key, "count": count }));
    }
    let agent_sample = agent_sample(hk).await.unwrap_or_else(|_| json!({}));
    let _ = bots; // the Bots-side extension attach counts come from Prepare
    json!({ "types": types, "agent_sample": agent_sample })
}

async fn agent_sample(hk: &AnytypeClient) -> Result<Value, AnytypeError> {
    let rows = hk.search(&json!({ "type": "agent" })).await?;
    let Some(id) = rows.iter().find_map(|row| row["id"].as_str()) else {
        return Ok(json!({ "error": "no agent in the HK space" }));
    };
    let object = hk.get_object(id).await?;
    let properties = object["properties"].clone();
    let counted = classify_agent(&properties);
    let mut value = counted.to_json();
    value["name"] = json!(object["name"].as_str().unwrap_or(""));
    value["fields"] = json!(properties
        .as_object()
        .map(|map| map.len())
        .unwrap_or(0));
    Ok(value)
}
