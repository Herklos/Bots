//! Tool-call rows for one thread projection.
//!
//! Quiet adds nothing. Louder levels add an `activity` array beside `events`.
//! Rows are not messages. The shapes come from one real thread projection in
//! `fixtures/thread-projection.json`: `ThreadSnapshot` from
//! `orchestration.getThreadProjection`. That snapshot's tool items carry
//! `type`, `status`, `ordinal`, and `toolName`, and no command, input, output,
//! or error field. Other field names below are read when a later projection has them.

use serde_json::{json, Value};

#[path = "activity_detail.rs"]
mod detail;

const MAX_GROUPS: usize = 200;
const TARGET_CHARS: usize = 120;

const COMMANDS: usize = 0;
const EDITS: usize = 1;
const SEARCHES: usize = 2;
const BROWSER: usize = 3;
const MEMORY: usize = 4;
const VAULT: usize = 5;
const TOOLS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    Quiet,
    Problems,
    Summary,
    Detail,
}

impl Level {
    pub(crate) fn parse(raw: Option<&str>) -> Self {
        match raw {
            Some("problems") => Self::Problems,
            Some("summary") => Self::Summary,
            Some("detail") => Self::Detail,
            _ => Self::Quiet,
        }
    }

    pub(crate) fn known(raw: Option<&str>) -> bool {
        matches!(raw, Some("quiet" | "problems" | "summary" | "detail"))
    }
}

struct Call {
    id: String,
    label: String,
    target: String,
    status: &'static str,
    bucket: usize,
    vault: bool,
}

struct Group {
    first_id: String,
    at: String,
    ordinal: i64,
    calls: Vec<Call>,
}

/// Groups tool calls between messages. `quiet` is empty.
pub(crate) fn activity(items: &[Value], level: Level) -> Vec<Value> {
    if level == Level::Quiet {
        return Vec::new();
    }
    let mut groups = Vec::new();
    let mut open: Option<Group> = None;
    for (index, raw) in items.iter().enumerate() {
        let item = bare(raw);
        if is_message(&item) {
            if let Some(group) = open.take() {
                groups.push(finish(group, assistant_closer(&item)));
            }
            continue;
        }
        let Some(call) = classify(&item) else {
            continue;
        };
        let ordinal = ordinal_of(&item, index);
        if let Some(group) = open.as_mut() {
            group.ordinal = ordinal;
            group.calls.push(call);
        } else {
            open = Some(Group {
                first_id: call.id.clone(),
                at: at_of(&item),
                ordinal,
                calls: vec![call],
            });
        }
    }
    if let Some(group) = open.take() {
        groups.push(group);
    }
    if groups.len() > MAX_GROUPS {
        let extra = groups.len() - MAX_GROUPS;
        groups.drain(0..extra);
    }
    groups
        .iter()
        .flat_map(|group| rows_for(group, level))
        .collect()
}

/// Adds `activity` when the level is not quiet, and marks in-between replies when asked.
/// Quiet with `final_only` false leaves the transcript untouched.
pub(crate) fn apply_activity(
    transcript: &mut Value,
    items: &[Value],
    level: Level,
    final_only: bool,
) {
    if level == Level::Quiet && !final_only {
        return;
    }
    if final_only {
        hide_finished(transcript);
    }
    if level != Level::Quiet {
        transcript["activity"] = json!(activity(items, level));
    }
}

/// `{ input, output, truncated }` for one call, or `None` when this thread has no such call.
pub(crate) fn call_detail(items: &[Value], id: &str) -> Option<Value> {
    let item = items
        .iter()
        .map(bare)
        .find(|item| item["id"].as_str() == Some(id))?;
    let call = classify(&item)?;
    if call.vault {
        return Some(json!({ "input": "", "output": "", "truncated": false }));
    }
    let (input, input_cut) = detail::clip(&detail::input_of(&item));
    let (mut output, mut output_cut) = detail::clip(&detail::output_of(&item));
    if output.is_empty() {
        let error = item.get("error").cloned().unwrap_or(Value::Null);
        if !error.is_null() {
            (output, output_cut) = detail::clip(&error);
        }
    }
    Some(json!({
        "input": input,
        "output": output,
        "truncated": input_cut || output_cut,
    }))
}

fn rows_for(group: &Group, level: Level) -> Vec<Value> {
    match level {
        Level::Quiet => Vec::new(),
        Level::Problems => group
            .calls
            .iter()
            .filter(|call| call.status == "failed")
            .map(|call| {
                row(
                    &format!("activity-{}", call.id),
                    group,
                    std::slice::from_ref(call),
                    level,
                )
            })
            .collect(),
        Level::Summary | Level::Detail => {
            vec![row(
                &format!("activity-{}", group.first_id),
                group,
                &group.calls,
                level,
            )]
        }
    }
}

fn row(id: &str, group: &Group, calls: &[Call], level: Level) -> Value {
    let mut counts = [0_i64; 7];
    for call in calls {
        counts[call.bucket] += 1;
    }
    json!({
        "id": id,
        "kind": "activity",
        "ordinal": group.ordinal,
        "at": group.at,
        "status": group_status(calls),
        "detail": level == Level::Detail,
        "counts": {
            "commands": counts[COMMANDS],
            "edits": counts[EDITS],
            "searches": counts[SEARCHES],
            "browser": counts[BROWSER],
            "memory": counts[MEMORY],
            "vault": counts[VAULT],
            "tools": counts[TOOLS],
        },
        "calls": calls.iter().map(|call| call_json(call, level)).collect::<Vec<_>>(),
    })
}

fn call_json(call: &Call, level: Level) -> Value {
    let mut body = json!({
        "id": call.id,
        "label": call.label,
        "status": call.status,
    });
    if level == Level::Detail {
        body["target"] = json!(call.target);
    }
    body
}

fn group_status(calls: &[Call]) -> &'static str {
    if calls.iter().any(|call| call.status == "failed") {
        "failed"
    } else if calls.iter().any(|call| call.status == "running") {
        "running"
    } else {
        "ok"
    }
}

fn finish(mut group: Group, closer: Option<&Value>) -> Group {
    if let Some(item) = closer.filter(|item| !super::project::live_item(item)) {
        if let Some(ordinal) = item["ordinal"].as_i64() {
            group.ordinal = ordinal;
        }
    }
    group
}

fn assistant_closer(item: &Value) -> Option<&Value> {
    (item["type"].as_str() == Some("assistant_message")).then_some(item)
}

fn classify(item: &Value) -> Option<Call> {
    if is_message(item) || skipped(item) {
        return None;
    }
    let id = item["id"].as_str().unwrap_or("").to_string();
    let status = call_status(item);
    let (label, target, bucket, vault) = match item["type"].as_str().unwrap_or("") {
        "command_execution" => ("command".to_string(), command_target(item), COMMANDS, false),
        "file_change" => ("edit".to_string(), file_target(item), EDITS, false),
        "web_search" => ("search".to_string(), search_target(item), SEARCHES, false),
        "dynamic_tool" => dynamic(item),
        _ => (
            "tool".to_string(),
            shown_target(item["type"].as_str().unwrap_or("")),
            TOOLS,
            false,
        ),
    };
    Some(Call {
        id,
        label,
        target,
        status,
        bucket,
        vault,
    })
}

fn dynamic(item: &Value) -> (String, String, usize, bool) {
    let tool = super::project::tool_of(item);
    if tool.starts_with("vault_") {
        return ("vault".to_string(), String::new(), VAULT, true);
    }
    if matches!(tool, "memory_read" | "skills_read") {
        return ("memory".to_string(), String::new(), MEMORY, false);
    }
    if tool.starts_with("preview_") || tool.contains("browser") {
        return ("browser".to_string(), url_target(item), BROWSER, false);
    }
    if tool.starts_with("projects_") {
        return (humanise(tool), String::new(), TOOLS, false);
    }
    let name = humanise(tool);
    (name, String::new(), TOOLS, false)
}

fn skipped(item: &Value) -> bool {
    matches!(
        item["type"].as_str(),
        Some(
            "reasoning"
                | "plan"
                | "checkpoint"
                | "error"
                | "approval_request"
                | "user_input_request"
                | "secret_request"
        )
    ) || super::project::event_for(item, &crate::chat::Peers::new(), 0).is_some()
}

fn is_message(item: &Value) -> bool {
    matches!(
        item["type"].as_str(),
        Some("user_message" | "assistant_message")
    )
}

fn call_status(item: &Value) -> &'static str {
    let failed = matches!(
        item["status"].as_str(),
        Some("failed" | "error" | "declined")
    ) || item.get("error").is_some_and(|value| !value.is_null());
    if failed {
        "failed"
    } else if super::project::live_item(item) {
        "running"
    } else {
        "ok"
    }
}

fn command_target(item: &Value) -> String {
    let input = super::project::parsed(&item["input"]);
    shown_target(
        item["command"]
            .as_str()
            .or(item["commandLine"].as_str())
            .or(input["command"].as_str())
            .or(input["cmd"].as_str())
            .unwrap_or(""),
    )
}

fn file_target(item: &Value) -> String {
    let input = super::project::parsed(&item["input"]);
    shown_target(
        item["path"]
            .as_str()
            .or(input["path"].as_str())
            .or(input["filePath"].as_str())
            .or(input["file"].as_str())
            .unwrap_or(""),
    )
}

fn search_target(item: &Value) -> String {
    let input = super::project::parsed(&item["input"]);
    shown_target(
        item["query"]
            .as_str()
            .or(input["query"].as_str())
            .unwrap_or(""),
    )
}

fn url_target(item: &Value) -> String {
    let input = super::project::parsed(&item["input"]);
    shown_target(item["url"].as_str().or(input["url"].as_str()).unwrap_or(""))
}

fn humanise(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        "tool".to_string()
    } else {
        name.replace('_', " ")
    }
}

fn shown_target(raw: &str) -> String {
    let scrubbed = detail::scrub(raw.trim());
    scrubbed.chars().take(TARGET_CHARS).collect()
}

fn hide_finished(transcript: &mut Value) {
    let Some(messages) = transcript.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    let mut index = 0;
    while index < messages.len() {
        if messages[index]["role"] != "bot" {
            index += 1;
            continue;
        }
        let start = index;
        while index < messages.len() && messages[index]["role"] == "bot" {
            index += 1;
        }
        let end = index;
        let closed = end < messages.len() || messages[end - 1]["streaming"] != true;
        if closed && end - start > 1 {
            for message in &mut messages[start..end - 1] {
                message["hidden"] = json!(true);
            }
        }
    }
}

fn bare(raw: &Value) -> Value {
    if raw["item"].is_object() {
        raw["item"].clone()
    } else {
        raw.clone()
    }
}

fn ordinal_of(item: &Value, index: usize) -> i64 {
    item["ordinal"]
        .as_i64()
        .unwrap_or_else(|| i64::try_from(index).unwrap_or(i64::MAX))
}

fn at_of(item: &Value) -> String {
    item["startedAt"]
        .as_str()
        .or(item["updatedAt"].as_str())
        .unwrap_or("")
        .to_string()
}
