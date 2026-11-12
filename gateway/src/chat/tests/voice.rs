use super::*;

#[tokio::test]
pub(crate) async fn a_short_clip_stays_an_attachment_the_player_can_use() {
    let dir = tempfile::tempdir().unwrap();
    let threads = crate::chat::thread_meta::ThreadMeta::new(dir.path().join("thread-meta.json"));
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
    let (_meta, disk) = crate::chat::attachments::read_file(dir.path(), "ada", id)
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
pub(crate) async fn a_stored_audio_file_is_dispatched_without_a_second_copy() {
    let dir = tempfile::tempdir().unwrap();
    let threads = crate::chat::thread_meta::ThreadMeta::new(dir.path().join("thread-meta.json"));
    let meta =
        crate::chat::attachments::write_file(dir.path(), "ada", "voice.m4a", "audio/mp4", b"once")
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
        crate::chat::attachments::read_dir(dir.path(), "ada")
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
pub(crate) async fn a_non_audio_file_is_refused_as_a_voice_clip() {
    let dir = tempfile::tempdir().unwrap();
    let threads = crate::chat::thread_meta::ThreadMeta::new(dir.path().join("thread-meta.json"));
    let meta =
        crate::chat::attachments::write_file(dir.path(), "ada", "note.txt", "text/plain", b"hi")
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
pub(crate) fn a_computer_open_turn_stays_out_of_the_one_to_one_chat() {
    let p = json!({ "turnItems": [
        { "id": "1", "type": "user_message", "text": "hi", "status": "completed" },
        { "id": "2", "type": "assistant_message", "text": "hello", "status": "completed" },
        { "id": "3", "type": "user_message", "text": crate::bots::computer::OPEN_PROMPT, "status": "completed" },
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
pub(crate) fn a_groups_turns_stay_out_of_the_one_to_one_chat() {
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
