use super::*;

#[test]
pub(crate) fn a_group_prompt_later_in_the_text_and_the_older_slug_frame_stay_out() {
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
pub(crate) fn a_group_only_projection_stays_empty_for_one_to_one_and_kept_for_the_group() {
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
    items.push(
        json!({ "id": "u2", "type": "user_message", "text": "and then", "status": "completed" }),
    );
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

pub(crate) fn family() -> Value {
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
            "status": "running",
            "childThreadId": "child-thread"
        }]
    })
}

#[test]
pub(crate) fn a_child_dispatch_uses_the_child_thread_and_a_refusal_does_not_write_the_parent() {
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

#[test]
pub(crate) fn a_client_message_id_is_kept_when_it_is_a_short_token() {
    assert_eq!(message_id(Some("m-1")), "m-1");
    assert_eq!(message_id(Some("  m-1  ")), "m-1");
    assert_ne!(
        message_id(Some("turn-item:message:m-1")),
        "turn-item:message:m-1"
    );
    assert_eq!(message_id(Some("")).len(), 36);
    assert_eq!(message_id(None).len(), 36);
    let command = command_with("thread", "hi", &[], "m-1");
    assert_eq!(command["messageId"], "m-1");
    assert_eq!(command["commandId"], "m-1");
}
