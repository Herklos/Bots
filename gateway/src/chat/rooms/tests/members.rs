use super::*;

#[tokio::test]
pub(crate) async fn a_mention_wakes_only_that_member_and_frames_the_message() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = trio_room(&rooms).await;
    let (_, woken) = rooms
        .send(
            &id,
            &trio(),
            "@bea draft the plan",
            "m1",
            "t1",
            AnswerPolicy::Last,
        )
        .await
        .unwrap();
    assert_eq!(slugs(&woken), vec!["bea"]);
    let text = &woken[0].text;
    assert!(text.contains("Group \"Team\""));
    assert!(
        text.contains("Ada, Bea, Cy Park"),
        "the roster lists names, not @handles"
    );
    assert!(
        !text.contains("Ada (@ada)"),
        "echoing the header must not wake everyone"
    );
    assert!(text.contains("You wrote:") && text.contains("draft the plan"));
    let commands = dispatch_commands(&woken);
    assert_eq!(commands[0]["messageId"], json!(woken[0].message_id));
    assert_eq!(commands[0]["threadId"], "thread-bea");
}

pub(crate) fn thread(anchor: &str, replies: &[(&str, &str, bool)]) -> Vec<Value> {
    let mut out = vec![
        json!({ "id": anchor, "role": "user", "text": "framed", "streaming": false, "at": "a0" }),
    ];
    for (id, text, streaming) in replies {
        out.push(
            json!({ "id": id, "role": "bot", "text": text, "streaming": streaming, "at": "a1" }),
        );
    }
    out
}

#[tokio::test]
pub(crate) async fn replies_after_the_anchor_join_the_room_once_and_streaming_ones_wait() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = trio_room(&rooms).await;
    let (_, woken) = rooms
        .send(&id, &trio(), "@ada hello", "m1", "t1", AnswerPolicy::Last)
        .await
        .unwrap();
    let anchor = woken[0].message_id.clone();
    let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
    threads.insert(
        "ada".into(),
        thread(
            &anchor,
            &[("a1", "Hi there", false), ("a2", "Typing", true)],
        ),
    );

    let (room, wake) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    assert!(wake.is_empty());
    let messages = room["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1]["from"], "ada");
    assert_eq!(messages[1]["text"], "Hi there");

    let (again, _) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    assert_eq!(
        again["messages"].as_array().unwrap().len(),
        2,
        "a second sync adds nothing"
    );

    threads.insert(
        "ada".into(),
        thread(
            &anchor,
            &[("a1", "Hi there", false), ("a2", "Done typing", false)],
        ),
    );
    let (done, _) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    assert_eq!(done["messages"].as_array().unwrap().len(), 3);
}

#[tokio::test]
pub(crate) async fn a_thread_without_the_anchor_or_with_old_replies_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = trio_room(&rooms).await;
    rooms
        .send(&id, &trio(), "@ada hello", "m1", "t1", AnswerPolicy::Last)
        .await
        .unwrap();
    let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
    threads.insert(
        "ada".into(),
        vec![
            json!({ "id": "old", "role": "bot", "text": "From before the group", "streaming": false }),
            json!({ "id": "other", "role": "user", "text": "Not our message", "streaming": false }),
        ],
    );
    let (room, wake) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    assert_eq!(
        room["messages"].as_array().unwrap().len(),
        1,
        "only the person's message"
    );
    assert!(wake.is_empty());
}

#[tokio::test]
pub(crate) async fn the_anchor_matches_the_item_id_t3_gives_a_dispatched_message() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = trio_room(&rooms).await;
    let (_, woken) = rooms
        .send(&id, &trio(), "@ada hello", "m1", "t1", AnswerPolicy::Last)
        .await
        .unwrap();
    let anchor = format!("turn-item:message:{}", woken[0].message_id);
    let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
    threads.insert(
        "ada".into(),
        vec![
            json!({ "id": "turn-item:message:older", "role": "user", "text": "earlier", "streaming": false }),
            json!({ "id": "turn-item:provider:x:old", "role": "bot", "text": "old reply", "streaming": false }),
            json!({ "id": anchor, "role": "user", "text": "framed", "streaming": false }),
            json!({ "id": "turn-item:provider:x:new", "role": "bot", "text": "fresh reply", "streaming": false, "at": "a2" }),
        ],
    );
    let (room, _) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    let texts: Vec<&str> = room["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["text"].as_str().unwrap())
        .collect();
    assert_eq!(
        texts,
        vec!["@ada hello", "fresh reply"],
        "only what follows our message is imported"
    );
}

#[tokio::test]
pub(crate) async fn a_reply_that_mentions_another_member_wakes_only_that_member() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = trio_room(&rooms).await;
    let (_, woken) = rooms
        .send(
            &id,
            &trio(),
            "@ada ask Cy for numbers",
            "m1",
            "t1",
            AnswerPolicy::Last,
        )
        .await
        .unwrap();
    let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
    threads.insert(
        "ada".into(),
        thread(
            &woken[0].message_id,
            &[("a1", "@cy what are this week's numbers?", false)],
        ),
    );
    let (room, wake) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    assert_eq!(slugs(&wake), vec!["cy"], "Bea stays asleep");
    assert!(wake[0].text.contains("Ada wrote:"));
    assert_eq!(room["hops"], 1);

    // Cy's answer is kept in the room. The hop budget is spent, so Ada is not woken again.
    threads.insert(
        "cy".into(),
        thread(&wake[0].message_id, &[("c1", "@ada 42 units", false)]),
    );
    let (room, back) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    assert!(back.is_empty(), "an answer does not start another turn");
    assert!(
        room["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["text"].as_str() == Some("@ada 42 units")),
        "Cy's answer still lands in the room"
    );
}

#[tokio::test]
pub(crate) async fn a_bot_reply_containing_everyone_wakes_nobody() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = trio_room(&rooms).await;
    let (_, woken) = rooms
        .send(&id, &trio(), "@ada start", "m1", "t1", AnswerPolicy::Last)
        .await
        .unwrap();
    let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
    threads.insert(
        "ada".into(),
        thread(
            &woken[0].message_id,
            &[("a1", "I am asking @everyone to look at this", false)],
        ),
    );
    let (room, wake) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    assert!(wake.is_empty(), "a Bot's own @everyone is not a wake-up");
    assert_eq!(room["hops"], 0);
    assert!(room["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| { m["text"].as_str() == Some("I am asking @everyone to look at this") }));
}

#[tokio::test]
pub(crate) async fn a_courtesy_reply_does_not_wake_anyone() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = trio_room(&rooms).await;
    let (_, woken) = rooms
        .send(&id, &trio(), "@ada start", "m1", "t1", AnswerPolicy::Last)
        .await
        .unwrap();
    let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
    threads.insert(
        "ada".into(),
        thread(&woken[0].message_id, &[("a1", "thanks @bea, on it", false)]),
    );
    let (room, wake) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    assert!(wake.is_empty(), "a thank-you must not start another turn");
    assert_eq!(room["hops"], 0, "courtesy does not spend the hop budget");
    assert!(room["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["text"].as_str() == Some("thanks @bea, on it")));
}
