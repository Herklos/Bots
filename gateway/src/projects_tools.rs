//! The Projects board as tools on the Bots' own MCP server (`bot`), as in hbots
//! `server/projects-mcp.mts`. Every change a Bot makes here shows on the user's open Projects
//! page straight away, labelled with the Bot's name. Also the same-origin check the page's
//! writes need (`server/projects-routes.mts`).
use crate::{
    error::ApiError,
    mcp::bot_actor,
    projects::{self, FIELD_NAMES, KINDS, LINK_TYPES, PRIORITIES},
    AppState,
};
use axum::http::HeaderMap;
use serde_json::{json, Value};

const NAMES: &[&str] = &[
    "projects_overview",
    "projects_find",
    "projects_get",
    "projects_save",
    "projects_delete",
];

/// Links in tool results are built against this address when the gateway knows none.
const APP_URL: &str = "";

fn msg(e: ApiError) -> String {
    e.message
}

fn status_help() -> String {
    KINDS
        .iter()
        .filter(|k| !statuses(k).is_empty())
        .map(|k| format!("{k}: {}", statuses(k).join("|")))
        .collect::<Vec<_>>()
        .join("; ")
}

fn statuses(kind: &str) -> &'static [&'static str] {
    projects::statuses(kind)
}

fn link_schema() -> Value {
    json!({
        "type": "object",
        "description": format!("A link. type is one of {}. thread: {{threadId, environmentId?}} for a T3 thread (environmentId for another machine). bot: {{slug}}. item: {{id}} for another board item. url: {{url}}. t3project: {{projectId, environmentId?}}. Any link can have a label.", LINK_TYPES.join(", ")),
        "properties": {
            "type": { "type": "string", "enum": LINK_TYPES },
            "threadId": { "type": "string" },
            "environmentId": { "type": "string" },
            "slug": { "type": "string" },
            "id": { "type": "string" },
            "url": { "type": "string" },
            "projectId": { "type": "string" },
            "label": { "type": "string" }
        },
        "required": ["type"]
    })
}

fn item_schema() -> Value {
    let field_props: Value = FIELD_NAMES
        .iter()
        .map(|name| (name.to_string(), json!({})))
        .collect::<serde_json::Map<_, _>>()
        .into();
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": "Leave out to create an item. Give an existing id to change only the properties you pass." },
            "ref": { "type": "string", "description": "A short name for a new item, so other items in this same call can say parentId or machineId \"ref:<name>\"." },
            "kind": { "type": "string", "enum": KINDS, "description": "project, task (a to-do), machine, service (something running somewhere), repo, note or link." },
            "title": { "type": "string", "description": "Short and specific, e.g. \"Ship hbots projects page\"." },
            "status": { "type": "string", "description": format!("Depends on kind. {}. Note and link have none.", status_help()) },
            "area": { "type": "string", "description": "Which part of life it belongs to, e.g. work or personal. Reuse the areas already on the board." },
            "parentId": { "type": ["string", "null"], "description": "The project (or other item) this belongs to. null takes it out." },
            "machineId": { "type": ["string", "null"], "description": "For services and repos: the machine item it runs or lives on." },
            "body": { "type": "string", "description": "Markdown notes: context, decisions, next steps, commands. Replaces the old body; read it first with projects_get." },
            "fields": {
                "type": "object",
                "description": format!("Extra facts, merged into the existing ones (null removes one): {}. due is YYYY-MM-DD, priority is {}, port is a number, tags is a list.", FIELD_NAMES.join(", "), PRIORITIES.join("|")),
                "properties": field_props
            },
            "addLinks": { "type": "array", "items": link_schema(), "description": "Links to add: threads working on it, Bots that own it, related items, web pages." },
            "removeLinks": { "type": "array", "items": link_schema(), "description": "Links to take off (match on type and target)." },
            "links": { "type": "array", "items": link_schema(), "description": "Replace all links. Prefer addLinks/removeLinks." },
            "pinned": { "type": "boolean", "description": "Pinned items sit at the top of the page." },
            "order": { "type": ["number", "null"], "description": "Sort position among siblings; lower comes first." },
            "archived": { "type": "boolean", "description": "true hides it from the page without deleting it; false brings it back." }
        }
    })
}

fn thread_prop() -> Value {
    json!({ "type": "string", "description": "Your main conversation's T3 thread id (in AGENTS.md, or mainThreadId in .bot/bot.json). Changes are shown as made by you." })
}

/// The Projects tools as tools/list shows them.
pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "projects_overview",
            "title": "Read the Projects board",
            "description": "The user's Projects page at a glance: every project with its open to-do count, open to-dos (highest priority and soonest first), machines with what runs on each, and the latest changes. Start here when the user asks what's on their plate, what's running where, or before adding things, so you don't duplicate.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": true }
        }),
        json!({
            "name": "projects_find",
            "title": "Find board items",
            "description": "Search the Projects board. Filter by kind, status, area, parent project, machine or a linked thread, or search words in titles, notes, tags, paths and addresses.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "search": { "type": "string" },
                    "kind": { "type": "string", "enum": KINDS },
                    "status": { "type": "string" },
                    "open": { "type": "boolean", "description": "Only to-dos that aren't done." },
                    "area": { "type": "string" },
                    "parentId": { "type": "string" },
                    "machineId": { "type": "string" },
                    "threadId": { "type": "string", "description": "Items linked to this T3 thread." },
                    "includeArchived": { "type": "boolean" },
                    "limit": { "type": "number" }
                }
            },
            "annotations": { "readOnlyHint": true }
        }),
        json!({
            "name": "projects_get",
            "title": "Open a board item",
            "description": "One item in full: notes, fields, links, what's inside it, what's on it (for a machine) and what links to it, plus its link to share in chat.",
            "inputSchema": { "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] },
            "annotations": { "readOnlyHint": true }
        }),
        json!({
            "name": "projects_save",
            "title": "Add or change board items",
            "description": "Create items or change existing ones on the Projects board, up to 100 at once. The user sees each change on their screen immediately. Leave id out to create; pass id to change only what you give. To create a project with its to-dos in one call, give the project ref \"x\" and the to-dos parentId \"ref:x\". Mark a to-do done with status \"done\" when the work is finished. Returns the saved items with their links.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "items": { "type": "array", "items": item_schema(), "minItems": 1, "maxItems": 100 },
                    "threadId": thread_prop()
                },
                "required": ["items", "threadId"]
            }
        }),
        json!({
            "name": "projects_delete",
            "title": "Delete board items",
            "description": "Delete items for good. Things inside them move up to the deleted item's parent. Prefer archived: true with projects_save unless the user wants it gone or you made a mistake.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "ids": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                    "threadId": thread_prop()
                },
                "required": ["ids", "threadId"]
            },
            "annotations": { "destructiveHint": true }
        }),
    ]
}

/// Runs a Projects tool. None when `name` isn't one of them, so the caller can answer "unknown tool".
pub async fn call(state: &AppState, name: &str, args: &Value) -> Option<Result<Value, String>> {
    if !NAMES.contains(&name) {
        return None;
    }
    Some(run(state, name, args).await)
}

async fn run(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    let board = &state.projects;
    match name {
        "projects_overview" => {
            let items = board.items().await.map_err(msg)?;
            let activity = board.activity().await.map_err(msg)?;
            Ok(projects::overview(&items, &activity, APP_URL))
        }
        "projects_find" => {
            let items = board.items().await.map_err(msg)?;
            Ok(projects::find(&items, args, APP_URL))
        }
        "projects_get" => {
            let id = args["id"].as_str().ok_or("id is required")?;
            let items = board.items().await.map_err(msg)?;
            projects::detail(&items, id, APP_URL)
                .ok_or_else(|| "No item with that id. Use projects_find.".to_string())
        }
        "projects_save" => {
            let who = bot_actor(state, args).await?;
            let items = args["items"].as_array().cloned().unwrap_or_default();
            let result = board.save(&items, who).await.map_err(msg)?;
            let saved: Vec<Value> = result["items"]
                .as_array()
                .map(|list| list.as_slice())
                .unwrap_or_default()
                .iter()
                .map(|item| {
                    let mut out = json!({
                        "id": item["id"], "kind": item["kind"], "title": item["title"],
                        "link": projects::item_link(item["id"].as_str().unwrap_or_default(), APP_URL),
                    });
                    if let Some(status) = item["status"].as_str() {
                        out["status"] = json!(status);
                    }
                    out
                })
                .collect();
            let mut out = json!({ "saved": saved });
            if result["unchanged"] == true {
                out["note"] = json!("Nothing changed: those values were already set.");
            }
            Ok(out)
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
            let result = board.delete(&ids, who).await.map_err(msg)?;
            Ok(json!({ "deleted": result["removed"] }))
        }
        other => Err(format!("Unknown tool {other}")),
    }
}

/// A write must come from this app's own page, never from another website using the user's session.
/// Works behind a proxy that forwards `x-forwarded-host`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn same_origin(headers: &HeaderMap) -> bool {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let Some(origin) = header("origin") else {
        return header("sec-fetch-site").is_none_or(|s| s == "same-origin");
    };
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    };
    let forwarded = header("x-forwarded-host")
        .and_then(|f| f.split(',').next())
        .map(str::trim)
        .filter(|f| !f.is_empty());
    header("host") == Some(host.as_str()) || forwarded == Some(host.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn the_tool_list_names_every_projects_tool_with_a_schema() {
        let list = tools();
        let names: Vec<&str> = list.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, NAMES);
        for tool in &list {
            assert_eq!(tool["inputSchema"]["type"], "object", "{}", tool["name"]);
        }
        let save = list.iter().find(|t| t["name"] == "projects_save").unwrap();
        assert_eq!(
            save["inputSchema"]["required"],
            json!(["items", "threadId"])
        );
        assert_eq!(
            save["inputSchema"]["properties"]["items"]["items"]["properties"]["kind"]["enum"],
            json!(KINDS)
        );
        let status = save["inputSchema"]["properties"]["items"]["items"]["properties"]["status"]
            ["description"]
            .as_str()
            .unwrap();
        assert!(status.contains("task: todo|doing|waiting|done"), "{status}");
    }

    #[test]
    fn unknown_names_are_not_projects_tools() {
        assert!(!NAMES.contains(&"vault_list"));
        assert!(NAMES.iter().all(|n| n.starts_with("projects_")));
    }

    #[test]
    fn same_origin_accepts_the_app_and_refuses_other_sites() {
        assert!(same_origin(&headers(&[
            ("host", "bot.example"),
            ("origin", "https://bot.example")
        ])));
        assert!(same_origin(&headers(&[
            ("host", "127.0.0.1:3781"),
            ("x-forwarded-host", "nas.tailnet.ts.net:8443"),
            ("origin", "https://nas.tailnet.ts.net:8443"),
        ])));
        assert!(!same_origin(&headers(&[
            ("host", "bot.example"),
            ("origin", "https://bot.example.evil.test")
        ])));
        assert!(!same_origin(&headers(&[
            ("host", "bot.example"),
            ("origin", "null")
        ])));
        assert!(!same_origin(&headers(&[
            ("host", "bot.example"),
            ("sec-fetch-site", "cross-site")
        ])));
        assert!(same_origin(&headers(&[
            ("host", "bot.example"),
            ("sec-fetch-site", "same-origin")
        ])));
        assert!(
            same_origin(&headers(&[("host", "bot.example")])),
            "no origin header: a non-browser caller"
        );
    }
}
