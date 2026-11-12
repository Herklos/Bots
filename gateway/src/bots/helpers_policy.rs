//! Owner policy for lasting helpers and one-off child threads.
//!
//! Missing keys read as the product defaults. A save deep-merges `subagent`,
//! `subagents`, and `childThreads`. JSON null on a leaf removes that key.
//! Creating a durable child stays inside the parent's defaults and allowlist.

use super::{is_subagent, parent_slug_of};
use crate::platform::error::{ApiError, ApiResult};
use serde_json::{json, Map, Value};

pub(crate) const HELPERS_OFF: &str = "Lasting helpers are turned off for this Bot.";
pub(crate) const NO_POLICY: &str = "A subagent has no helpers policy.";
pub(crate) const MODEL_DENIED: &str = "That model isn't allowed for this Bot's helpers.";
pub(crate) const MODE_DENIED: &str = "That approval mode isn't allowed for this Bot's helpers.";

pub(super) const WHEN_LIMIT: usize = 2000;
const NESTED: [&str; 3] = ["subagent", "subagents", "childThreads"];
pub(super) const RUNTIME_MODES: [&str; 3] = ["approval-required", "auto", "full-access"];
pub(super) const EFFORTS: [&str; 3] = ["low", "medium", "high"];
const SUBAGENT_WHEN: &str = "Create a lasting helper when the work will outlive this turn and needs its own notes, skills, or ongoing thread. Prefer a one-off child thread for work that ends with this turn.";
const CHILD_WHEN: &str = "Start a child thread for parallel or specialised work that ends with this turn. Do not create a lasting Subagent for that.";

pub(crate) fn default_subagents() -> Value {
    json!({
        "enabled": true,
        "maxCount": 8,
        "when": SUBAGENT_WHEN,
        "defaults": {
            "model": null,
            "runtimeMode": null,
            "inheritParentPrompt": false,
            "toolAllow": null,
            "effort": null
        },
        "allowlist": {
            "models": null,
            "runtimeModes": RUNTIME_MODES,
            "efforts": EFFORTS
        }
    })
}

pub(crate) fn default_child_threads() -> Value {
    json!({
        "enabled": true,
        "maxConcurrent": 4,
        "when": CHILD_WHEN,
        "defaults": { "model": null, "runtimeMode": null, "effort": null },
        "allowlist": {
            "models": null,
            "runtimeModes": RUNTIME_MODES,
            "efforts": EFFORTS
        }
    })
}

/// Fill absent policy on a Bot the API is about to return. This does not write the file.
pub(crate) fn resolve_on_view(bot: &mut Value) {
    if is_subagent(bot) {
        if let Some(obj) = bot.as_object_mut() {
            obj.remove("subagents");
            obj.remove("childThreads");
        }
        return;
    }
    let subagents = bot.get("subagents").cloned().unwrap_or(Value::Null);
    let threads = bot.get("childThreads").cloned().unwrap_or(Value::Null);
    if let Some(obj) = bot.as_object_mut() {
        obj.insert(
            "subagents".into(),
            overlay(&default_subagents(), &subagents),
        );
        obj.insert(
            "childThreads".into(),
            overlay(&default_child_threads(), &threads),
        );
    }
}

pub(crate) struct ChildPlan {
    pub model: Option<Value>,
    pub runtime_mode: Option<String>,
    pub inherit_parent_prompt: Option<bool>,
    pub tool_allow: Option<Value>,
}

pub(crate) fn plan_child(
    input: &Value,
    parent: &Value,
    existing: &[Value],
) -> ApiResult<ChildPlan> {
    if input.get("subagents").is_some() || input.get("childThreads").is_some() {
        return Err(ApiError::bad_request(NO_POLICY));
    }
    gate(parent, existing)?;
    let policy = resolved_subagents(parent);
    let sent = input.get("subagent").and_then(Value::as_object);
    let inherit = if sent.is_some_and(|fields| fields.contains_key("inheritParentPrompt")) {
        None
    } else {
        Some(policy["defaults"]["inheritParentPrompt"] == true)
    };
    let tool_allow = if sent.is_some_and(|fields| fields.contains_key("toolAllow")) {
        None
    } else {
        policy["defaults"]["toolAllow"]
            .as_array()
            .cloned()
            .map(Value::Array)
    };
    Ok(ChildPlan {
        model: model::resolve_model(input, parent, &policy)?,
        runtime_mode: model::resolve_mode(input, parent, &policy)?,
        inherit_parent_prompt: inherit,
        tool_allow,
    })
}

pub(crate) fn apply_plan(bot: &mut Value, plan: ChildPlan) {
    match plan.model {
        Some(model) => bot["model"] = model,
        None => bot["model"] = Value::Null,
    }
    match plan.runtime_mode {
        Some(mode) => bot["runtimeMode"] = json!(mode),
        None => {
            if let Some(obj) = bot.as_object_mut() {
                obj.remove("runtimeMode");
            }
        }
    }
    if let Some(inherit) = plan.inherit_parent_prompt {
        bot["subagent"]["inheritParentPrompt"] = json!(inherit);
    }
    if let Some(tools) = plan.tool_allow {
        bot["subagent"]["toolAllow"] = tools;
    }
}

/// Under the save lock, so two creates cannot both pass `maxCount`.
pub(crate) fn ensure_can_add(bots: &[Value], child: &Value) -> ApiResult<()> {
    if !is_subagent(child) {
        return Ok(());
    }
    let Some(parent_slug) = parent_slug_of(child) else {
        return Ok(());
    };
    let parent = bots
        .iter()
        .find(|bot| bot["slug"].as_str() == Some(parent_slug.as_str()))
        .ok_or_else(|| ApiError::bad_request("That parent Bot does not exist."))?;
    gate(parent, bots)
}

pub(crate) fn merge_policy(
    bot: &mut Map<String, Value>,
    patch: &Map<String, Value>,
) -> ApiResult<()> {
    let child = is_subagent(&json!({
        "role": bot.get("role").cloned().unwrap_or(Value::Null),
        "hideFromSidebar": bot.get("hideFromSidebar").cloned().unwrap_or(Value::Bool(false)),
    }));
    if child && (patch.contains_key("subagents") || patch.contains_key("childThreads")) {
        return Err(ApiError::bad_request(NO_POLICY));
    }
    for key in NESTED {
        let Some(value) = patch.get(key) else {
            continue;
        };
        if value.is_null() {
            if key == "subagent" && child {
                return Err(ApiError::bad_request("subagent must be an object"));
            }
            bot.remove(key);
            continue;
        }
        let Some(patch_obj) = value.as_object() else {
            return Err(ApiError::bad_request(format!("{key} must be an object")));
        };
        let mut base = bot
            .get(key)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        merge_into(&mut base, patch_obj);
        bot.insert(key.to_string(), Value::Object(base));
    }
    if let Some(value) = bot.get("subagents") {
        validate::validate_policy(value, true)?;
    }
    if let Some(value) = bot.get("childThreads") {
        validate::validate_policy(value, false)?;
    }
    if let Some(value) = bot.get("subagent").filter(|value| value.is_object()) {
        validate::validate_tool_allow(value.get("toolAllow"), "subagent.toolAllow")?;
    }
    Ok(())
}

/// A model or approval change on a durable child, checked against the parent allowlist.
pub(crate) fn check_child_update(
    parent: &Value,
    selection: &Value,
    runtime_mode: Option<&str>,
) -> ApiResult<()> {
    let policy = resolved_subagents(parent);
    let allow = &policy["allowlist"];
    if !model::model_allowed(selection, &allow["models"]) {
        return Err(ApiError::bad_request(MODEL_DENIED));
    }
    let effort = crate::harness::providers::effort_of(selection);
    if !effort.is_empty() && !model::listed(&allow["efforts"], &effort) {
        return Err(ApiError::bad_request(
            "That effort isn't allowed for this Bot's helpers.",
        ));
    }
    if let Some(mode) = runtime_mode {
        if !model::listed(&allow["runtimeModes"], mode) {
            return Err(ApiError::bad_request(MODE_DENIED));
        }
    }
    Ok(())
}

pub(crate) fn at_limit_message(count: i64) -> String {
    format!("This Bot already has {count} lasting helpers.")
}

/// True when `mode` is inside the parent's helper allowlist. A missing list allows every mode.
pub(crate) fn runtime_allowed(parent: &Value, mode: &str) -> bool {
    model::listed(&subagents_of(parent)["allowlist"]["runtimeModes"], mode)
}

fn gate(parent: &Value, existing: &[Value]) -> ApiResult<()> {
    let policy = resolved_subagents(parent);
    if policy["enabled"] != true {
        return Err(ApiError::new(409, HELPERS_OFF));
    }
    let max = policy["maxCount"].as_i64().unwrap_or(8);
    let count = helper_count(parent, existing);
    if count >= max {
        return Err(ApiError::new(409, at_limit_message(count)));
    }
    Ok(())
}

pub(crate) fn helper_count(parent: &Value, existing: &[Value]) -> i64 {
    let parent_slug = parent["slug"].as_str().unwrap_or("");
    i64::try_from(
        existing
            .iter()
            .filter(|bot| parent_slug_of(bot).as_deref() == Some(parent_slug))
            .count(),
    )
    .unwrap_or(i64::MAX)
}

/// The parent's lasting-helper policy, with missing keys filled from the product defaults.
pub(crate) fn subagents_of(parent: &Value) -> Value {
    overlay(
        &default_subagents(),
        parent.get("subagents").unwrap_or(&Value::Null),
    )
}

/// The parent's one-off child-thread policy, with missing keys filled from the product defaults.
pub(crate) fn child_threads_of(parent: &Value) -> Value {
    overlay(
        &default_child_threads(),
        parent.get("childThreads").unwrap_or(&Value::Null),
    )
}

fn resolved_subagents(parent: &Value) -> Value {
    subagents_of(parent)
}

fn overlay(default: &Value, stored: &Value) -> Value {
    let (Some(defaults), Some(stored_obj)) = (default.as_object(), stored.as_object()) else {
        return default.clone();
    };
    let mut out = Map::new();
    for (key, default_value) in defaults {
        match stored_obj.get(key) {
            Some(stored_value) if default_value.is_object() && stored_value.is_object() => {
                out.insert(key.clone(), overlay(default_value, stored_value));
            }
            Some(stored_value) if !stored_value.is_null() => {
                out.insert(key.clone(), stored_value.clone());
            }
            _ => {
                out.insert(key.clone(), default_value.clone());
            }
        }
    }
    Value::Object(out)
}

fn merge_into(base: &mut Map<String, Value>, patch: &Map<String, Value>) {
    for (key, value) in patch {
        if value.is_null() {
            base.remove(key);
            continue;
        }
        if let Some(patch_obj) = value.as_object() {
            let mut child = base
                .get(key)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            merge_into(&mut child, patch_obj);
            base.insert(key.clone(), Value::Object(child));
            continue;
        }
        base.insert(key.clone(), value.clone());
    }
}

mod model;
mod validate;
