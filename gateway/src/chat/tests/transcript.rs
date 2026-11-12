use super::*;

#[test]
pub(crate) fn bubbles_from_turn_items() {
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

pub(crate) fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/t3")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

pub(crate) fn sample_view(text: &str, now: u64) -> Value {
    json!({
        "messages": [{ "id": "m1", "text": text }],
        "events": [],
        "requests": [],
        "working": false,
        "pins": ["m1"],
        "readOnly": false,
        "threadId": "thread-1",
        "now": now,
        "child": {
            "messages": [{ "id": "c1", "text": "Notes" }],
            "events": [],
            "requests": [],
            "working": false,
            "pins": [],
            "readOnly": true,
            "threadId": "child-1",
            "now": now,
        }
    })
}

#[test]
pub(crate) fn the_same_transcript_keeps_its_tag_and_a_new_message_changes_it() {
    let first = sample_view("Hello", 1);
    let later = sample_view("Hello", 2);
    assert_eq!(transcript_tag(&first), transcript_tag(&later));
    assert!(none_match(
        Some(&transcript_tag(&first)),
        &transcript_tag(&later)
    ));
    let mut changed = later;
    changed["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "m2", "text": "Again" }));
    assert_ne!(transcript_tag(&first), transcript_tag(&changed));
    let mut child_changed = sample_view("Hello", 1);
    child_changed["child"]["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "c2", "text": "More" }));
    assert_ne!(transcript_tag(&first), transcript_tag(&child_changed));
}

#[tokio::test]
pub(crate) async fn a_held_read_returns_when_the_transcript_changes() {
    let holds = HeldReads::new();
    let text = Arc::new(Mutex::new("Hello".to_string()));
    let seen = text.clone();
    let waiting = tokio::spawn(async move {
        holds
            .hold_until_change(
                "ada",
                "Hello",
                Duration::from_millis(20),
                Duration::from_secs(25),
                move || {
                    let seen = seen.clone();
                    async move {
                        let tag = seen.lock().unwrap_or_else(|p| p.into_inner()).clone();
                        Some(((), tag))
                    }
                },
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    *text.lock().unwrap_or_else(|p| p.into_inner()) = "Again".to_string();
    let outcome = tokio::time::timeout(Duration::from_secs(2), waiting)
        .await
        .expect("the held read waited out the full hold")
        .unwrap();
    assert!(matches!(outcome, Hold::Changed(())));
}

#[tokio::test]
pub(crate) async fn a_second_hold_replaces_the_first_and_shutdown_ends_it() {
    let holds = HeldReads::new();
    let first_holds = holds.clone();
    let first = tokio::spawn(async move {
        first_holds
            .hold_until_change(
                "ada",
                "a",
                Duration::from_secs(25),
                Duration::from_secs(25),
                || async { Some(((), "a".to_string())) },
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    let second_holds = holds.clone();
    let second = tokio::spawn(async move {
        second_holds
            .hold_until_change(
                "ada",
                "a",
                Duration::from_secs(25),
                Duration::from_secs(25),
                || async { Some(((), "a".to_string())) },
            )
            .await
    });
    let first = tokio::time::timeout(Duration::from_secs(2), first)
        .await
        .expect("the replaced hold stayed up")
        .unwrap();
    assert!(matches!(first, Hold::Superseded));
    holds.stop();
    let second = tokio::time::timeout(Duration::from_secs(2), second)
        .await
        .expect("shutdown left the hold waiting")
        .unwrap();
    assert!(matches!(second, Hold::Stopped));
}

#[test]
pub(crate) fn a_real_thread_gives_messages_with_times_and_no_noise_events() {
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
pub(crate) fn a_routine_made_by_a_bot_is_a_line_with_its_title() {
    let made = fixture("routine-created.json");
    let item = json!({ "id": "i1", "ordinal": 7, "type": "dynamic_tool", "status": "completed", "startedAt": "2026-11-10T13:05:00Z",
        "toolName": made["toolItem"]["toolName"], "input": made["toolItem"]["input"], "output": made["toolItem"]["output"] });
    let t = transcript(&json!({ "turnItems": [item] }));
    let e = &t["events"][0];
    assert_eq!(
        (e["kind"].as_str(), e["text"].as_str(), e["title"].as_str()),
        (
            Some("routine-created"),
            Some("Created routine"),
            Some("hbots test")
        )
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
