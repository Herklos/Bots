//! The Bots' own watch tools (`bots/watch_tools.rs`, Phase 4): a Bot can ask to be told
//! when a page changes, with no model call per poll. Identity is the caller's
//! `threadId`, the way the memory tools do it. Creation from a Bot is public hosts
//! only: the SSRF guard runs a preflight resolve, and `allowPrivate` is never set
//! here. It exists for the app's confirmed sheet, and it never carries an auth header.

use crate::{bots::watch, AppState};
use serde_json::{json, Value};

const NAMES: [&str; 3] = ["watch_create", "watch_list", "watch_delete"];

/// The tool json for the Bots' MCP server.
pub(crate) fn tool_defs() -> Vec<Value> {
    let thread = json!({
        "type": "string",
        "description": "Your main conversation's T3 thread id (in AGENTS.md, or mainThreadId in .bot/local.json or .bot/bot.json)."
    });
    vec![
        json!({
            "name": "watch_create",
            "description": "Watch a public page (GET or HEAD) for a change. Check is hash, status, json (a pointer, with op changes, equals, not_equals, gt, or lt), contains, or regex. Fire is on_change (default), on_true, or every, with cooldownSec at least 60. The interval is at least 5 minutes. At most 10 watches per Bot. Private hosts are refused.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "threadId": thread,
                    "name": { "type": "string", "description": "A short name for the watch, shown in the routine list." },
                    "url": { "type": "string", "description": "The public http or https url to poll." },
                    "check": { "type": "object", "description": "{ type: hash | status | json | contains | regex, ... }" },
                    "everyMs": { "type": "number", "description": "The poll interval in milliseconds; at least 300000." },
                    "fire": { "type": "object", "description": "{ mode: on_change | on_true | every, cooldownSec: number }." }
                },
                "required": ["threadId", "name", "url", "check"],
            },
        }),
        json!({
            "name": "watch_list",
            "description": "This Bot's watches: name, target, interval, next check, and the last result.",
            "inputSchema": { "type": "object", "properties": { "threadId": thread }, "required": ["threadId"] },
            "annotations": { "readOnlyHint": true }
        }),
        json!({
            "name": "watch_delete",
            "description": "Delete one of this Bot's watches by id, together with its routine. Deleting the watch deletes the routine.",
            "inputSchema": {
                "type": "object",
                "properties": { "threadId": thread, "id": { "type": "string" } },
                "required": ["threadId", "id"]
            },
        }),
    ]
}

pub(crate) fn is_watch_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

pub(crate) async fn call(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    let roster = state.bots.list().await.map_err(|e| e.message)?;
    let bot = caller(&roster, args)?;
    match name {
        "watch_create" => create(state, bot, args).await,
        "watch_list" => list(state, bot).await,
        "watch_delete" => delete(state, bot, args).await,
        _ => Err(format!("Unknown tool {name}")),
    }
}

/// The Bot whose main thread is `threadId`.
fn caller<'a>(roster: &'a [Value], args: &Value) -> Result<&'a Value, String> {
    let thread = args["threadId"]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or("threadId is required: pass your main thread id from AGENTS.md.")?;
    roster
        .iter()
        .find(|b| b["mainThreadId"] == thread)
        .ok_or_else(|| "That threadId isn't a Bot's main conversation.".to_string())
}

async fn create(state: &AppState, bot: &Value, args: &Value) -> Result<Value, String> {
    let name = args["name"]
        .as_str()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or("Give the watch a name.")?;
    if name.chars().count() > 74 {
        return Err("A watch name is at most 74 characters.".to_string());
    }
    let target = watch::validate_target(
        &json!({ "type": "http", "url": args["url"], "method": args.get("method") }),
    )
    .map_err(str::to_string)?;
    let url = url::Url::parse(target["url"].as_str().unwrap_or(""))
        .map_err(|_bad| "That url does not parse.".to_string())?;
    let host = url.host_str().ok_or("That url has no host.")?.to_string();
    let port = url.port_or_known_default().ok_or("That url has no port.")?;
    // A creation from a Bot is public-host only: resolve now and refuse private ranges.
    // The poller re-checks on every poll, so a host that later points inward is still
    // refused. `allowPrivate` is never set from here.
    watch::guard::resolve_checked(&host, port, false)
        .await
        .map_err(|guard| guard.message())?;
    let check =
        watch::validate_check(args.get("check").unwrap_or(&Value::Null)).map_err(str::to_string)?;
    let every_ms = watch::validate_every_ms(
        args.get("everyMs")
            .unwrap_or(&json!(watch::TOOL_MIN_EVERY_MS)),
        watch::TOOL_MIN_EVERY_MS,
    )
    .map_err(str::to_string)?;
    let fire =
        watch::validate_fire(args.get("fire").unwrap_or(&Value::Null)).map_err(str::to_string)?;

    let mut watch_record = watch::new_record(name, &target, &check, &fire, every_ms);
    let routines = state.harness.routines().map_err(|err| err.to_string())?;
    // The hidden routine; its id comes back and lives in the watch.
    watch::hidden_routine(state, routines, bot, &mut watch_record).await?;
    let slug = bot["slug"].as_str().ok_or("The Bot has no folder.")?;
    let file = watch::WatchFile::new(&state.cfg.bots_root, slug);
    let created = file
        .update(|watches| {
            if watches.len() >= watch::MAX_PER_BOT {
                return Err(format!(
                    "A Bot keeps at most {} watches.",
                    watch::MAX_PER_BOT
                ));
            }
            watches.push(watch_record.clone());
            Ok(())
        })
        .await
        .map_err(|err| err.to_string())?;
    let row = created
        .iter()
        .find(|w| w["id"] == watch_record["id"])
        .map(watch::row)
        .unwrap_or(Value::Null);
    Ok(json!({ "ok": true, "watch": row }))
}

async fn list(state: &AppState, bot: &Value) -> Result<Value, String> {
    let slug = bot["slug"].as_str().ok_or("The Bot has no folder.")?;
    let file = watch::WatchFile::new(&state.cfg.bots_root, slug);
    let rows: Vec<Value> = file
        .watches(&file.load().await)
        .iter()
        .map(watch::row)
        .collect();
    Ok(json!({ "ok": true, "watches": rows }))
}

async fn delete(state: &AppState, bot: &Value, args: &Value) -> Result<Value, String> {
    let id = args["id"]
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or("Pass the watch id from watch_list.")?;
    let slug = bot["slug"].as_str().ok_or("The Bot has no folder.")?;
    let file = watch::WatchFile::new(&state.cfg.bots_root, slug);
    let doc = file.load().await;
    let watches = file.watches(&doc);
    let Some(watch) = watches.iter().find(|w| w["id"].as_str() == Some(id)) else {
        return Err("That watch is not one of yours.".to_string());
    };
    // Deleting the watch deletes the hidden routine with it.
    let routine_id = watch["routineId"].as_str().unwrap_or("").to_string();
    let mut kept = file.watches(&doc);
    kept.retain(|w| w["id"].as_str() != Some(id));
    file.save(&json!({ "version": 1, "watches": kept }))
        .await
        .map_err(|err| err.to_string())?;
    drop(file);
    watch::forget_routine(state, &routine_id).await?;
    Ok(json!({ "ok": true, "deleted": id }))
}
