//! The Bots' own tool server, `POST /mcp/bot` (MCP Streamable HTTP with JSON responses).
//! Only local callers with the bearer from `.bot-mcp-token` get in; requests that came
//! through a tunnel are refused.
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
    list.extend(crate::vault_tools::tool_defs()); // T1 hook: vault_fill_*, vault_save_*, vault_secret_file
    list.extend(crate::subagent_tools::tool_defs());
    list.extend(crate::memory_tools::tool_defs());
    // PROJECTS-HOOK (T3): the Projects tools are defined in projects_tools.rs.
    if projects_on {
        list.extend(crate::projects_tools::tools());
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

async fn call_tool(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    // T1 hook: the Vault tools that need the browser, the secret files or the contact store.
    if crate::vault_tools::is_vault_tool(name) {
        return crate::vault_tools::call(state, name, args).await;
    }
    if crate::subagent_tools::is_subagent_tool(name) {
        return crate::subagent_tools::call(state, name, args).await;
    }
    if crate::memory_tools::is_memory_tool(name) {
        return crate::memory_tools::call(state, name, args).await;
    }
    let projects_on = state.settings().await["beta"]["projects"] == true;
    if name.starts_with("projects_") && !projects_on {
        return Err(format!("Unknown tool {name}"));
    }
    // PROJECTS-HOOK (T3): projects_* calls are answered by projects_tools.rs.
    if let Some(result) = crate::projects_tools::call(state, name, args).await {
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

/// One JSON-RPC message; notifications get no reply.
pub async fn handle(state: &Arc<AppState>, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let ok = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result });
    Some(match msg["method"].as_str().unwrap_or("") {
        "initialize" => ok(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "bot", "title": "Bot", "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => ok(json!({})),
        "tools/list" => {
            ok(json!({ "tools": tools(state.settings().await["beta"]["projects"] == true) }))
        }
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
