use super::*;

#[test]
pub(crate) fn a_retrying_error_and_a_routine_run_are_handled() {
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
pub(crate) fn requests_carry_the_id_the_respond_route_needs_and_their_choices() {
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
pub(crate) fn an_attachment_with_no_text_is_still_a_message() {
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
pub(crate) fn search_returns_only_matching_loaded_messages() {
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
pub(crate) fn a_token_line_is_attached_only_for_a_shared_id() {
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
pub(crate) async fn a_voice_send_stores_bytes_as_an_attachment_and_a_duration() {
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
pub(crate) fn an_audio_attachment_plays_as_audio() {
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
