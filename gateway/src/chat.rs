//! A small REST face on a Bot's main thread for clients that don't speak the harness RPC,
//! such as the Exact app's `app.ts`: read the conversation, send a message, stop a run, answer
//! an approval. T3 Code is the harness wired today; other backends may be added. The `/t3/*`
//! passthrough is that backend's door.
use serde_json::{json, Value};

const LIVE: &[&str] = &[
    "running",
    "in_progress",
    "pending",
    "queued",
    "streaming",
    "starting",
];

/// The bubbles and cards of a thread projection, oldest first.
pub fn transcript(projection: &Value) -> Value {
    let items = projection["turnItems"]
        .as_array()
        .or_else(|| projection["visibleTurnItems"].as_array())
        .cloned()
        .unwrap_or_default();
    let mut messages = Vec::new();
    let mut requests = Vec::new();
    let mut working = false;
    for raw in items {
        let item = if raw["item"].is_object() {
            raw["item"].clone()
        } else {
            raw
        };
        let kind = item["type"].as_str().unwrap_or("");
        let status = item["status"].as_str().unwrap_or("");
        let live = LIVE.contains(&status) || item["streaming"] == true;
        match kind {
            "user_message" | "assistant_message" => {
                let text = item["text"].as_str().unwrap_or("").trim().to_string();
                let attachments = item["attachments"].as_array().cloned().unwrap_or_default();
                if text.is_empty() && attachments.is_empty() && !live {
                    continue;
                }
                messages.push(json!({
                    "id": item["id"], "role": if kind == "user_message" { "user" } else { "bot" }, "text": text,
                    "attachments": attachments,
                    "at": item["startedAt"].as_str().or(item["updatedAt"].as_str()).unwrap_or(""), "streaming": live,
                }));
            }
            "approval_request" | "user_input_request" | "secret_request"
                if live || status == "awaiting_response" || status == "waiting" =>
            {
                requests.push(json!({ "id": item["id"], "kind": kind, "title": item["title"].as_str().unwrap_or("The Bot is asking"), "runId": item["runId"] }));
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
    json!({ "messages": messages, "requests": requests, "working": working || active.is_some(), "activeRunId": active })
}

pub const LOADED_SENTENCE: &str = "Search covers the loaded conversation.";

/// Filters messages already loaded from a projection. Nothing else is searched.
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
    projection["truncated"] == true || projection["hasMore"] == true || projection["partial"] == true
}

/// Adds a token line only where [`crate::usage::turn_token_line`] finds a shared id.
pub fn with_tokens(mut transcript: Value, projection: &Value) -> Value {
    if let Some(messages) = transcript.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages.iter_mut() {
            let id = message["id"].as_str().unwrap_or("");
            message["tokens"] = crate::usage::turn_token_line(projection, id)
                .unwrap_or(Value::Null);
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
            out["messages"][1]["tokens"]["inputTokens"].as_u64().unwrap(),
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
    }
}
