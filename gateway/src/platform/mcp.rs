//! The Bots' own tool server, `POST /mcp/bot` (MCP Streamable HTTP with JSON responses).
//! Only local callers with the bearer from `.bot-mcp-token` get in; requests that came
//! through a tunnel are refused.
//!
//! `subagent.toolAllow` filters this server's tools for that Bot. Null keeps the full
//! list. A string array offers only the named gateway tools. T3's own tools are untouched.
use crate::AppState;
use serde_json::{json, Value};
use std::sync::Arc;

const PROTOCOL: &str = "2025-06-18";

fn tools(projects_on: bool) -> Value {
    let thread = json!({ "type": "string", "description": "Your main T3 thread id." });
    let mut list = vec![json!({
        "name": "vault_list",
        "description": "List the Vault's saved logins and contact info. Passwords are never included.",
        "inputSchema": { "type": "object", "properties": { "threadId": thread }, "required": ["threadId"] },
    })];
    list.extend(crate::vault::vault_tools::tool_defs()); // T1 hook: vault_fill_*, vault_save_*, vault_secret_file
    list.extend(crate::bots::subagent_tools::tool_defs());
    list.extend(crate::bots::memory_tools::tool_defs());
    list.extend(crate::bots::image_tools::tool_defs());
    // PROJECTS-HOOK (T3): the Projects tools are defined in projects_tools.rs.
    if projects_on {
        list.extend(crate::projects::projects_tools::tools());
    }
    json!(list)
}

/// The Bot whose main thread is `threadId`, as a board actor.
pub(crate) async fn bot_actor(state: &AppState, args: &Value) -> Result<Value, String> {
    let thread = args["threadId"].as_str().ok_or("threadId is required")?;
    let bots = state.bots.list().await.map_err(|e| e.message)?;
    let bot = bots
        .iter()
        .find(|b| b["mainThreadId"] == thread)
        .ok_or("No Bot has that main thread. Pass your own main threadId.")?;
    Ok(json!({ "type": "bot", "name": bot["name"], "slug": bot["slug"] }))
}

/// Names in `subagent.toolAllow`, when that field is a string array.
/// Null, a missing field, and any other shape mean no restriction.
fn tool_allow_names(bot: &Value) -> Option<&Vec<Value>> {
    let subagent = bot.get("subagent")?;
    let fields = match subagent {
        Value::Object(fields) => fields,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            return None
        }
    };
    match fields.get("toolAllow") {
        Some(Value::Array(names)) => Some(names),
        None
        | Some(
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Object(_),
        ) => None,
    }
}

fn name_listed(names: &[Value], name: &str) -> bool {
    names.iter().any(|allowed| allowed.as_str() == Some(name))
}

/// Null `toolAllow` returns `catalog` unchanged. An array keeps named gateway tools only.
fn offered_tools(catalog: Value, bot: Option<&Value>) -> Value {
    let Some(bot) = bot else {
        return catalog;
    };
    let Some(names) = tool_allow_names(bot) else {
        return catalog;
    };
    let Some(rows) = catalog.as_array() else {
        return catalog;
    };
    let kept: Vec<Value> = rows
        .iter()
        .filter(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name_listed(names, name))
        })
        .cloned()
        .collect();
    json!(kept)
}

fn tool_known(catalog: &Value, name: &str) -> bool {
    catalog
        .as_array()
        .is_some_and(|rows| rows.iter().any(|tool| tool["name"].as_str() == Some(name)))
}

/// A gateway tool outside the allowlist is refused. A name this server does not offer,
/// including a T3 tool name, is left for the caller to reject as unknown.
fn refuse_hidden(bot: &Value, name: &str, catalog: &Value) -> Result<(), String> {
    if !tool_known(catalog, name) {
        return Ok(());
    }
    let offered = offered_tools(catalog.clone(), Some(bot));
    if tool_known(&offered, name) {
        Ok(())
    } else {
        Err("That tool isn't allowed for this Bot.".into())
    }
}

async fn call_tool(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    let projects_on = state.settings().await["beta"]["projects"] == true;
    if let Some(thread) = args
        .get("threadId")
        .and_then(Value::as_str)
        .filter(|thread| !thread.is_empty())
    {
        let bots = state.bots.list().await.map_err(|err| err.message)?;
        if let Some(bot) = bots.iter().find(|bot| bot["mainThreadId"] == thread) {
            refuse_hidden(bot, name, &tools(projects_on))?;
        }
    }
    // T1 hook: the Vault tools that need the browser, the secret files or the contact store.
    if crate::vault::vault_tools::is_vault_tool(name) {
        return crate::vault::vault_tools::call(state, name, args).await;
    }
    if crate::bots::subagent_tools::is_subagent_tool(name) {
        return crate::bots::subagent_tools::call(state, name, args).await;
    }
    if crate::bots::memory_tools::is_memory_tool(name) {
        return crate::bots::memory_tools::call(state, name, args).await;
    }
    if crate::bots::image_tools::is_image_tool(name) {
        return crate::bots::image_tools::call(state, name, args).await;
    }
    if name.starts_with("projects_") && !projects_on {
        return Err(format!("Unknown tool {name}"));
    }
    // PROJECTS-HOOK (T3): projects_* calls are answered by projects_tools.rs.
    if let Some(result) = crate::projects::projects_tools::call(state, name, args).await {
        return result;
    }
    match name {
        "vault_list" => {
            bot_actor(state, args).await?;
            Ok(json!({ "items": state.vault.list().await.map_err(|e| e.message)? }))
        }
        _ => Err(format!("Unknown tool {name}")),
    }
}

async fn listed_tools(state: &AppState, bot_slug: Option<&str>) -> Result<Value, String> {
    let projects_on = state.settings().await["beta"]["projects"] == true;
    let catalog = tools(projects_on);
    let Some(slug) = bot_slug else {
        return Ok(catalog);
    };
    let bot = state.bots.get(slug).await.map_err(|err| err.message)?;
    Ok(offered_tools(catalog, bot.as_ref()))
}

/// One JSON-RPC message; notifications get no reply.
/// `bot_slug` is the `bot` query on this Bot's MCP URL. Absent means no list restriction.
pub(crate) async fn handle(
    state: &Arc<AppState>,
    msg: &Value,
    bot_slug: Option<&str>,
) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let ok = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result });
    Some(match msg["method"].as_str().unwrap_or("") {
        "initialize" => ok(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "bot", "title": "Bot", "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => ok(json!({})),
        "tools/list" => match listed_tools(state, bot_slug).await {
            Ok(tools) => ok(json!({ "tools": tools })),
            Err(message) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32603, "message": message }
            }),
        },
        "tools/call" => {
            let name = msg["params"]["name"].as_str().unwrap_or("");
            let args = msg["params"]["arguments"].clone();
            match call_tool(state, name, &args).await {
                Ok(v) => ok(
                    json!({ "content": [{ "type": "text", "text": v.to_string() }], "structuredContent": v }),
                ),
                Err(e) => {
                    ok(json!({ "content": [{ "type": "text", "text": e }], "isError": true }))
                }
            }
        }
        other => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("Method not found: {other}") } })
        }
    })
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
