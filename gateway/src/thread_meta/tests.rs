use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn message(ordinal: i64, kind: &str, extra: Value) -> Value {
    let mut item = json!({
        "ordinal": ordinal,
        "type": kind,
        "startedAt": format!("2026-10-30T10:00:0{ordinal}.000Z"),
    });
    for (key, value) in extra.as_object().cloned().unwrap_or_default() {
        item[key] = value;
    }
    item
}

fn peers() -> HashMap<String, String> {
    HashMap::from([("peer".to_string(), "Peer".to_string())])
}

fn scratch(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(name);
    (dir, file)
}

#[test]
fn preview_text_reads_as_prose() {
    assert_eq!(
        preview_text(
            "<reply-context>\nOld\n</reply-context>\n\n**Ship** the [report](https://x.test) `now`"
        ),
        "Ship the report now"
    );
    assert_eq!(
        preview_text("![a.csv](t3-context://1)\n# Title\n- item"),
        "Title item"
    );
    assert_eq!(preview_text("Ultrathink: go"), "go");
    assert_eq!(
        preview_text("Sent it to <t3-thread://v2/env/thread>."),
        "Sent it to."
    );
}

#[test]
fn the_newest_recognizable_item_wins() {
    let preview = |items: Vec<Value>| latest_preview(&json!({ "turnItems": items }), &peers());
    assert_eq!(
        preview(vec![
            message(1, "user_message", json!({ "text": "hi" })),
            message(2, "reasoning", json!({ "text": "hidden" })),
        ]),
        Some(
            json!({ "kind": "text", "role": "user", "text": "hi", "at": "2026-10-30T10:00:01.000Z" })
        )
    );
    assert_eq!(
        preview(vec![message(
            1,
            "assistant_message",
            json!({ "text": "", "attachments": [{}] })
        )])
        .unwrap()["text"],
        "Attachment"
    );
    assert_eq!(
        preview(vec![message(
            1,
            "user_input_request",
            json!({ "questions": [{ "question": "Which one?" }] })
        )])
        .unwrap()["kind"],
        "question"
    );
    assert_eq!(
        preview(vec![message(
            1,
            "dynamic_tool",
            json!({
                "status": "completed",
                "toolName": "mcp__t3__t3_thread_send",
                "input": json!({ "threadId": "peer", "message": "Done" }).to_string(),
            })
        )])
        .unwrap()["text"],
        "Messaged Peer: Done"
    );
    assert_eq!(
        preview(vec![message(
            1,
            "dynamic_tool",
            json!({ "status": "failed", "toolName": "t3_thread_send", "input": "{}" })
        )]),
        None
    );
    assert_eq!(
        latest_preview(
            &json!({ "visibleTurnItems": [{ "item": message(1, "assistant_message", json!({ "text": "wrapped" })) }] }),
            &HashMap::new()
        )
        .unwrap()["text"],
        "wrapped"
    );
}

#[tokio::test]
async fn previews_reload_only_moved_threads_and_never_pass_on_a_stale_stamp() {
    let meta = ThreadMeta::new(scratch("unused.json").1);
    let calls = AtomicUsize::new(0);
    let failing = AtomicBool::new(false);
    let fetch = |_: String| {
        let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
        let fails = failing.load(Ordering::SeqCst);
        async move {
            if fails {
                anyhow::bail!("offline");
            }
            Ok(
                json!({ "turnItems": [message(i64::try_from(n).unwrap_or(i64::MAX), "assistant_message", json!({ "text": format!("reply {n}") }))] }),
            )
        }
    };
    let names = HashMap::new();
    let stamps = |pairs: &[(&str, &str)]| {
        Value::Object(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), json!(v)))
                .collect(),
        )
    };

    assert_eq!(
        meta.previews(Some(&stamps(&[("a", "1")])), &names, fetch)
            .await
            .unwrap()["a"]["text"],
        "reply 1"
    );
    assert_eq!(
        meta.previews(Some(&stamps(&[("a", "1")])), &names, fetch)
            .await
            .unwrap()["a"]["text"],
        "reply 1"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    failing.store(true, Ordering::SeqCst);
    assert!(meta
        .previews(Some(&stamps(&[("a", "2")])), &names, fetch)
        .await
        .unwrap()
        .get("a")
        .is_none());
    assert_eq!(
        meta.previews(Some(&stamps(&[("a", "1")])), &names, fetch)
            .await
            .unwrap()["a"]["text"],
        "reply 1"
    );

    failing.store(false, Ordering::SeqCst);
    assert_eq!(
        meta.previews(Some(&stamps(&[("a", "2")])), &names, fetch)
            .await
            .unwrap()["a"]["text"],
        "reply 3"
    );

    for bad_input in [json!({ "../bad": "1" }), json!([])] {
        let err = meta.stale(Some(&bad_input)).await.unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
    }
    assert_eq!(
        meta.stale(None).await.unwrap_err().status,
        axum::http::StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn reactions_toggle_per_emoji_persist_and_reject_anything_but_emoji() {
    let (_dir, file) = scratch("meta.json");
    let meta = ThreadMeta::new(file.clone());
    meta.toggle_reaction(Some("t"), Some("m"), Some("🔥"))
        .await
        .unwrap();
    assert_eq!(
        meta.toggle_reaction(Some("t"), Some("m"), Some("👍🏽"))
            .await
            .unwrap()["reactions"]["m"],
        json!(["🔥", "👍🏽"])
    );
    assert_eq!(
        meta.toggle_reaction(Some("t"), Some("m"), Some("🔥"))
            .await
            .unwrap()["reactions"]["m"],
        json!(["👍🏽"])
    );
    assert_eq!(
        ThreadMeta::new(file.clone()).get(Some("t")).await.unwrap()["reactions"],
        json!({ "m": ["👍🏽"] })
    );
    assert_eq!(
        meta.toggle_reaction(Some("t"), Some("m"), Some("👍🏽"))
            .await
            .unwrap()["reactions"],
        json!({})
    );
    for emoji in ["<b>", "hello", "1", "#", ""] {
        let err = meta
            .toggle_reaction(Some("t"), Some("m"), Some(emoji))
            .await
            .unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST, "{emoji:?}");
    }
    assert!(std::fs::read_to_string(&file).unwrap().contains("\"t\""));
}

#[tokio::test]
async fn voice_calls_record_a_bounded_rounded_duration() {
    let (_dir, file) = scratch("meta.json");
    let meta = ThreadMeta::new(file);
    assert_eq!(
        meta.add_voice_call(Some("t"), Some(&json!(35.4)), "2026-10-30T21:46:00.000Z")
            .await
            .unwrap()["voiceCalls"],
        json!([{ "at": "2026-10-30T21:46:00.000Z", "seconds": 35 }])
    );
    for bad_seconds in [json!(-1), json!("12"), json!(86_401)] {
        let err = meta
            .add_voice_call(Some("t"), Some(&bad_seconds), "x")
            .await
            .unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn failed_short_voice_calls_are_hidden_without_rewriting_the_file() {
    let (_dir, file) = scratch("meta.json");
    let saved = json!({ "t": { "reactions": {}, "voiceCalls": [
        { "at": "old", "seconds": 0 }, { "at": "short", "seconds": 9 }, { "at": "valid", "seconds": 10 }
    ] } })
    .to_string();
    std::fs::write(&file, &saved).unwrap();
    assert_eq!(
        ThreadMeta::new(file.clone()).get(Some("t")).await.unwrap()["voiceCalls"],
        json!([{ "at": "valid", "seconds": 10 }])
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
}

#[tokio::test]
async fn invalid_thread_and_item_ids_are_refused() {
    let meta = ThreadMeta::new(scratch("meta.json").1);
    assert_eq!(
        meta.get(None).await.unwrap_err().status,
        axum::http::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        meta.get(Some("../etc")).await.unwrap_err().status,
        axum::http::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        meta.toggle_reaction(Some("t"), Some("bad item"), Some("🔥"))
            .await
            .unwrap_err()
            .status,
        axum::http::StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn toggling_one_reaction_twice_leaves_no_mark() {
    let meta = ThreadMeta::new(scratch("meta.json").1);
    meta.toggle_reaction(Some("t"), Some("m"), Some("🔥"))
        .await
        .unwrap();
    let cleared = meta
        .toggle_reaction(Some("t"), Some("m"), Some("🔥"))
        .await
        .unwrap();
    assert!(cleared["reactions"].get("m").is_none());
    assert_eq!(cleared["reactions"], json!({}));
}

#[tokio::test]
async fn reply_to_round_trips_under_the_gateway_message_id() {
    let meta = ThreadMeta::new(scratch("meta.json").1);
    let command = crate::chat::send_command("thread-1", "thanks", &[]);
    assert!(command.get("quote").is_none());
    let message_id = command["messageId"].as_str().unwrap();
    assert_eq!(message_id, command["commandId"].as_str().unwrap());
    let seen = meta
        .set_reply(
            Some("thread-1"),
            Some(message_id),
            Some("m-old"),
            Some("the quoted line"),
        )
        .await
        .unwrap();
    assert_eq!(seen["replyTo"][message_id]["id"], "m-old");
    assert_eq!(seen["replyTo"][message_id]["text"], "the quoted line");
    let again = meta.get(Some("thread-1")).await.unwrap();
    assert_eq!(again["replyTo"][message_id]["text"], "the quoted line");
}

#[tokio::test]
async fn pins_toggle_refuse_one_past_the_cap_and_keep_a_missing_id() {
    let (_dir, file) = scratch("meta.json");
    let meta = ThreadMeta::new(file.clone());
    for n in 0..PIN_CAP {
        meta.toggle_pin(Some("t"), Some(&format!("m{n}")))
            .await
            .unwrap();
    }
    let err = meta
        .toggle_pin(Some("t"), Some("overflow"))
        .await
        .unwrap_err();
    assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(!saved.contains("overflow"));
    let removed = meta.toggle_pin(Some("t"), Some("m0")).await.unwrap();
    let pins = removed["pins"].as_array().unwrap();
    assert_eq!(pins.len(), PIN_CAP - 1);
    assert!(pins.iter().all(|id| id != "m0"));
    meta.toggle_pin(Some("t"), Some("m0")).await.unwrap();
    let seen = meta.get(Some("t")).await.unwrap();
    let lines = pin_lines(
        seen["pins"].as_array().unwrap(),
        &[json!({ "id": "m1", "text": "Hello" })],
    );
    assert!(lines
        .iter()
        .any(|line| line["id"] == "m1" && line["missing"] == false));
    assert!(lines.iter().any(|line| {
        line["id"] == "m2" && line["missing"] == true && line["text"] == NOT_IN_VIEW
    }));
}

#[tokio::test]
async fn last_read_advances_while_open_and_a_hidden_bot_keeps_its_mark() {
    let (_dir, file) = scratch("meta.json");
    let meta = ThreadMeta::new(file.clone());
    let open = meta
        .mark_read(Some("t"), Some("m2"), true, false)
        .await
        .unwrap();
    assert_eq!(open["lastRead"], "m2");
    assert_eq!(open["unread"], false);
    assert_eq!(next_last_read("m1", "m2", true, false), "m2");

    let hidden = meta
        .mark_read(Some("h"), Some("m1"), false, true)
        .await
        .unwrap();
    assert_eq!(hidden["lastRead"], "");
    let later = meta
        .mark_read(Some("h"), Some("m2"), false, true)
        .await
        .unwrap();
    assert_eq!(later["lastRead"], "");
    assert_eq!(later["latest"], "m2");
    assert_eq!(later["unread"], true);
    assert_eq!(next_last_read("", "m2", false, true), "");
    assert!(is_unread("", "m2", false, true));
    assert!(!is_unread("m1", "m2", true, false));
    assert!(is_unread("m1", "m2", false, false));

    let opened = ThreadMeta::new(file)
        .mark_read(Some("h"), None, true, true)
        .await
        .unwrap();
    assert_eq!(opened["lastRead"], "m2");
    assert_eq!(opened["unread"], false);
}

#[test]
fn a_hidden_bot_with_a_later_message_id_is_unread_until_that_chat_is_open() {
    assert!(inbox_unread(true, false, "", "m2"));
    assert!(is_unread("", "m2", false, true));
    assert!(!inbox_unread(true, false, "m2", "m2"));
    assert!(!is_unread("m2", "m2", true, true));
    assert!(!inbox_unread(false, false, "", "m2"));
    assert!(inbox_unread(false, true, "m2", "m2"));
    let item = message(
        1,
        "assistant_message",
        json!({ "id": "m2", "text": "Done" }),
    );
    let preview = latest_preview(&json!({ "turnItems": [item] }), &HashMap::new()).unwrap();
    assert_eq!(preview["id"], "m2");
    assert_eq!(preview["text"], "Done");
}
