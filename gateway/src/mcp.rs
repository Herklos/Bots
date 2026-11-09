//! The Bots' own tool server, `POST /mcp/bot` (MCP Streamable HTTP with JSON responses), as in
//! hbots `server/vault-mcp.mts` and `server/projects-mcp.mts`. Only local callers with the
//! bearer from `.bot-mcp-token` get in; requests that came through a tunnel are refused.
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
    if projects_on {
        list.extend([
            json!({ "name": "projects_overview", "description": "The Projects board: every item and recent activity.",
                "inputSchema": { "type": "object", "properties": { "threadId": thread } } }),
            json!({ "name": "projects_get", "description": "One board item by id.",
                "inputSchema": { "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] } }),
            json!({ "name": "projects_save", "description": "Create items (no id) or change them (id; null clears a field). Up to 100 at once.",
                "inputSchema": { "type": "object", "properties": { "threadId": thread, "items": { "type": "array", "items": { "type": "object" } } }, "required": ["threadId", "items"] } }),
            json!({ "name": "projects_delete", "description": "Delete board items; their children move up a level.",
                "inputSchema": { "type": "object", "properties": { "threadId": thread, "ids": { "type": "array", "items": { "type": "string" } } }, "required": ["threadId", "ids"] } }),
        ]);
    }
    json!(list)
}

/// The Bot whose main thread is `threadId`, as a board actor.
async fn bot_actor(state: &AppState, args: &Value) -> Result<Value, String> {
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
    let projects_on = state.settings().await["beta"]["projects"] == true;
    if name.starts_with("projects_") && !projects_on {
        return Err(format!("Unknown tool {name}"));
    }
    match name {
        "vault_list" => {
            bot_actor(state, args).await?;
            Ok(json!({ "items": state.vault.list().await.map_err(|e| e.message)? }))
        }
        "projects_overview" => Ok(state.projects.board().await),
        "projects_get" => {
            let id = args["id"].as_str().ok_or("id is required")?;
            let board = state.projects.board().await;
            board["items"]
                .as_array()
                .and_then(|l| l.iter().find(|i| i["id"] == id).cloned())
                .ok_or_else(|| format!("No item {id}."))
        }
        "projects_save" => {
            let who = bot_actor(state, args).await?;
            let items = args["items"].as_array().cloned().unwrap_or_default();
            state
                .projects
                .save(&items, who)
                .await
                .map_err(|e| e.message)
        }
        "projects_delete" => {
            let who = bot_actor(state, args).await?;
            let ids: Vec<String> = args["ids"]
                .as_array()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            state
                .projects
                .delete(&ids, who)
                .await
                .map_err(|e| e.message)
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
