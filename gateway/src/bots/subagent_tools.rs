//! Durable subagent tools on the Bots' own tool server (`POST /mcp/bot`).
//!
//! The caller is the Bot whose `mainThreadId` is `threadId`. A `parentSlug` in the
//! arguments is ignored, so a Bot cannot create a child for someone else.
use crate::{bots::Bots, AppState};
use serde_json::{json, Value};
use tokio::sync::Notify;

const MAX_TEXT: usize = 2000;
const NAMES: [&str; 2] = ["subagent_list", "subagent_create"];

/// What the tools need, passed in so tests can supply their own.
pub(crate) struct Deps<'a> {
    pub bots: &'a Bots,
    /// The Bots, as `bots.list()` gives them (each has `mainThreadId`).
    pub roster: &'a [Value],
    pub provision_now: &'a Notify,
    /// Saved settings. An empty object is no cap.
    pub settings: Value,
}

pub(crate) fn tool_defs() -> Vec<Value> {
    let thread = json!({
        "type": "string",
        "description": "Your main conversation's T3 thread id (in AGENTS.md, or mainThreadId in .bot/local.json or .bot/bot.json)."
    });
    vec![
        json!({
            "name": "subagent_list",
            "description": "List your lasting helpers and this Bot's helper policy. Each helper includes slug, name, prompt, inheritParentPrompt, mainThreadId (null until its thread has started), runtimeMode, model, and toolAllow. policy says whether you may create another. A one-off child thread of this turn is not listed here.",
            "inputSchema": {
                "type": "object",
                "properties": { "threadId": thread },
                "required": ["threadId"],
            },
        }),
        json!({
            "name": "subagent_create",
            "description": "Create a lasting helper: another Bot with its own folder, project, and main thread, and your face. The owner's helper policy decides whether this is allowed and which model, effort, and approval the helper gets. If lasting helpers are turned off, or this Bot is already at its limit, the call is refused. Pass the standing prompt. An optional task is the first job, queued once, and is not copied into the prompt. Omit inheritParentPrompt to use the owner's default. Work that ends with this turn stays a T3 child thread. mainThreadId is null until the thread starts; read it later with subagent_list.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "threadId": thread,
                    "prompt": { "type": "string", "description": "What this helper is for. Required, at most 2000 characters." },
                    "name": { "type": "string", "description": "Optional. Blank uses \"{your name} junior N\"." },
                    "inheritParentPrompt": { "type": "boolean", "description": "Copy your prompt into the helper's instructions. Omit this to use the owner's default." },
                    "task": { "type": "string", "description": "Optional first job, at most 2000 characters. Not copied into the prompt." },
                },
                "required": ["threadId", "prompt"],
            },
        }),
    ]
}

pub(crate) fn is_subagent_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

/// Entry point from `mcp.rs`.
pub(crate) async fn call(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    let roster = state.bots.list().await.map_err(|e| e.message)?;
    dispatch(
        &Deps {
            bots: &state.bots,
            roster: &roster,
            provision_now: &state.provision_now,
            settings: state.settings().await,
        },
        name,
        args,
    )
    .await
}

pub(crate) async fn dispatch(d: &Deps<'_>, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "subagent_list" => list(d, args),
        "subagent_create" => create(d, args).await,
        _ => Err(format!("Unknown tool {name}")),
    }
}

fn caller<'a>(roster: &'a [Value], args: &Value) -> Result<&'a Value, String> {
    let thread = args["threadId"]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or("threadId is required: pass your main thread id from AGENTS.md.")?;
    roster
        .iter()
        .find(|b| b["mainThreadId"] == thread)
        .ok_or_else(|| "That threadId isn't a Bot's main conversation. Pass your main thread id from AGENTS.md (or mainThreadId in .bot/local.json or .bot/bot.json).".to_string())
}

fn is_subagent(bot: &Value) -> bool {
    bot["role"] == "subagent" || bot["hideFromSidebar"] == true
}

fn child_view(bot: &Value) -> Value {
    let main_thread = bot["mainThreadId"].as_str().filter(|t| !t.is_empty());
    json!({
        "slug": bot["slug"],
        "name": bot["name"],
        "prompt": bot["subagent"]["prompt"].as_str().unwrap_or("").trim(),
        "inheritParentPrompt": bot["subagent"]["inheritParentPrompt"] == true,
        "mainThreadId": main_thread,
        "runtimeMode": text_or_null(bot.get("runtimeMode")),
        "model": model_view(&bot["model"]),
        "toolAllow": bot["subagent"].get("toolAllow").filter(|tools| tools.is_array()).cloned().unwrap_or(Value::Null),
    })
}

fn text_or_null(value: Option<&Value>) -> Value {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(|text| json!(text))
        .unwrap_or(Value::Null)
}

fn model_view(model: &Value) -> Value {
    let instance = model["instanceId"].as_str().filter(|id| !id.is_empty());
    let name = model["model"].as_str().filter(|name| !name.is_empty());
    match (instance, name) {
        (Some(instance), Some(name)) => json!({ "instanceId": instance, "model": name }),
        (Some(_), None) | (None, Some(_)) | (None, None) => Value::Null,
    }
}

fn policy_view(parent: &Value, count: usize) -> Value {
    let policy = crate::bots::helpers_policy::subagents_of(parent);
    json!({
        "enabled": policy["enabled"] == true,
        "maxCount": policy["maxCount"],
        "count": count,
        "when": policy["when"],
    })
}

fn list(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let parent = caller(d.roster, args)?;
    let slug = parent["slug"].as_str().unwrap_or_default();
    let subagents: Vec<Value> = d
        .roster
        .iter()
        .filter(|bot| bot["parentSlug"].as_str() == Some(slug))
        .map(child_view)
        .collect();
    if is_subagent(parent) {
        return Ok(json!({ "subagents": subagents }));
    }
    let count = subagents.len();
    Ok(json!({
        "subagents": subagents,
        "policy": policy_view(parent, count),
    }))
}

fn limited(args: &Value, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            let text = text.trim();
            if text.chars().count() > MAX_TEXT {
                return Err(format!("{key} must be at most {MAX_TEXT} characters."));
            }
            Ok(Some(text.to_string()).filter(|t| !t.is_empty()))
        }
        Some(_) => Err(format!("{key} must be a string.")),
    }
}

async fn create(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let parent = caller(d.roster, args)?;
    if is_subagent(parent) {
        return Err("A subagent can't have a subagent.".into());
    }
    let Some(prompt) = limited(args, "prompt")? else {
        return Err("prompt is required.".into());
    };
    let task = limited(args, "task")?;
    let name = limited(args, "name")?;
    let inherit = optional_bool(args, "inheritParentPrompt")?;
    let parent_slug = parent["slug"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("That threadId isn't a Bot's main conversation. Pass your main thread id from AGENTS.md (or mainThreadId in .bot/local.json or .bot/bot.json).")?;
    let mut subagent = json!({ "prompt": prompt });
    if let Some(inherit) = inherit {
        subagent["inheritParentPrompt"] = json!(inherit);
    }
    let mut input = json!({
        "parentSlug": parent_slug,
        "subagent": subagent,
    });
    if let Some(name) = name {
        input["name"] = json!(name);
    }
    if let Some(task) = task {
        input["brief"] = json!(task);
    }
    if let Some(created_by) = parent["name"]
        .as_str()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        input["createdBy"] = json!(created_by);
    }
    let bot = d.bots.create(&input).await.map_err(|e| e.message)?;
    let bot = cap_runtime_mode(d, parent, bot).await?;
    d.provision_now.notify_one();
    Ok(json!({
        "slug": bot["slug"],
        "name": bot["name"],
        "mainThreadId": Value::Null,
    }))
}

fn optional_bool(args: &Value, key: &str) -> Result<Option<bool>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("{key} must be a boolean.")),
    }
}

/// The settings cap, stored when it is still inside the parent's helper allowlist.
/// Create already applied the policy. This only lowers a mode that sits above the cap,
/// or writes the launch mode when the policy left the field unset.
async fn cap_runtime_mode(d: &Deps<'_>, parent: &Value, bot: Value) -> Result<Value, String> {
    let capped = crate::bots::provisioner::launch_runtime_mode(&bot, &d.settings);
    if bot["runtimeMode"].as_str() == Some(capped) {
        return Ok(bot);
    }
    if !crate::bots::helpers_policy::runtime_allowed(parent, capped) {
        return Ok(bot);
    }
    let slug = bot["slug"].as_str().unwrap_or("").to_string();
    d.bots
        .save(&slug, &json!({ "runtimeMode": capped }), false)
        .await
        .map_err(|err| err.message)
}

#[cfg(test)]
#[path = "subagent_tools_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "subagent_policy_tests.rs"]
mod policy_tests;
