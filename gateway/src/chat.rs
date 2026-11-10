//! A small REST face on a Bot's main thread for clients that don't speak the harness RPC,
//! such as the Exact app's `app.ts`: read the conversation, send a message, stop a run, answer
//! an approval. T3 Code is the harness wired today; other backends may be added. The `/t3/*`
//! passthrough is that backend's door.
use serde_json::{json, Value};
use std::collections::HashMap;

const LIVE: &[&str] = &[
    "running",
    "in_progress",
    "pending",
    "queued",
    "streaming",
    "starting",
];

/// Bots by main thread: `{slug, name, color, shape}`. It turns a thread id in a tool call or a
/// sender into the Bot the person knows.
pub type Peers = HashMap<String, Value>;

/// The last segment of a tool name: `mcp__t3-code__schedule_task` is `schedule_task`.
fn tool_of(item: &Value) -> &str {
    let raw = item["toolName"].as_str().unwrap_or("");
    raw.rsplit("__").next().unwrap_or(raw)
}

/// T3 sends some fields as JSON text; anything unreadable reads as an empty object.
fn parsed(value: &Value) -> Value {
    match value {
        Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
        Value::Null => json!({}),
        other => other.clone(),
    }
}

/// The files of a message, as a card needs them.
fn files_of(attachments: &[Value]) -> Vec<Value> {
    attachments
        .iter()
        .map(|a| {
            let mime = a["mimeType"].as_str().unwrap_or("");
            let kind = if a["type"] == "image" || mime.starts_with("image/") {
                "image"
            } else if a["type"] == "audio" || mime.starts_with("audio/") {
                "audio"
            } else {
                "file"
            };
            json!({
                "id": a["id"], "name": a["name"].as_str().or(a["fileName"].as_str()).unwrap_or("Attachment"),
                "mime": mime, "bytes": a["sizeBytes"].as_u64().unwrap_or(0), "kind": kind,
            })
        })
        .collect()
}

fn peer(peers: &Peers, thread: &Value) -> Value {
    thread
        .as_str()
        .and_then(|t| peers.get(t))
        .cloned()
        .unwrap_or(Value::Null)
}

/// A line the transcript shows between messages: who the Bot messaged, who messaged it, a
/// routine made or run, a one-off child thread started. Anything else in a turn is not for the person.
fn event_for(item: &Value, peers: &Peers, ordinal: i64) -> Option<Value> {
    let id = format!("event-{}", item["id"].as_str().unwrap_or(""));
    let at = item["startedAt"]
        .as_str()
        .or(item["updatedAt"].as_str())
        .unwrap_or("");
    let line = |kind: &str, text: &str, title: &str, bot: Value| {
        Some(
            json!({ "id": id, "ordinal": ordinal, "at": at, "kind": kind, "text": text, "title": title, "bot": bot }),
        )
    };
    match item["type"].as_str()? {
        "dynamic_tool" if item["status"] == "completed" => {
            let input = parsed(&item["input"]);
            match tool_of(item) {
                "t3_thread_send" => line("sent", "Messaged", "", peer(peers, &input["threadId"])),
                "schedule_task" if input["scheduledTaskId"].is_null() => line(
                    "routine",
                    "Created routine",
                    input["title"].as_str().unwrap_or(""),
                    Value::Null,
                ),
                "schedule_task" | "update_scheduled_task" => line(
                    "routine",
                    "Updated routine",
                    input["title"].as_str().unwrap_or(""),
                    Value::Null,
                ),
                "delete_scheduled_task" => line("routine", "Deleted routine", "", Value::Null),
                _ => None,
            }
        }
        "user_message" if item["senderThreadId"].is_string() => line(
            "from",
            "Message from",
            "",
            peer(peers, &item["senderThreadId"]),
        ),
        "user_message" if item["scheduledTaskId"].is_string() => {
            line("ran", "Routine ran", "", Value::Null)
        }
        // A T3 child thread for this turn, not a lasting hbots Subagent.
        "subagent" => line(
            "started",
            "Started child thread",
            item["title"].as_str().unwrap_or("one-off work"),
            Value::Null,
        ),
        _ => None,
    }
}

/// What a pending request needs to be answered from the app: its id for the respond route
/// (not the item's own id) and the choices it offers.
fn request_for(item: &Value) -> Value {
    let kind = item["type"].as_str().unwrap_or("");
    let item_id = item["id"].clone();
    let id = item["requestId"]
        .as_str()
        .map(Value::from)
        .unwrap_or_else(|| item_id.clone());
    match kind {
        "approval_request" => json!({
            "id": id, "itemId": item_id, "kind": kind, "runId": item["runId"],
            "title": item["prompt"].as_str().or(item["title"].as_str()).unwrap_or("The Bot is asking"),
            "reason": item["reason"].as_str().unwrap_or(""),
            "requestKind": item["requestKind"].as_str().unwrap_or(""),
            "options": item["options"].as_array().cloned().unwrap_or_default(),
        }),
        "user_input_request" => {
            let questions = parsed(&item["questions"]);
            let questions = questions.as_array().cloned().unwrap_or_default();
            let first = questions
                .first()
                .and_then(|q| q["question"].as_str())
                .or(item["title"].as_str())
                .unwrap_or("The Bot is asking");
            json!({
                "id": id, "itemId": item_id, "kind": kind, "runId": item["runId"],
                "title": first, "questions": questions,
            })
        }
        _ => json!({
            "id": item_id, "itemId": item_id, "kind": kind, "runId": item["runId"],
            "title": item["label"].as_str().or(item["title"].as_str()).unwrap_or("The Bot is asking"),
            "reason": item["reason"].as_str().unwrap_or(""),
            "placeholder": item["placeholder"].as_str().unwrap_or(""),
        }),
    }
}

/// Drops framed turns from a one-to-one chat: a group dispatch (`[Group "…`) or a Computer
/// open nudge (`[Computer]`), plus everything the Bot did in answer, up to the next ordinary
/// message from the person. Group rooms use [`group_transcript`] and keep their own frames.
pub fn without_group_turns(items: Vec<Value>) -> Vec<Value> {
    let mut hidden = false;
    items
        .into_iter()
        .filter(|raw| {
            let item = if raw["item"].is_object() {
                &raw["item"]
            } else {
                raw
            };
            if item["type"] == "user_message" {
                hidden = item["text"].as_str().is_some_and(is_framed_prompt);
            }
            !hidden
        })
        .collect()
}

fn is_framed_prompt(text: &str) -> bool {
    is_group_prompt(text) || is_computer_prompt(text)
}

fn is_group_prompt(text: &str) -> bool {
    text.contains("[Group \"")
        || text.contains("Your reply is shared with the group")
        || text.contains("Write @slug only when")
}

fn is_computer_prompt(text: &str) -> bool {
    text.contains("[Computer]")
}

fn turn_items(projection: &Value) -> Vec<Value> {
    projection["turnItems"]
        .as_array()
        .or_else(|| projection["visibleTurnItems"].as_array())
        .cloned()
        .unwrap_or_default()
}

/// The bubbles and cards of a thread projection, oldest first.
pub fn transcript(projection: &Value) -> Value {
    transcript_for(projection, &Peers::new())
}

/// Like [`transcript`], with the lines between messages (`events`) and each message's files.
/// `ordinal` orders messages and events together.
pub fn transcript_for(projection: &Value, peers: &Peers) -> Value {
    project_turns(
        &without_group_turns(turn_items(projection)),
        peers,
        projection,
    )
}

/// The same shape as [`transcript`], for a thread that is itself the group.
/// A group thread has no one-to-one sender line, so this takes no peers.
/// The one-to-one filter stays on [`transcript`]: it would drop the framed user message and the reply.
pub fn group_transcript(projection: &Value) -> Value {
    project_turns(&turn_items(projection), &Peers::new(), projection)
}

fn project_turns(items: &[Value], peers: &Peers, projection: &Value) -> Value {
    let mut messages = Vec::new();
    let mut events = Vec::new();
    let mut requests = Vec::new();
    let mut working = false;
    for (index, raw) in items.iter().enumerate() {
        let item = if raw["item"].is_object() {
            raw["item"].clone()
        } else {
            raw.clone()
        };
        let kind = item["type"].as_str().unwrap_or("");
        let status = item["status"].as_str().unwrap_or("");
        let live = LIVE.contains(&status) || item["streaming"] == true;
        let ordinal = item["ordinal"].as_i64().unwrap_or(index as i64);
        if let Some(event) = event_for(&item, peers, ordinal) {
            events.push(event);
        }
        match kind {
            "user_message" | "assistant_message" => {
                let text = item["text"].as_str().unwrap_or("").trim().to_string();
                let attachments = item["attachments"].as_array().cloned().unwrap_or_default();
                if text.is_empty() && attachments.is_empty() && !live {
                    continue;
                }
                let sender = peer(peers, &item["senderThreadId"]);
                messages.push(json!({
                    "id": item["id"], "role": if kind == "user_message" { "user" } else { "bot" }, "text": text,
                    "files": files_of(&attachments), "attachments": attachments, "ordinal": ordinal,
                    "from": sender,
                    "at": item["startedAt"].as_str().or(item["updatedAt"].as_str()).unwrap_or(""), "streaming": live,
                }));
            }
            "approval_request" | "user_input_request" | "secret_request"
                if live || status == "awaiting_response" || status == "waiting" =>
            {
                requests.push(request_for(&item));
            }
            _ => {}
        }
        working |= live && kind != "user_message";
    }
    let runs = projection["runs"].as_array().cloned().unwrap_or_default();
    let active = runs
        .iter()
        .rev()
        .find(|r| r["status"].as_str().is_some_and(|s| LIVE.contains(&s)))
        .and_then(|r| r["id"].as_str().map(String::from));
    json!({ "messages": messages, "events": events, "requests": requests, "working": working || active.is_some(), "activeRunId": active })
}

// Nothing in the gateway calls this yet.
#[allow(dead_code)]
pub const LOADED_SENTENCE: &str = "Search covers the loaded conversation.";

/// Filters messages already loaded from a projection. Nothing else is searched.
// Nothing in the gateway calls this yet.
#[allow(dead_code)]
pub fn search_messages(messages: &[Value], query: &str, partial: bool) -> Value {
    let query = query.trim().to_lowercase();
    let hits: Vec<Value> = if query.is_empty() {
        Vec::new()
    } else {
        messages
            .iter()
            .filter(|message| {
                message["text"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&query)
            })
            .cloned()
            .collect()
    };
    json!({
        "messages": hits,
        "note": if partial && !query.is_empty() { LOADED_SENTENCE } else { "" },
    })
}

pub fn projection_partial(projection: &Value) -> bool {
    projection["truncated"] == true
        || projection["hasMore"] == true
        || projection["partial"] == true
}

/// Adds a token line only where [`crate::usage::turn_token_line`] finds a shared id.
pub fn with_tokens(mut transcript: Value, projection: &Value) -> Value {
    if let Some(messages) = transcript.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages.iter_mut() {
            let id = message["id"].as_str().unwrap_or("");
            message["tokens"] =
                crate::usage::turn_token_line(projection, id).unwrap_or(Value::Null);
        }
    }
    transcript["partial"] = json!(projection_partial(projection));
    transcript
}

/// Writes the clip as an attachment, records the duration, and builds the dispatch.
/// `voiceCalls` stays a time and a duration. The command gains no quote field.
pub async fn store_voice_send(
    root: &std::path::Path,
    slug: &str,
    threads: &crate::thread_meta::ThreadMeta,
    thread: &str,
    bytes: &[u8],
    seconds: f64,
    at: &str,
) -> crate::error::ApiResult<Value> {
    let meta = crate::attachments::write_file(root, slug, "voice.m4a", "audio/mp4", bytes).await?;
    let seen = threads
        .add_voice_call(Some(thread), Some(&json!(seconds)), at)
        .await?;
    let command = send_command(thread, "", &[crate::attachments::command_part(&meta)]);
    Ok(json!({
        "attachment": meta,
        "voiceCalls": seen["voiceCalls"],
        "command": command,
    }))
}

const VOICE_TYPES: &[&str] = &["audio/mp4", "audio/webm", "audio/mpeg", "audio/wav"];

/// Dispatches a clip that is already stored. It does not write a second file.
/// A file that is not audio is refused. The command gains no quote field.
pub async fn dispatch_stored_voice(
    root: &std::path::Path,
    slug: &str,
    threads: &crate::thread_meta::ThreadMeta,
    thread: &str,
    attachment_id: &str,
    seconds: f64,
    at: &str,
) -> crate::error::ApiResult<Value> {
    let (meta, _) = crate::attachments::read_file(root, slug, attachment_id).await?;
    let media = meta["mediaType"]
        .as_str()
        .unwrap_or("")
        .to_ascii_lowercase();
    if !VOICE_TYPES.contains(&media.as_str()) {
        return Err(crate::error::ApiError::bad_request(
            "That file is not a voice clip.",
        ));
    }
    let seen = threads
        .add_voice_call(Some(thread), Some(&json!(seconds)), at)
        .await?;
    let command = send_command(thread, "", &[crate::attachments::command_part(&meta)]);
    Ok(json!({
        "attachment": meta,
        "voiceCalls": seen["voiceCalls"],
        "command": command,
    }))
}

/// The command a phone send dispatches.
/// `attachments` is the gateway's file records (`attachments::command_part`).
pub fn send_command(thread: &str, text: &str, attachments: &[Value]) -> Value {
    let id = uuid::Uuid::new_v4().to_string();
    json!({
        "commandId": id, "type": "message.dispatch", "threadId": thread, "messageId": id, "text": text,
        "attachments": attachments, "createdBy": "user", "creationSource": "web",
        "deliveryIntent": "steer", "dispatchMode": { "type": "start_immediately" },
    })
}

/// A send aimed at a child. The command is built only after the child resolves, so a remote
/// child or the parent id produces no command and does not write the parent. No quote field.
pub fn child_send(
    rows: &[Value],
    parent: &str,
    asked: &str,
    text: &str,
    attachments: &[Value],
) -> Result<Value, &'static str> {
    let thread = crate::lineage::thread_for(rows, parent, Some(asked))?;
    Ok(send_command(&thread, text, attachments))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubbles_from_turn_items() {
        let p = json!({
            "turnItems": [
                { "id": "1", "type": "user_message", "text": "hi", "status": "completed", "startedAt": "t1" },
                { "id": "2", "type": "tool_call", "status": "completed" },
                { "id": "3", "type": "assistant_message", "text": "hello", "status": "running", "streaming": true },
                { "id": "4", "type": "approval_request", "status": "pending", "title": "Run rm?" },
            ],
            "runs": [{ "id": "r1", "status": "running" }],
        });
        let t = transcript(&p);
        assert_eq!(t["messages"].as_array().unwrap().len(), 2);
        assert_eq!(t["messages"][1]["role"], "bot");
        assert_eq!(t["requests"][0]["title"], "Run rm?");
        assert_eq!(
            (t["working"].as_bool(), t["activeRunId"].as_str()),
            (Some(true), Some("r1"))
        );
    }

    fn fixture(name: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/t3")
            .join(name);
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn a_real_thread_gives_messages_with_times_and_no_noise_events() {
        let p = fixture("thread-projection.json");
        let t = transcript(&p["value"]);
        assert_eq!(t["messages"].as_array().unwrap().len(), 6);
        assert!(t["messages"][0]["at"]
            .as_str()
            .unwrap()
            .starts_with("2026-"));
        assert!(
            t["events"].as_array().unwrap().is_empty(),
            "reads, commands and checkpoints are not lines"
        );
        assert_eq!(t["working"], false);
    }

    #[test]
    fn a_routine_made_by_a_bot_is_a_line_with_its_title() {
        let made = fixture("routine-created.json");
        let item = json!({ "id": "i1", "ordinal": 7, "type": "dynamic_tool", "status": "completed", "startedAt": "2026-11-10T13:05:00Z",
            "toolName": made["toolItem"]["toolName"], "input": made["toolItem"]["input"], "output": made["toolItem"]["output"] });
        let t = transcript(&json!({ "turnItems": [item] }));
        let e = &t["events"][0];
        assert_eq!(
            (e["kind"].as_str(), e["text"].as_str(), e["title"].as_str()),
            (Some("routine"), Some("Created routine"), Some("hbots test"))
        );
        assert_eq!(e["ordinal"], 7);
        // An update carries the id of the routine it changes.
        let update = json!({ "id": "i2", "type": "dynamic_tool", "status": "completed", "toolName": "mcp__t3-code__schedule_task",
            "input": { "scheduledTaskId": "scheduled-task:1", "title": "Morning" } });
        assert_eq!(
            transcript(&json!({ "turnItems": [update] }))["events"][0]["text"],
            "Updated routine"
        );
    }

    #[test]
    fn messages_between_bots_name_the_bot() {
        let mut peers = Peers::new();
        peers.insert(
            "thread-inbox".into(),
            json!({ "slug": "inbox", "name": "Inbox", "color": "brand.pink", "shape": "hex" }),
        );
        let sent = json!({ "id": "a", "type": "dynamic_tool", "status": "completed", "toolName": "mcp__t3__t3_thread_send",
            "input": "{\"threadId\":\"thread-inbox\",\"message\":\"Draft ready\"}" });
        let got = json!({ "id": "b", "type": "user_message", "status": "completed", "text": "Sent you a file", "senderThreadId": "thread-inbox" });
        let t = transcript_for(&json!({ "turnItems": [sent, got] }), &peers);
        assert_eq!(t["events"][0]["text"], "Messaged");
        assert_eq!(t["events"][0]["bot"]["name"], "Inbox");
        assert_eq!(t["events"][1]["text"], "Message from");
        assert_eq!(t["messages"][0]["from"]["slug"], "inbox");
        // A message from the person has no sender.
        let mine = transcript(
            &json!({ "turnItems": [{ "id": "c", "type": "user_message", "status": "completed", "text": "hi" }] }),
        );
        assert_eq!(mine["messages"][0]["from"], Value::Null);
    }

    #[test]
    fn a_retrying_error_and_a_routine_run_are_handled() {
        let made = fixture("routine-created.json");
        let retry =
            json!({ "id": "e", "type": "error", "status": "completed", "retry": { "attempt": 4 } });
        assert!(transcript(&json!({ "turnItems": [retry] }))["events"]
            .as_array()
            .unwrap()
            .is_empty());
        let ran = json!({ "id": "r", "type": "user_message", "status": "completed", "text": "test only", "scheduledTaskId": made["taskRow"]["id"] });
        assert_eq!(
            transcript(&json!({ "turnItems": [ran] }))["events"][0]["text"],
            "Routine ran"
        );
    }

    #[test]
    fn requests_carry_the_id_the_respond_route_needs_and_their_choices() {
        let approval = json!({ "id": "item-1", "requestId": "req-1", "type": "approval_request", "status": "pending", "prompt": "Run ls?",
            "requestKind": "command", "options": [{ "decision": "accept", "label": "Allow" }, { "decision": "decline", "label": "Deny" }] });
        let question = json!({ "id": "item-2", "requestId": "req-2", "type": "user_input_request", "status": "pending",
            "questions": [{ "id": "q1", "header": "Pick", "question": "Which one?", "options": [{ "label": "A", "description": "a" }] }] });
        let secret = json!({ "id": "item-3", "type": "secret_request", "status": "pending", "label": "API key", "reason": "To call the API", "placeholder": "sk-" });
        let t = transcript(&json!({ "turnItems": [approval, question, secret] }));
        let r = t["requests"].as_array().unwrap();
        assert_eq!(
            (
                r[0]["id"].as_str(),
                r[0]["itemId"].as_str(),
                r[0]["title"].as_str()
            ),
            (Some("req-1"), Some("item-1"), Some("Run ls?"))
        );
        assert_eq!(r[0]["options"][1]["decision"], "decline");
        assert_eq!(
            (r[1]["id"].as_str(), r[1]["title"].as_str()),
            (Some("req-2"), Some("Which one?"))
        );
        assert_eq!(r[1]["questions"][0]["id"], "q1");
        assert_eq!(
            (
                r[2]["id"].as_str(),
                r[2]["title"].as_str(),
                r[2]["placeholder"].as_str()
            ),
            (Some("item-3"), Some("API key"), Some("sk-"))
        );
    }

    #[test]
    fn an_attachment_with_no_text_is_still_a_message() {
        let p = json!({
            "turnItems": [{
                "id": "1", "type": "user_message", "text": "", "status": "completed",
                "attachments": [{ "type": "file", "name": "notes.txt" }],
            }],
        });
        let t = transcript(&p);
        assert_eq!(t["messages"][0]["attachments"][0]["name"], "notes.txt");
        let command = send_command(
            "thread",
            "",
            &t["messages"][0]["attachments"].as_array().cloned().unwrap(),
        );
        assert_eq!(command["attachments"][0]["name"], "notes.txt");
        assert_eq!(command["text"], "");
    }

    #[test]
    fn search_returns_only_matching_loaded_messages() {
        let messages = vec![
            json!({ "id": "1", "text": "Hello Ada" }),
            json!({ "id": "2", "text": "goodbye" }),
        ];
        let hit = search_messages(&messages, "ada", true);
        assert_eq!(hit["messages"].as_array().unwrap().len(), 1);
        assert_eq!(hit["messages"][0]["id"], "1");
        assert_eq!(hit["note"], LOADED_SENTENCE);
        assert_eq!(search_messages(&messages, "ada", false)["note"], "");
        assert!(search_messages(&messages, "missing", true)["messages"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            search_messages(&messages, "missing", true)["note"],
            LOADED_SENTENCE
        );
    }

    #[test]
    fn a_token_line_is_attached_only_for_a_shared_id() {
        let projection = json!({
            "partial": true,
            "providerTurns": [{
                "id": "m2",
                "turnTokenUsage": {
                    "inputTokens": 10, "outputTokens": 2,
                    "cachedInputTokens": 4, "cacheCreationTokens": 1
                }
            }],
            "turnItems": [
                { "id": "m1", "type": "user_message", "text": "hi", "status": "completed" },
                { "id": "m2", "type": "assistant_message", "text": "hello", "status": "completed" }
            ]
        });
        let out = with_tokens(transcript(&projection), &projection);
        assert_eq!(out["partial"], true);
        assert!(out["messages"][0]["tokens"].is_null());
        assert_eq!(out["messages"][1]["tokens"]["inputTokens"], 10);
        assert_eq!(out["messages"][1]["tokens"]["cachedInputTokens"], 4);
        assert_ne!(
            out["messages"][1]["tokens"]["inputTokens"]
                .as_u64()
                .unwrap(),
            14
        );
    }

    #[tokio::test]
    async fn a_voice_send_stores_bytes_as_an_attachment_and_a_duration() {
        let dir = tempfile::tempdir().unwrap();
        let threads = crate::thread_meta::ThreadMeta::new(dir.path().join("thread-meta.json"));
        let bytes = b"fake-audio-bytes";
        let out = store_voice_send(
            dir.path(),
            "ada",
            &threads,
            "thread-1",
            bytes,
            12.2,
            "2026-11-10T00:00:00.000Z",
        )
        .await
        .unwrap();
        let id = out["attachment"]["id"].as_str().unwrap();
        let (_meta, disk) = crate::attachments::read_file(dir.path(), "ada", id)
            .await
            .unwrap();
        assert_eq!(disk, bytes);
        assert_eq!(out["voiceCalls"].as_array().unwrap().len(), 1);
        assert_eq!(out["voiceCalls"][0]["at"], "2026-11-10T00:00:00.000Z");
        assert_eq!(out["voiceCalls"][0]["seconds"], 12);
        assert!(out["voiceCalls"][0].get("audio").is_none());
        let raw = serde_json::to_string(&out["voiceCalls"]).unwrap();
        assert!(!raw.contains("fake-audio"));
        assert!(out["command"].get("quote").is_none());
        assert_eq!(out["command"]["attachments"][0]["id"], id);
        assert_eq!(out["command"]["type"], "message.dispatch");
        assert_eq!(out["attachment"]["mediaType"], "audio/mp4");
        assert_eq!(
            crate::attachments::read_dir(dir.path(), "ada")
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn an_audio_attachment_plays_as_audio() {
        let out = transcript(&json!({
            "turnItems": [{
                "id": "v",
                "type": "user_message",
                "text": "",
                "status": "completed",
                "attachments": [
                    { "id": "voice", "name": "voice.m4a", "mimeType": "audio/mp4", "sizeBytes": 12, "type": "file" },
                    { "id": "note", "name": "n.txt", "mimeType": "text/plain", "sizeBytes": 1, "type": "file" }
                ]
            }]
        }));
        let files = &out["messages"][0]["files"];
        assert_eq!(files[0]["kind"], "audio");
        assert_eq!(files[1]["kind"], "file");
    }

    #[tokio::test]
    async fn a_short_clip_stays_an_attachment_the_player_can_use() {
        let dir = tempfile::tempdir().unwrap();
        let threads = crate::thread_meta::ThreadMeta::new(dir.path().join("thread-meta.json"));
        let bytes = b"short-clip";
        let out = store_voice_send(
            dir.path(),
            "ada",
            &threads,
            "thread-1",
            bytes,
            4.2,
            "2026-11-10T00:00:00.000Z",
        )
        .await
        .unwrap();
        let id = out["attachment"]["id"].as_str().unwrap();
        let (_meta, disk) = crate::attachments::read_file(dir.path(), "ada", id)
            .await
            .unwrap();
        assert_eq!(disk, bytes);
        assert!(out["voiceCalls"].as_array().unwrap().is_empty());
        let raw = std::fs::read_to_string(dir.path().join("thread-meta.json")).unwrap();
        assert!(raw.contains("\"seconds\": 4"));
        assert!(!raw.contains("audio"));
        assert!(!raw.contains("short-clip"));
        assert_eq!(out["command"]["attachments"][0]["id"], id);
        assert!(out["command"].get("quote").is_none());
    }

    #[tokio::test]
    async fn a_stored_audio_file_is_dispatched_without_a_second_copy() {
        let dir = tempfile::tempdir().unwrap();
        let threads = crate::thread_meta::ThreadMeta::new(dir.path().join("thread-meta.json"));
        let meta =
            crate::attachments::write_file(dir.path(), "ada", "voice.m4a", "audio/mp4", b"once")
                .await
                .unwrap();
        let id = meta["id"].as_str().unwrap().to_string();
        let out = dispatch_stored_voice(
            dir.path(),
            "ada",
            &threads,
            "thread-1",
            &id,
            12.0,
            "2026-11-10T00:00:00.000Z",
        )
        .await
        .unwrap();
        assert_eq!(
            crate::attachments::read_dir(dir.path(), "ada")
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(out["command"]["attachments"][0]["id"], id);
        assert_eq!(out["attachment"]["id"], id);
        assert!(out["command"].get("quote").is_none());
        assert_eq!(out["voiceCalls"][0]["seconds"], 12);
        assert!(out["voiceCalls"][0].get("audio").is_none());
        let raw = serde_json::to_string(&out["voiceCalls"]).unwrap();
        assert!(!raw.contains("once"));
    }

    #[tokio::test]
    async fn a_non_audio_file_is_refused_as_a_voice_clip() {
        let dir = tempfile::tempdir().unwrap();
        let threads = crate::thread_meta::ThreadMeta::new(dir.path().join("thread-meta.json"));
        let meta =
            crate::attachments::write_file(dir.path(), "ada", "note.txt", "text/plain", b"hi")
                .await
                .unwrap();
        let id = meta["id"].as_str().unwrap();
        let err = dispatch_stored_voice(
            dir.path(),
            "ada",
            &threads,
            "thread-1",
            id,
            12.0,
            "2026-11-10T00:00:00.000Z",
        )
        .await
        .unwrap_err();
        assert_eq!(err.message, "That file is not a voice clip.");
        assert!(threads.get(Some("thread-1")).await.unwrap()["voiceCalls"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_computer_open_turn_stays_out_of_the_one_to_one_chat() {
        let p = json!({ "turnItems": [
            { "id": "1", "type": "user_message", "text": "hi", "status": "completed" },
            { "id": "2", "type": "assistant_message", "text": "hello", "status": "completed" },
            { "id": "3", "type": "user_message", "text": crate::computer::OPEN_PROMPT, "status": "completed" },
            { "id": "4", "type": "assistant_message", "text": "Opening Google.", "status": "completed" },
            { "id": "5", "type": "dynamic_tool", "toolName": "preview_open", "status": "completed" },
            { "id": "6", "type": "user_message", "text": "thanks", "status": "completed" },
            { "id": "7", "type": "assistant_message", "text": "welcome", "status": "completed" },
        ]});
        let t = transcript(&p);
        let texts: Vec<&str> = t["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(texts, vec!["hi", "hello", "thanks", "welcome"]);
        assert!(
            t["events"].as_array().unwrap().is_empty(),
            "computer-open tool use stays out of the one-to-one chat"
        );
    }

    #[test]
    fn a_groups_turns_stay_out_of_the_one_to_one_chat() {
        let p = json!({ "turnItems": [
            { "id": "1", "type": "user_message", "text": "hi", "status": "completed" },
            { "id": "2", "type": "assistant_message", "text": "hello", "status": "completed" },
            { "id": "3", "type": "user_message", "text": "[Group \"Duo\": Ada (@ada). You wrote:]\nask @tester", "status": "completed" },
            { "id": "4", "type": "assistant_message", "text": "@tester what is 2+2?", "status": "completed" },
            { "id": "5", "type": "dynamic_tool", "toolName": "t3_thread_send", "status": "completed" },
            { "id": "6", "type": "user_message", "text": "thanks", "status": "completed" },
            { "id": "7", "type": "assistant_message", "text": "welcome", "status": "completed" },
        ]});
        let t = transcript(&p);
        let texts: Vec<&str> = t["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(texts, vec!["hi", "hello", "thanks", "welcome"]);
        assert!(
            t["events"].as_array().unwrap().is_empty(),
            "the Bot's messaging inside the group is hidden too"
        );

        let padded = json!({ "turnItems": [
            { "id": "1", "type": "user_message", "text": "  [Group \"Duo\": Ada. You wrote:]\nhi\n\n[Your reply is shared with the group.]", "status": "completed" },
            { "id": "2", "type": "assistant_message", "text": "leak", "status": "completed" },
            { "id": "3", "type": "user_message", "text": "back", "status": "completed" },
        ]});
        let kept = transcript(&padded);
        let texts: Vec<&str> = kept["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(
            texts,
            vec!["back"],
            "a padded group prompt stays out of the one-to-one chat"
        );
    }

    #[test]
    fn a_group_prompt_later_in_the_text_and_the_older_slug_frame_stay_out() {
        let later = json!({ "turnItems": [
            { "id": "1", "type": "user_message", "text": "hi", "status": "completed" },
            { "id": "2", "type": "assistant_message", "text": "hello", "status": "completed" },
            { "id": "3", "type": "user_message", "text": "note [Group \"Duo\": Ada (@ada). You wrote:]\nask", "status": "completed" },
            { "id": "4", "type": "assistant_message", "text": "from the group", "status": "completed" },
            { "id": "5", "type": "user_message", "text": "thanks", "status": "completed" },
        ]});
        let later = transcript(&later);
        let kept: Vec<&str> = later["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(kept, vec!["hi", "hello", "thanks"]);

        let older = json!({ "turnItems": [
            { "id": "1", "type": "user_message", "text": "mine", "status": "completed" },
            { "id": "2", "type": "user_message", "text": "[Group \"Duo\": Ada (@ada), Bea (@bea). You wrote:]\nask\n\n[Your reply is shared with the group. Write @slug only when you need something from that member; never to thank or acknowledge.]", "status": "completed" },
            { "id": "3", "type": "assistant_message", "text": "should hide", "status": "completed" },
            { "id": "4", "type": "user_message", "text": "still mine", "status": "completed" },
        ]});
        let older = transcript(&older);
        let kept: Vec<&str> = older["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(kept, vec!["mine", "still mine"]);
    }

    #[test]
    fn a_group_only_projection_stays_empty_for_one_to_one_and_kept_for_the_group() {
        let frame = "[Group \"Duo\": Ada (@ada). You wrote:]\nask";
        let mut items = vec![
            json!({ "id": "turn-item:message:disp-1", "type": "user_message", "text": frame, "status": "completed" }),
            json!({ "id": "turn-item:message:reply-1", "type": "assistant_message", "text": "four", "status": "streaming", "streaming": true }),
        ];
        let group_only = json!({ "turnItems": items });
        assert!(transcript(&group_only)["messages"]
            .as_array()
            .unwrap()
            .is_empty());
        let rows = |projection: &Value| -> Vec<(String, String, String)> {
            group_transcript(projection)["messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| {
                    (
                        m["id"].as_str().unwrap().to_string(),
                        m["role"].as_str().unwrap().to_string(),
                        m["text"].as_str().unwrap().to_string(),
                    )
                })
                .collect()
        };
        let kept = group_transcript(&group_only);
        assert_eq!(
            rows(&group_only),
            vec![
                (
                    "turn-item:message:disp-1".into(),
                    "user".into(),
                    frame.into()
                ),
                (
                    "turn-item:message:reply-1".into(),
                    "bot".into(),
                    "four".into()
                ),
            ]
        );
        assert_eq!(kept["working"], true);
        items.push(json!({ "id": "u2", "type": "user_message", "text": "and then", "status": "completed" }));
        let with_follow = json!({ "turnItems": items });
        assert_eq!(
            rows(&with_follow),
            vec![
                (
                    "turn-item:message:disp-1".into(),
                    "user".into(),
                    frame.into()
                ),
                (
                    "turn-item:message:reply-1".into(),
                    "bot".into(),
                    "four".into()
                ),
                ("u2".into(), "user".into(), "and then".into()),
            ]
        );
    }

    fn family() -> Value {
        json!({
            "turnItems": [{
                "type": "subagent",
                "id": "away",
                "title": "Away",
                "status": "running",
                "remoteChild": { "threadId": "remote-1", "environmentId": "env-9" },
                "childThreadId": "should-not-win"
            }],
            "subagents": [{
                "id": "notes",
                "title": "Notes",
                "status": "completed",
                "childThreadId": "child-thread"
            }]
        })
    }

    #[test]
    fn a_child_dispatch_uses_the_child_thread_and_a_refusal_does_not_write_the_parent() {
        let rows = crate::lineage::rows(&family());
        let sent = child_send(&rows, "parent-thread", "child-thread", "hello", &[]).unwrap();
        assert_eq!(sent["type"], "message.dispatch");
        assert_eq!(sent["threadId"], "child-thread");
        assert!(sent.get("quote").is_none());
        assert!(sent.get("parentSlug").is_none());

        let mut written = Vec::new();
        for asked in ["remote-1", "parent-thread", "no-such-thread"] {
            match child_send(&rows, "parent-thread", asked, "nope", &[]) {
                Ok(command) => written.push(command),
                Err(message) => assert_eq!(message, "That thread stays on the other machine."),
            }
        }
        assert!(
            written.is_empty(),
            "a refused child does not write the parent"
        );
        assert!(written
            .iter()
            .all(|command| command["threadId"] != "parent-thread"));
    }
}
