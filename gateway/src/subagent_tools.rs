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
pub struct Deps<'a> {
    pub bots: &'a Bots,
    /// The Bots, as `bots.list()` gives them (each has `mainThreadId`).
    pub roster: &'a [Value],
    pub provision_now: &'a Notify,
}

pub fn tool_defs() -> Vec<Value> {
    let thread = json!({
        "type": "string",
        "description": "Your main conversation's T3 thread id (in AGENTS.md, or mainThreadId in .bot/bot.json)."
    });
    vec![
        json!({
            "name": "subagent_list",
            "description": "List your durable subagents: slug, name, prompt, whether they inherit your prompt, and mainThreadId (null until their thread has started). A one-off child thread of this turn is not listed here.",
            "inputSchema": {
                "type": "object",
                "properties": { "threadId": thread },
                "required": ["threadId"],
            },
        }),
        json!({
            "name": "subagent_create",
            "description": "Create a lasting subagent: another Bot that keeps its own folder, project and main thread, with your face. Pass the standing prompt. An optional task is the first job, queued once as a brief, not copied into the prompt. Work that ends with this turn stays a T3 child thread instead. mainThreadId is null until the thread starts; read it later with subagent_list.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "threadId": thread,
                    "prompt": { "type": "string", "description": "What this subagent is for. Required, at most 2000 characters." },
                    "name": { "type": "string", "description": "Optional. Blank uses \"{your name} junior N\"." },
                    "inheritParentPrompt": { "type": "boolean", "description": "Copy your prompt into the subagent's instructions. Default false." },
                    "task": { "type": "string", "description": "Optional first job, at most 2000 characters. Not copied into the prompt." },
                },
                "required": ["threadId", "prompt"],
            },
        }),
    ]
}

pub fn is_subagent_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

/// Entry point from `mcp.rs`.
pub async fn call(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    let roster = state.bots.list().await.map_err(|e| e.message)?;
    dispatch(
        &Deps {
            bots: &state.bots,
            roster: &roster,
            provision_now: &state.provision_now,
        },
        name,
        args,
    )
    .await
}

pub async fn dispatch(d: &Deps<'_>, name: &str, args: &Value) -> Result<Value, String> {
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
        .ok_or_else(|| "That threadId isn't a Bot's main conversation. Pass your main thread id from AGENTS.md (or mainThreadId in .bot/bot.json).".to_string())
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
    Ok(json!({ "subagents": subagents }))
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
    let inherit = match args.get("inheritParentPrompt") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(v)) => *v,
        Some(_) => return Err("inheritParentPrompt must be a boolean.".into()),
    };
    let parent_slug = parent["slug"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("That threadId isn't a Bot's main conversation. Pass your main thread id from AGENTS.md (or mainThreadId in .bot/bot.json).")?;
    let mut input = json!({
        "parentSlug": parent_slug,
        "subagent": { "prompt": prompt, "inheritParentPrompt": inherit },
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
    d.provision_now.notify_one();
    Ok(json!({
        "slug": bot["slug"],
        "name": bot["name"],
        "mainThreadId": Value::Null,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    async fn world() -> (tempfile::TempDir, Bots, Notify) {
        let dir = tempfile::tempdir().unwrap();
        let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        bots.create(&json!({ "name": "Ada", "prompt": "Be brief." }))
            .await
            .unwrap();
        bots.save("ada", &json!({ "mainThreadId": "thread-ada" }), false)
            .await
            .unwrap();
        bots.create(&json!({ "name": "Bea" })).await.unwrap();
        bots.save("bea", &json!({ "mainThreadId": "thread-bea" }), false)
            .await
            .unwrap();
        bots.create(&json!({
            "parentSlug": "bea",
            "name": "Bea helper",
            "subagent": { "prompt": "Bea's notes." }
        }))
        .await
        .unwrap();
        (dir, bots, Notify::new())
    }

    fn deps<'a>(bots: &'a Bots, roster: &'a [Value], notify: &'a Notify) -> Deps<'a> {
        Deps {
            bots,
            roster,
            provision_now: notify,
        }
    }

    #[tokio::test]
    async fn a_parent_creates_a_child_and_a_subagent_caller_is_refused() {
        let (dir, bots, notify) = world().await;
        let roster = bots.list().await.unwrap();
        let created = dispatch(
            &deps(&bots, &roster, &notify),
            "subagent_create",
            &json!({
                "threadId": "thread-ada",
                "prompt": "Look things up.",
                "parentSlug": "bea",
                "toolAllow": ["read"],
                "shape": "cloud",
                "color": "#ffffff",
                "model": "opus",
            }),
        )
        .await
        .unwrap();
        assert_eq!(created["slug"], "ada-junior-1");
        assert_eq!(created["name"], "Ada junior 1");
        assert!(created["mainThreadId"].is_null());
        assert!(
            tokio::time::timeout(Duration::from_secs(1), notify.notified())
                .await
                .is_ok()
        );
        let child = bots.get("ada-junior-1").await.unwrap().unwrap();
        assert_eq!(child["parentSlug"], "ada");
        assert_eq!(child["subagent"]["prompt"], "Look things up.");
        assert_eq!(child["subagent"]["inheritParentPrompt"], false);
        assert!(child["subagent"].get("toolAllow").is_none());
        assert!(child["model"].is_null());
        assert_eq!(
            child["shape"],
            bots.get("ada").await.unwrap().unwrap()["shape"]
        );
        assert!(child.get("firstMessage").is_none() || child["firstMessage"].is_null());
        let agents = std::fs::read_to_string(dir.path().join("ada-junior-1/AGENTS.md")).unwrap();
        assert!(agents.contains("Look things up."));

        bots.save(
            "ada-junior-1",
            &json!({ "mainThreadId": "thread-junior" }),
            false,
        )
        .await
        .unwrap();
        let roster = bots.list().await.unwrap();
        let err = dispatch(
            &deps(&bots, &roster, &notify),
            "subagent_create",
            &json!({ "threadId": "thread-junior", "prompt": "Nested.", "parentSlug": "ada" }),
        )
        .await
        .unwrap_err();
        assert!(err.contains("subagent"), "{err}");
        assert!(bots.get("ada-junior-1-junior-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_forged_parent_slug_is_ignored_and_list_is_only_this_parents_children() {
        let (_dir, bots, notify) = world().await;
        let roster = bots.list().await.unwrap();
        dispatch(
            &deps(&bots, &roster, &notify),
            "subagent_create",
            &json!({
                "threadId": "thread-ada",
                "prompt": "Ada's lookup.",
                "name": "Lookup",
                "parentSlug": "bea",
            }),
        )
        .await
        .unwrap();
        let child = bots.get("lookup").await.unwrap().unwrap();
        assert_eq!(child["parentSlug"], "ada");
        assert_eq!(child["name"], "Lookup");

        let roster = bots.list().await.unwrap();
        let listed = dispatch(
            &deps(&bots, &roster, &notify),
            "subagent_list",
            &json!({ "threadId": "thread-ada", "parentSlug": "bea" }),
        )
        .await
        .unwrap();
        assert_eq!(listed["subagents"].as_array().unwrap().len(), 1);
        assert_eq!(listed["subagents"][0]["slug"], "lookup");
        assert_eq!(listed["subagents"][0]["name"], "Lookup");
        assert_eq!(listed["subagents"][0]["prompt"], "Ada's lookup.");
        assert_eq!(listed["subagents"][0]["inheritParentPrompt"], false);
        assert!(listed["subagents"][0]["mainThreadId"].is_null());
        let text = listed.to_string();
        assert!(!text.contains("workspace"));
        assert!(!text.contains("token"));
        assert!(!text.contains("bot.json"));

        let bea = dispatch(
            &deps(&bots, &roster, &notify),
            "subagent_list",
            &json!({ "threadId": "thread-bea" }),
        )
        .await
        .unwrap();
        assert_eq!(bea["subagents"].as_array().unwrap().len(), 1);
        assert_eq!(bea["subagents"][0]["slug"], "bea-helper");
        assert_eq!(bea["subagents"][0]["prompt"], "Bea's notes.");
    }

    #[tokio::test]
    async fn an_empty_prompt_is_refused_and_overlong_text_is_an_error() {
        let (_dir, bots, notify) = world().await;
        let roster = bots.list().await.unwrap();
        let d = deps(&bots, &roster, &notify);
        let missing = dispatch(&d, "subagent_create", &json!({ "threadId": "thread-ada" }))
            .await
            .unwrap_err();
        assert!(missing.contains("prompt"), "{missing}");
        for prompt in [json!(""), json!("   "), json!(null)] {
            let err = dispatch(
                &d,
                "subagent_create",
                &json!({ "threadId": "thread-ada", "prompt": prompt }),
            )
            .await
            .unwrap_err();
            assert!(err.contains("prompt"), "{err}");
        }
        let long = "x".repeat(2001);
        let err = dispatch(
            &d,
            "subagent_create",
            &json!({ "threadId": "thread-ada", "prompt": long }),
        )
        .await
        .unwrap_err();
        assert!(err.contains("2000"), "{err}");
        let err = dispatch(
            &d,
            "subagent_create",
            &json!({ "threadId": "thread-ada", "prompt": "Ok.", "task": "y".repeat(2001) }),
        )
        .await
        .unwrap_err();
        assert!(err.contains("task"), "{err}");
        assert!(bots.get("ada-junior-1").await.unwrap().is_none());
        let err = dispatch(
            &d,
            "subagent_create",
            &json!({ "threadId": "missing", "prompt": "Ok." }),
        )
        .await
        .unwrap_err();
        assert!(err.contains("threadId"), "{err}");
    }

    #[tokio::test]
    async fn task_lands_as_brief_plus_created_by() {
        let (dir, bots, notify) = world().await;
        let roster = bots.list().await.unwrap();
        dispatch(
            &deps(&bots, &roster, &notify),
            "subagent_create",
            &json!({
                "threadId": "thread-ada",
                "prompt": "  File the notes.  ",
                "name": "   ",
                "inheritParentPrompt": true,
                "task": "  Sort today's mail.  ",
            }),
        )
        .await
        .unwrap();
        let child = bots.get("ada-junior-1").await.unwrap().unwrap();
        assert_eq!(child["name"], "Ada junior 1");
        assert_eq!(child["brief"], "Sort today's mail.");
        assert_eq!(child["createdBy"], "Ada");
        assert_eq!(child["subagent"]["prompt"], "File the notes.");
        assert_eq!(child["subagent"]["inheritParentPrompt"], true);
        assert!(child["firstMessage"].is_null());
        let raw = std::fs::read_to_string(dir.path().join("ada-junior-1/.bot/bot.json")).unwrap();
        assert!(raw.contains("Sort today's mail."));
        assert!(raw.contains("\"createdBy\": \"Ada\"") || raw.contains("\"createdBy\":\"Ada\""));
        let agents = std::fs::read_to_string(dir.path().join("ada-junior-1/AGENTS.md")).unwrap();
        assert!(agents.contains("File the notes."));
        assert!(!agents.contains("Sort today's mail."));
    }
}
