use super::*;

#[test]
pub(crate) fn tool_lines_carry_a_stable_verb_id() {
    let mut peers = Peers::new();
    peers.insert(
        "thread-inbox".into(),
        json!({ "slug": "inbox", "name": "Inbox", "color": "brand.pink", "shape": "hex" }),
    );
    let cases = [
        (
            json!({ "id": "s", "type": "dynamic_tool", "status": "completed", "toolName": "mcp__t3__t3_thread_send",
                "input": { "threadId": "thread-inbox", "message": "hi" } }),
            "sent",
            "Messaged",
        ),
        (
            json!({ "id": "c", "type": "dynamic_tool", "status": "completed", "toolName": "mcp__t3-code__schedule_task",
                "input": { "title": "Morning" } }),
            "routine-created",
            "Created routine",
        ),
        (
            json!({ "id": "u", "type": "dynamic_tool", "status": "completed", "toolName": "mcp__t3-code__update_scheduled_task",
                "input": { "scheduledTaskId": "scheduled-task:1", "title": "Morning" } }),
            "routine-updated",
            "Updated routine",
        ),
        (
            json!({ "id": "d", "type": "dynamic_tool", "status": "completed", "toolName": "mcp__t3-code__delete_scheduled_task",
                "input": { "scheduledTaskId": "scheduled-task:1" } }),
            "routine-deleted",
            "Deleted routine",
        ),
        (
            json!({ "id": "f", "type": "user_message", "status": "completed", "text": "hello", "senderThreadId": "thread-inbox" }),
            "from",
            "Message from",
        ),
        (
            json!({ "id": "r", "type": "user_message", "status": "completed", "text": "go", "scheduledTaskId": "scheduled-task:1" }),
            "ran",
            "Routine ran",
        ),
        (
            json!({ "id": "h", "type": "subagent", "status": "completed", "title": "Lookup" }),
            "started",
            "Started child thread",
        ),
    ];
    for (item, kind, text) in cases {
        let t = transcript_for(&json!({ "turnItems": [item] }), &peers);
        let event = &t["events"][0];
        assert_eq!(
            (event["kind"].as_str(), event["text"].as_str()),
            (Some(kind), Some(text))
        );
    }
}

#[test]
pub(crate) fn messages_between_bots_name_the_bot() {
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

pub(crate) fn provider_error_fixture(id: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/provider-errors.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    fixture["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"].as_str() == Some(id))
        .cloned()
        .unwrap_or_else(|| panic!("missing provider error fixture {id}"))
}

#[test]
pub(crate) fn a_terminal_usage_limit_is_a_provider_limit_event() {
    let item = provider_error_fixture("usage-limit-terminal");
    let event = &transcript(&json!({ "turnItems": [item.clone()] }))["events"][0];
    assert_eq!(event["kind"], "provider-limit");
    assert_eq!(event["text"], "Limited");
    assert_eq!(event["title"], item["failure"]["message"]);
    assert_eq!(event["failure"]["kind"], "rate_limit");
    assert_eq!(event["failure"]["terminal"], true);
    assert_eq!(event["failure"]["resetAt"], item["failure"]["resetAt"]);
    assert!(event["failure"]["attempt"].is_null());
    assert_eq!(event["bot"], Value::Null);
    let wrapped = transcript(&json!({ "turnItems": [{ "item": item }] }));
    assert_eq!(wrapped["events"][0]["kind"], "provider-limit");
}

#[test]
pub(crate) fn a_terminal_transport_error_is_a_provider_error() {
    let item = provider_error_fixture("transport-gave-up");
    let event = &transcript(&json!({ "turnItems": [item] }))["events"][0];
    assert_eq!(event["kind"], "provider-error");
    assert_eq!(event["text"], "Failed");
    assert_eq!(event["failure"]["kind"], "transport");
    assert_eq!(event["failure"]["terminal"], true);
    assert_eq!(event["failure"]["attempt"], 5);
    assert_eq!(event["failure"]["maxAttempts"], 5);
}

#[test]
pub(crate) fn a_completed_transport_retry_emits_no_event() {
    let item = provider_error_fixture("transport-retrying");
    let classified = provider_failure(&item).unwrap();
    assert_eq!(classified["kind"], "transport");
    assert_eq!(classified["terminal"], false);
    assert!(transcript(&json!({ "turnItems": [item] }))["events"]
        .as_array()
        .unwrap()
        .is_empty());
    let recovered = provider_error_fixture("usage-limit-retrying");
    assert_eq!(provider_failure(&recovered).unwrap()["kind"], "rate_limit");
    assert!(transcript(&json!({ "turnItems": [recovered] }))["events"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
pub(crate) fn an_error_with_only_retry_emits_no_event() {
    let retry =
        json!({ "id": "e", "type": "error", "status": "completed", "retry": { "attempt": 4 } });
    assert!(provider_failure(&retry).is_none());
    assert!(transcript(&json!({ "turnItems": [retry] }))["events"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
pub(crate) fn an_unknown_failure_class_is_other() {
    let mut item = provider_error_fixture("transport-gave-up");
    item["failure"]["class"] = json!("not_a_known_class");
    let classified = provider_failure(&item).unwrap();
    assert_eq!(classified["kind"], "other");
    assert_eq!(classified["terminal"], true);
    let event = &transcript(&json!({ "turnItems": [item] }))["events"][0];
    assert_eq!(event["kind"], "provider-error");
    assert_eq!(event["failure"]["kind"], "other");
    assert!(provider_failure(&json!({ "type": "user_message" })).is_none());
}
