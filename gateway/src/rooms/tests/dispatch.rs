use super::*;

#[tokio::test]
pub(crate) async fn create_needs_a_name_and_two_distinct_bots() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let ada = vec!["ada".to_string()];
    assert!(rooms
        .create(
            " ",
            "",
            "",
            "",
            &["ada".into(), "bea".into()],
            &[],
            None,
            None
        )
        .await
        .is_err());
    assert!(rooms
        .create("Solo", "", "", "", &ada, &[], None, None)
        .await
        .is_err());
    let twice = vec!["ada".to_string(), "ada".to_string()];
    let err = rooms
        .create("Twin", "", "", "", &twice, &[], None, None)
        .await
        .unwrap_err();
    assert_eq!(
        err.message, "A group needs two Bots.",
        "a duplicate is not a second Bot"
    );
}

#[tokio::test]
pub(crate) async fn update_changes_fields_keeps_messages_and_refuses_one_member() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = trio_room(&rooms).await;
    rooms
        .send(&id, &trio(), "hello", "m1", "t1", AnswerPolicy::Last)
        .await
        .unwrap();
    let updated = rooms
        .update(&id, &json!({ "title": "Crew", "description": "", "color": "red", "members": ["ada", "cy"] }), &[])
        .await
        .unwrap();
    assert_eq!(updated["title"], "Crew");
    assert_eq!(updated["description"], "");
    assert_eq!(updated["color"], "red");
    assert_eq!(updated["shape"], "hex", "an omitted field stays");
    assert_eq!(updated["members"], json!(["ada", "cy"]));
    assert_eq!(updated["messages"].as_array().unwrap().len(), 1);
    assert!(rooms
        .update(&id, &json!({ "members": ["ada"] }), &[])
        .await
        .is_err());
    assert!(rooms
        .update(&id, &json!({ "title": "  " }), &[])
        .await
        .is_err());
    assert!(rooms
        .update("missing", &json!({ "title": "x" }), &[])
        .await
        .is_err());
}

#[tokio::test]
pub(crate) async fn a_room_without_last_seen_is_stamped_and_only_a_later_bot_message_is_unread() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("groups.json");
    let old = json!({
        "version": 1,
        "rooms": [{
            "id": "room-1",
            "title": "Team",
            "description": "",
            "color": "blue",
            "shape": "ball",
            "members": ["ada", "bea"],
            "messages": [{
                "id": "m0", "at": "2020-01-01T00:00:00.000Z",
                "from": "ada", "name": "Ada", "text": "old"
            }],
            "anchors": {},
            "threads": {},
            "hops": 0
        }]
    });
    tokio::fs::write(&path, serde_json::to_vec(&old).unwrap())
        .await
        .unwrap();
    let rooms = Rooms::new(path);
    let listed = rooms.list().await.unwrap();
    let stamp = listed[0]["lastSeenAt"].as_str().unwrap().to_string();
    assert!(!stamp.is_empty());
    let row = summary(&listed[0], &[]);
    assert_eq!(row["unread"], false, "an old room is not unread");
    assert_eq!(row["notifications"], true, "a missing key stays on");
    assert_eq!(row["last"]["from"], "ada");
    assert_eq!(
        present(&listed[0], &[], AnswerPolicy::Last)["notifications"],
        true
    );
    let again = rooms.list().await.unwrap();
    assert_eq!(
        again[0]["lastSeenAt"],
        json!(stamp),
        "the stamp is written once"
    );

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
    assert_eq!(created["notifications"], true);

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let bot_at = crate::vault::now_iso();
    assert!(bot_at.as_str() > stamp.as_str());
    rooms
        .reply("room-1", "ada", "Ada", "new", "m1", &bot_at)
        .await
        .unwrap();
    let after_bot = rooms.get("room-1").await.unwrap();
    assert_eq!(summary(&after_bot, &[])["unread"], true);
    assert_eq!(summary(&after_bot, &[])["last"]["from"], "ada");

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let user_at = crate::vault::now_iso();
    rooms
        .send(
            "room-1",
            &directory(),
            "mine",
            "m2",
            &user_at,
            AnswerPolicy::Last,
        )
        .await
        .unwrap();
    let after_user = rooms.get("room-1").await.unwrap();
    assert_eq!(summary(&after_user, &[])["unread"], false);
    assert_eq!(summary(&after_user, &[])["last"]["from"], "user");

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let later = crate::vault::now_iso();
    rooms
        .reply("room-1", "bea", "Bea", "again", "m3", &later)
        .await
        .unwrap();
    let muted = rooms
        .update("room-1", &json!({ "notifications": false }), &[])
        .await
        .unwrap();
    assert_eq!(muted["notifications"], false);
    let muted_row = summary(&muted, &[]);
    assert_eq!(muted_row["unread"], true, "mute does not clear the dot");
    assert_eq!(muted_row["notifications"], false);
    assert_eq!(
        present(&muted, &[], AnswerPolicy::Last)["notifications"],
        false
    );
    let err = rooms
        .update("room-1", &json!({ "notifications": "no" }), &[])
        .await
        .unwrap_err();
    assert_eq!(err.message, "notifications must be a boolean");
    assert_eq!(
        rooms.get("room-1").await.unwrap()["notifications"],
        false,
        "a bad value is not stored"
    );

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let seen = rooms.seen("room-1").await.unwrap();
    assert!(seen["lastSeenAt"].as_str().unwrap() > later.as_str());
    assert_eq!(summary(&seen, &[])["unread"], false);
}

pub(crate) async fn patch_bot(root: &std::path::Path, slug: &str, patch: Value) {
    let path = root.join(slug).join(".bot").join("bot.json");
    let text = tokio::fs::read_to_string(&path).await.unwrap();
    let mut meta: Value = serde_json::from_str(&text).unwrap();
    for (key, value) in patch.as_object().unwrap() {
        meta[key] = value.clone();
    }
    tokio::fs::write(&path, serde_json::to_vec(&meta).unwrap())
        .await
        .unwrap();
}

#[tokio::test]
pub(crate) async fn a_subagent_cannot_take_a_seat_in_a_group() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bots");
    let bots = crate::bots::Bots::new(root.clone(), String::new(), String::new());
    let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
    let bea = bots.create(&json!({ "name": "Bea" })).await.unwrap();
    let ada_slug = ada["slug"].as_str().unwrap().to_string();
    let bea_slug = bea["slug"].as_str().unwrap().to_string();
    let kid = bots
        .create(&json!({ "name": "Kid", "parentSlug": ada_slug }))
        .await
        .unwrap();
    let kid_slug = kid["slug"].as_str().unwrap().to_string();
    let rooms = Rooms::new(dir.path().join("groups.json"));

    let loaded = bots.list().await.unwrap();
    let err = rooms
        .create(
            "Design",
            "",
            "",
            "",
            &[ada_slug.clone(), kid_slug.clone()],
            &loaded,
            None,
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(err.status, 409);
    assert!(rooms.list().await.unwrap().is_empty());

    patch_bot(&root, &bea_slug, json!({ "hideFromSidebar": true })).await;
    let loaded = bots.list().await.unwrap();
    let err = rooms
        .create(
            "Design",
            "",
            "",
            "",
            &[ada_slug.clone(), bea_slug.clone()],
            &loaded,
            None,
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(err.status, 409);

    patch_bot(
        &root,
        &bea_slug,
        json!({ "hideFromSidebar": false, "role": "bot" }),
    )
    .await;
    let loaded = bots.list().await.unwrap();
    let created = rooms
        .create(
            "Design",
            "",
            "",
            "",
            &[ada_slug.clone(), bea_slug.clone()],
            &loaded,
            None,
            None,
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    // A seat taken while Bea was a normal Bot stays until a member list is saved.
    patch_bot(&root, &bea_slug, json!({ "role": "subagent" })).await;
    let before = rooms.get(&id).await.unwrap();
    let loaded = bots.list().await.unwrap();
    let err = rooms
        .update(
            &id,
            &json!({ "title": "Nope", "members": [ada_slug, kid_slug] }),
            &loaded,
        )
        .await
        .unwrap_err();
    assert_eq!(err.status, 409);
    let after = rooms.get(&id).await.unwrap();
    assert_eq!(after["title"], before["title"]);
    assert_eq!(after["members"], before["members"]);

    let cy = bots.create(&json!({ "name": "Cy" })).await.unwrap();
    let cy_slug = cy["slug"].as_str().unwrap().to_string();
    let loaded = bots.list().await.unwrap();
    let updated = rooms
        .update(&id, &json!({ "members": [ada_slug, cy_slug] }), &loaded)
        .await
        .unwrap();
    assert_eq!(updated["members"], json!([ada_slug, cy_slug]));
}
