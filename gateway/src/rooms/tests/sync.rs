use super::*;

#[tokio::test]
pub(crate) async fn bots_cannot_wake_each_other_forever() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = rooms
        .create(
            "Pair",
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
    let (_, mut woken) = rooms
        .send(
            &id,
            &directory(),
            "@ada start",
            "m1",
            "t1",
            AnswerPolicy::Last,
        )
        .await
        .unwrap();
    let mut turns = 0;
    let mut counter = 0;
    while let Some(next) = woken.pop() {
        counter += 1;
        turns += 1;
        assert!(turns < 20, "the ping-pong never ends");
        let other = if next.slug == "ada" {
            "@bea your turn"
        } else {
            "@ada your turn"
        };
        let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
        threads.insert(
            next.slug.clone(),
            thread(&next.message_id, &[(&format!("r{counter}"), other, false)]),
        );
        let (_, wake) = rooms
            .sync(&id, &directory(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        woken.extend(wake);
    }
    assert_eq!(
        u64::try_from(turns).unwrap_or(0),
        MAX_HOPS + 1,
        "the person's first wake-up, then at most {MAX_HOPS} more"
    );
    let room = rooms.get(&id).await.unwrap();
    // A new message from the person resets the budget.
    rooms
        .send(
            &id,
            &directory(),
            "@bea again",
            "m2",
            "t9",
            AnswerPolicy::Last,
        )
        .await
        .unwrap();
    assert_eq!(rooms.get(&id).await.unwrap()["hops"], 0);
    assert!(room["messages"].as_array().unwrap().len() > 1);
}

#[test]
pub(crate) fn each_policy_picks_who_answers_and_an_explicit_name_still_wins() {
    let m = trio();
    assert_eq!(
        plan_dispatches(&m, "hello", None, AnswerPolicy::Everyone, Some("bea")),
        vec![0, 1, 2]
    );
    assert_eq!(
        plan_dispatches(&m, "hello", Some("cy"), AnswerPolicy::Last, Some("bea")),
        vec![2]
    );
    assert_eq!(
        plan_dispatches(&m, "hello", Some("cy"), AnswerPolicy::Chief, Some("bea")),
        vec![1],
        "the chief answers when nobody is named"
    );
    assert_eq!(
        plan_dispatches(
            &m,
            "@ada hello",
            Some("cy"),
            AnswerPolicy::Chief,
            Some("bea")
        ),
        vec![0],
        "an @mention beats the policy"
    );
    assert_eq!(
        plan_dispatches(&m, "@all hello", None, AnswerPolicy::Chief, Some("bea")),
        vec![0, 1, 2]
    );
    assert_eq!(
        plan_dispatches(&m, "ask Ada", None, AnswerPolicy::Everyone, Some("bea")),
        vec![0],
        "a single plain name beats the policy"
    );
}

#[test]
pub(crate) fn a_chief_who_cannot_answer_falls_back_to_the_last_speaker_then_everyone() {
    let mut quiet = trio();
    quiet[1].thread_id.clear();
    quiet[1].provisioned = false;
    assert_eq!(
        plan_dispatches(
            &quiet,
            "hello",
            Some("cy"),
            AnswerPolicy::Chief,
            Some("bea")
        ),
        vec![2],
        "the last speaker answers when the chief has no thread"
    );
    assert_eq!(
        plan_dispatches(&quiet, "hello", None, AnswerPolicy::Chief, Some("bea")),
        vec![0, 1, 2],
        "everyone answers when the chief cannot and nobody has spoken"
    );
    assert_eq!(max_hops(AnswerPolicy::Last, 3), MAX_HOPS);
    assert_eq!(max_hops(AnswerPolicy::Everyone, 3), MAX_HOPS);
    assert_eq!(max_hops(AnswerPolicy::Chief, 3), 2);
}

#[test]
pub(crate) fn the_effective_chief_is_the_stored_member_or_the_first() {
    let room = json!({ "members": ["ada", "bea", "cy"], "chief": "cy" });
    assert_eq!(effective_chief(&room, &trio()).as_deref(), Some("cy"));
    let dropped = json!({ "members": ["ada", "bea"], "chief": "gone" });
    assert_eq!(
        effective_chief(&dropped, &directory()).as_deref(),
        Some("ada")
    );
    let missing = json!({ "members": ["ada", "bea"] });
    assert_eq!(
        effective_chief(&missing, &directory()).as_deref(),
        Some("ada")
    );
}

#[tokio::test]
pub(crate) async fn chief_mode_frames_only_the_chief_and_allows_a_longer_handoff() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = rooms
        .create(
            "Team",
            "",
            "",
            "",
            &["ada".into(), "bea".into(), "cy".into()],
            &[],
            Some("chief"),
            Some("bea"),
        )
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, woken) = rooms
        .send(&id, &trio(), "hello", "m1", "t1", AnswerPolicy::Last)
        .await
        .unwrap();
    assert_eq!(slugs(&woken), vec!["bea"]);
    assert!(
        woken[0].text.contains(
            "You are the group chief. Answer first, and bring in members with @slug only when you need them."
        )
    );

    let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
    threads.insert(
        "bea".into(),
        thread(&woken[0].message_id, &[("b1", "@ada take this", false)]),
    );
    let (_, to_ada) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    assert_eq!(slugs(&to_ada), vec!["ada"]);
    assert!(
        !to_ada[0].text.contains("You are the group chief"),
        "a member the chief brings in is not the chief"
    );
    threads.insert(
        "ada".into(),
        thread(&to_ada[0].message_id, &[("a1", "@cy and this", false)]),
    );
    let (_, to_cy) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    assert_eq!(
        slugs(&to_cy),
        vec!["cy"],
        "the second handoff is still inside the budget"
    );
    threads.insert(
        "cy".into(),
        thread(&to_cy[0].message_id, &[("c1", "@bea done", false)]),
    );
    let (room, back) = rooms
        .sync(&id, &trio(), &threads, AnswerPolicy::Last)
        .await
        .unwrap();
    assert!(back.is_empty(), "the budget is members_len - 1");
    assert_eq!(room["hops"], 2);
}

#[tokio::test]
pub(crate) async fn a_room_without_the_new_fields_stays_on_last_and_a_group_override_beats_the_default(
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("groups.json");
    let rooms = Rooms::new(path);
    let id = trio_room(&rooms).await;
    let stored = rooms.get(&id).await.unwrap();
    assert!(stored.get("answerPolicy").is_none());
    assert!(stored.get("chief").is_none());
    let outsider = rooms
        .create(
            "Chiefs",
            "",
            "",
            "",
            &["ada".into(), "bea".into(), "cy".into()],
            &[],
            Some("chief"),
            Some("nope"),
        )
        .await
        .unwrap_err();
    assert_eq!(outsider.status.as_u16(), 400);
    assert_eq!(rooms.list().await.unwrap().len(), 1);
    let view = present(&stored, &trio(), AnswerPolicy::Everyone);
    assert_eq!(view["answerPolicy"], "default");
    assert_eq!(view["answerPolicyEffective"], "everyone");
    assert_eq!(view["chief"], Value::Null);

    let (_, woken) = rooms
        .send(&id, &trio(), "hello", "m1", "t1", AnswerPolicy::Last)
        .await
        .unwrap();
    assert_eq!(slugs(&woken), vec!["ada", "bea", "cy"]);
    rooms
        .reply(&id, "ada", "Ada", "noted", "r1", "t1b")
        .await
        .unwrap();

    let updated = rooms
        .update(
            &id,
            &json!({ "answerPolicy": "everyone", "chief": "cy" }),
            &[],
        )
        .await
        .unwrap();
    let view = present(&updated, &trio(), AnswerPolicy::Last);
    assert_eq!(view["answerPolicy"], "everyone");
    assert_eq!(view["answerPolicyEffective"], "everyone");
    assert_eq!(view["chief"], "cy");
    let (_, all) = rooms
        .send(&id, &trio(), "again", "m2", "t2", AnswerPolicy::Last)
        .await
        .unwrap();
    assert_eq!(
        slugs(&all),
        vec!["ada", "bea", "cy"],
        "everyone overrides a settings default of last, even after Ada spoke"
    );

    rooms
        .update(&id, &json!({ "answerPolicy": "last" }), &[])
        .await
        .unwrap();
    rooms
        .reply(&id, "cy", "Cy", "on it", "r2", "t2b")
        .await
        .unwrap();
    let (_, only_last) = rooms
        .send(
            &id,
            &trio(),
            "and then?",
            "m3",
            "t3",
            AnswerPolicy::Everyone,
        )
        .await
        .unwrap();
    assert_eq!(
        slugs(&only_last),
        vec!["cy"],
        "the room's last policy beats a settings default of everyone"
    );
}
