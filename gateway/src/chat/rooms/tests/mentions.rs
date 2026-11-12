use super::*;

#[tokio::test]
pub(crate) async fn delete_removes_one_group_and_leaves_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let keep = trio_room(&rooms).await;
    let drop = trio_room(&rooms).await;
    rooms.delete(&drop).await.unwrap();
    let left = rooms.list().await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0]["id"], json!(keep));
    assert!(
        rooms.delete(&drop).await.is_err(),
        "a second delete finds nothing"
    );
}

#[test]
pub(crate) fn mentions_pick_members_by_slug_or_name_and_all_wakes_everyone() {
    let m = trio();
    assert_eq!(
        plan_dispatches(
            &m,
            "@ada and @bea please look",
            None,
            AnswerPolicy::Last,
            None
        ),
        vec![0, 1]
    );
    assert_eq!(
        plan_dispatches(&m, "@cypark can you check", None, AnswerPolicy::Last, None),
        vec![2],
        "a name without spaces"
    );
    assert_eq!(
        plan_dispatches(&m, "@Cy hi", None, AnswerPolicy::Last, None),
        vec![2],
        "case does not matter"
    );
    assert_eq!(
        plan_dispatches(&m, "@all stand up", None, AnswerPolicy::Last, None),
        vec![0, 1, 2]
    );
    assert_eq!(
        plan_dispatches(&m, "@everyone", None, AnswerPolicy::Last, None),
        vec![0, 1, 2]
    );
    assert_eq!(
        plan_dispatches(
            &m,
            "mail me at joe@example.com",
            None,
            AnswerPolicy::Last,
            None
        ),
        vec![0, 1, 2],
        "an address is not a mention"
    );
    assert_eq!(
        plan_dispatches(&m, "hello", None, AnswerPolicy::Last, None),
        vec![0, 1, 2],
        "nobody has spoken yet"
    );
}

#[test]
pub(crate) fn without_a_mention_the_last_speaker_answers() {
    let m = trio();
    assert_eq!(
        plan_dispatches(&m, "and then?", Some("bea"), AnswerPolicy::Last, None),
        vec![1]
    );
    assert_eq!(
        plan_dispatches(&m, "@ada and then?", Some("bea"), AnswerPolicy::Last, None),
        vec![0],
        "a mention beats the last speaker"
    );
    assert_eq!(
        plan_dispatches(&m, "and then?", Some("gone"), AnswerPolicy::Last, None),
        vec![0, 1, 2]
    );
}

#[test]
pub(crate) fn everyone_wakes_the_whole_group_and_a_member_slug_wins() {
    let m = trio();
    let all = vec![0, 1, 2];
    assert_eq!(
        plan_dispatches(&m, "@everyone", Some("bea"), AnswerPolicy::Last, None),
        all,
        "an earlier speaker does not narrow @everyone"
    );
    assert_eq!(
        plan_dispatches(&m, "@ada @everyone", Some("bea"), AnswerPolicy::Last, None),
        all,
        "a named member beside @everyone still asks the group"
    );
    assert_eq!(
        plan_dispatches(
            &m,
            "please weigh in, @everyone, before lunch",
            Some("cy"),
            AnswerPolicy::Last,
            None
        ),
        all,
        "@everyone counts anywhere in the text"
    );
    assert_eq!(
        plan_dispatches(&m, "@team status", Some("bea"), AnswerPolicy::Last, None),
        vec![1],
        "@team is not a whole-group wake"
    );
    assert_eq!(
        plan_dispatches(&m, "@group status", Some("bea"), AnswerPolicy::Last, None),
        vec![1],
        "@group is not a whole-group wake"
    );

    let mut named_everyone = m.clone();
    named_everyone[1].slug = "everyone".into();
    assert_eq!(
        plan_dispatches(
            &named_everyone,
            "@everyone",
            Some("ada"),
            AnswerPolicy::Last,
            None
        ),
        vec![1],
        "a member whose slug is everyone is that member"
    );
    let mut named_all = trio();
    named_all[2].slug = "all".into();
    assert_eq!(
        plan_dispatches(&named_all, "@all", Some("bea"), AnswerPolicy::Last, None),
        vec![2],
        "a member whose slug is all is that member"
    );
}

#[tokio::test]
pub(crate) async fn everyone_send_frames_the_whole_group() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = trio_room(&rooms).await;
    rooms
        .send(&id, &trio(), "@bea first", "m0", "t0", AnswerPolicy::Last)
        .await
        .unwrap();
    let mut threads = HashMap::new();
    let room = rooms.get(&id).await.unwrap();
    let anchor = room["anchors"]["bea"].as_str().unwrap().to_string();
    threads.insert("bea".into(), thread(&anchor, &[("b1", "I started", false)]));
    rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();

    let (_, woken) = rooms
        .send(
            &id,
            &trio(),
            "thoughts @everyone?",
            "m1",
            "t1",
            AnswerPolicy::Last,
        )
        .await
        .unwrap();
    assert_eq!(slugs(&woken), vec!["ada", "bea", "cy"]);
    let text = &woken[0].text;
    assert!(text.contains("You wrote to everyone:"));
    assert!(text.contains("Everyone is being asked, so answer for yourself."));
}
