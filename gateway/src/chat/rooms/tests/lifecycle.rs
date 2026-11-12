use super::*;

#[test]
pub(crate) fn a_stored_main_thread_is_missing() {
    assert!(Rooms::group_thread_missing("", "main-1"));
    assert!(Rooms::group_thread_missing("main-1", "main-1"));
    assert!(!Rooms::group_thread_missing("group-1", "main-1"));
}

pub(crate) fn directory() -> Vec<Member> {
    vec![
        Member {
            slug: "ada".into(),
            title: "Ada".into(),
            thread_id: "thread-ada".into(),
            working: true,
            provisioned: true,
        },
        Member {
            slug: "bea".into(),
            title: "Bea".into(),
            thread_id: "thread-bea".into(),
            working: false,
            provisioned: true,
        },
    ]
}

#[tokio::test]
pub(crate) async fn a_room_send_stores_one_message_and_fans_out_to_each_main_thread() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let created = rooms
        .create(
            "Design",
            "",
            "",
            "",
            &["ada".into(), "bea".into()],
            &[],
            None,
            None,
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    assert!(!rooms.list().await.unwrap().is_empty());

    let (stored, dispatches) = rooms
        .send(
            &id,
            &directory(),
            "hello both",
            "m1",
            "t1",
            AnswerPolicy::Last,
        )
        .await
        .unwrap();
    assert_eq!(stored["messages"].as_array().unwrap().len(), 1);
    assert_eq!(stored["messages"][0]["text"], "hello both");
    assert_eq!(stored["messages"][0]["from"], "user");
    assert_eq!(dispatches.len(), 2);
    assert_eq!(dispatches[0].thread_id, "thread-ada");
    assert_eq!(dispatches[1].thread_id, "thread-bea");

    let commands = dispatch_commands(&dispatches);
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0]["threadId"], "thread-ada");
    assert_eq!(commands[1]["threadId"], "thread-bea");
    assert_eq!(commands[0]["type"], "message.dispatch");
    assert!(commands[0].get("quote").is_none());
    assert_eq!(commands[0]["messageId"], commands[0]["commandId"]);
    assert_ne!(commands[0]["messageId"], commands[1]["messageId"]);

    let again = Rooms::new(dir.path().join("groups.json"));
    let loaded = again.get(&id).await.unwrap();
    assert_eq!(loaded["messages"].as_array().unwrap().len(), 1);
}

#[tokio::test]
pub(crate) async fn naming_one_member_by_slug_or_title_dispatches_only_to_that_member() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = rooms
        .create(
            "Design",
            "",
            "",
            "",
            &["ada".into(), "bea".into()],
            &[],
            None,
            None,
        )
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (_, by_slug) = rooms
        .send(
            &id,
            &directory(),
            "ada please look",
            "m2",
            "t2",
            AnswerPolicy::Last,
        )
        .await
        .unwrap();
    assert_eq!(by_slug.len(), 1);
    assert_eq!(by_slug[0].slug, "ada");
    assert_eq!(by_slug[0].thread_id, "thread-ada");

    let (stored, by_title) = rooms
        .send(
            &id,
            &directory(),
            "ask Bea about it",
            "m3",
            "t3",
            AnswerPolicy::Last,
        )
        .await
        .unwrap();
    assert_eq!(by_title.len(), 1);
    assert_eq!(by_title[0].slug, "bea");
    assert_eq!(stored["messages"].as_array().unwrap().len(), 2);

    let (_, both_named) = rooms
        .send(
            &id,
            &directory(),
            "Ada and Bea",
            "m4",
            "t4",
            AnswerPolicy::Last,
        )
        .await
        .unwrap();
    assert_eq!(both_named.len(), 2);
}

#[tokio::test]
pub(crate) async fn a_member_without_a_thread_stays_quiet_and_replies_keep_order() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = rooms
        .create(
            "Design",
            "",
            "",
            "",
            &["ada".into(), "bea".into()],
            &[],
            None,
            None,
        )
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let members = vec![
        Member {
            slug: "ada".into(),
            title: "Ada".into(),
            thread_id: "thread-ada".into(),
            working: true,
            provisioned: true,
        },
        Member {
            slug: "bea".into(),
            title: "Bea".into(),
            thread_id: String::new(),
            working: false,
            provisioned: false,
        },
    ];
    let (_, dispatches) = rooms
        .send(&id, &members, "hello", "m1", "t1", AnswerPolicy::Last)
        .await
        .unwrap();
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0].slug, "ada");

    rooms
        .reply(&id, "ada", "Ada", "On it", "r1", "t2")
        .await
        .unwrap();
    rooms
        .reply(&id, "ada", "Ada", "On it", "r1", "t3")
        .await
        .unwrap();
    let stored = rooms
        .reply(&id, "bea", "Bea", "I am still starting", "r2", "t4")
        .await
        .unwrap();
    let messages = stored["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1]["name"], "Ada");
    assert_eq!(messages[1]["from"], "ada");
    assert_eq!(messages[2]["name"], "Bea");
    assert_eq!(messages[2]["text"], "I am still starting");

    let view = present(&stored, &members, AnswerPolicy::Last);
    assert_eq!(view["members"][0]["name"], "Ada");
    assert_eq!(view["members"][0]["line"], "Working");
    assert_eq!(view["members"][0]["state"], "working");
    assert_eq!(view["members"][1]["line"], "Getting set up…");
    assert_eq!(view["members"][1]["ready"], false);
    assert_eq!(view["members"][1]["state"], "asleep");
    assert_eq!(view["messages"].as_array().unwrap().len(), 3);
}

pub(crate) fn trio() -> Vec<Member> {
    let mut m = directory();
    m.push(Member {
        slug: "cy".into(),
        title: "Cy Park".into(),
        thread_id: "thread-cy".into(),
        working: false,
        provisioned: true,
    });
    m
}

pub(crate) async fn trio_room(rooms: &Rooms) -> String {
    rooms
        .create(
            "Team",
            "Plans the week",
            "blue",
            "hex",
            &["ada".into(), "bea".into(), "cy".into()],
            &[],
            None,
            None,
        )
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

pub(crate) fn slugs(dispatches: &[Dispatch]) -> Vec<&str> {
    dispatches.iter().map(|d| d.slug.as_str()).collect()
}

#[tokio::test]
pub(crate) async fn a_group_keeps_its_description_and_avatar() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = trio_room(&rooms).await;
    let room = rooms.get(&id).await.unwrap();
    assert_eq!(room["description"], "Plans the week");
    assert_eq!(room["color"], "blue");
    assert_eq!(room["shape"], "hex");

    let odd = rooms
        .create(
            "Odd",
            "",
            "not-a-colour",
            "not-a-shape",
            &["ada".into(), "bea".into()],
            &[],
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(odd["color"], hbots_design::tokens().avatar.default.as_str());
    assert_eq!(
        odd["shape"], "ball",
        "an unknown shape falls back to the ball"
    );
    let app = rooms
        .create(
            "App",
            "",
            "brand.violet",
            "egg",
            &["ada".into(), "bea".into()],
            &[],
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        app["color"], "brand.violet",
        "the app's own colour names are kept"
    );

    let view = present(&room, &trio(), AnswerPolicy::Last);
    assert_eq!(view["description"], "Plans the week");
    assert_eq!(view["shape"], "hex");
    let row = summary(&room, &[]);
    assert!(
        row.get("messages").is_none(),
        "the list carries no transcript"
    );
    assert_eq!(row["last"], Value::Null);
}
