use super::*;

#[tokio::test]
pub(crate) async fn answer_policy_and_chief_are_checked_and_a_dropped_member_clears_the_chief() {
    let dir = tempfile::tempdir().unwrap();
    let rooms = Rooms::new(dir.path().join("groups.json"));
    let id = trio_room(&rooms).await;
    let err = rooms
        .update(&id, &json!({ "answerPolicy": "round" }), &[])
        .await
        .unwrap_err();
    assert_eq!(err.status, 400);
    let err = rooms
        .update(&id, &json!({ "chief": "nope" }), &[])
        .await
        .unwrap_err();
    assert_eq!(err.status, 400);
    let err = rooms
        .create(
            "Bad",
            "",
            "",
            "",
            &["ada".into(), "bea".into()],
            &[],
            Some("sideways"),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(err.status, 400);

    let set = rooms
        .update(&id, &json!({ "chief": "bea" }), &[])
        .await
        .unwrap();
    assert_eq!(set["chief"], "bea");
    let cleared = rooms
        .update(&id, &json!({ "chief": "" }), &[])
        .await
        .unwrap();
    assert!(cleared.get("chief").is_none());
    rooms
        .update(&id, &json!({ "chief": "cy" }), &[])
        .await
        .unwrap();
    let dropped = rooms
        .update(&id, &json!({ "members": ["ada", "bea"] }), &[])
        .await
        .unwrap();
    assert!(dropped.get("chief").is_none());
    let view = present(&dropped, &directory(), AnswerPolicy::Chief);
    assert_eq!(view["answerPolicy"], "default");
    assert_eq!(view["answerPolicyEffective"], "chief");
}

pub(crate) fn listed_room() -> Value {
    json!({
        "id": "room-1",
        "title": "Team",
        "description": "",
        "color": "blue",
        "shape": "ball",
        "members": ["ada", "bea", "cy"],
        "messages": [],
        "anchors": {},
        "threads": {},
        "hops": 0
    })
}

#[test]
pub(crate) fn a_list_row_names_zero_one_or_many_working_members() {
    let room = listed_room();
    let none = summary(&room, &[]);
    assert_eq!(none["working"], false);
    assert_eq!(none["workingSlugs"], json!([]));
    assert!(none.get("messages").is_none());

    let one = summary(&room, &["bea".into()]);
    assert_eq!(one["working"], true);
    assert_eq!(one["workingSlugs"], json!(["bea"]));

    let many = summary(&room, &["cy".into(), "ada".into(), "bea".into()]);
    assert_eq!(many["working"], true);
    assert_eq!(
        many["workingSlugs"],
        json!(["ada", "bea", "cy"]),
        "working slugs stay in member order"
    );

    let outsider = summary(&room, &["nope".into()]);
    assert_eq!(outsider["working"], false);
    assert_eq!(outsider["workingSlugs"], json!([]));
}

#[test]
pub(crate) fn an_rpc_error_keeps_the_last_known_working_value_for_a_short_time() {
    let mut book = WorkingBook::default();
    let now = Instant::now();
    assert!(
        !book.decide("thread-ada", MemberProbe::Failed, now),
        "an error with nothing remembered is not working"
    );
    assert!(book.decide("thread-ada", MemberProbe::Read { working: true }, now));
    assert!(
        book.decide(
            "thread-ada",
            MemberProbe::Failed,
            now + Duration::from_secs(1)
        ),
        "a failed read does not force false"
    );
    assert!(
        !book.decide(
            "thread-ada",
            MemberProbe::Failed,
            now + WORKING_TTL + Duration::from_secs(1)
        ),
        "the remembered value expires"
    );
    assert!(!book.decide("thread-bea", MemberProbe::Read { working: false }, now));
    assert!(
        !book.decide(
            "thread-bea",
            MemberProbe::Failed,
            now + Duration::from_millis(200)
        ),
        "a remembered idle stays idle"
    );
}

#[test]
pub(crate) fn a_dispatch_shows_working_until_a_live_item_or_about_two_seconds() {
    let mut book = WorkingBook::default();
    let now = Instant::now();
    book.note("thread-ada", now);
    assert!(book.decide(
        "thread-ada",
        MemberProbe::Read { working: false },
        now + Duration::from_millis(500)
    ));
    assert!(!book.decide(
        "thread-ada",
        MemberProbe::Read { working: false },
        now + WORKING_GRACE + Duration::from_millis(50)
    ));

    book.note("thread-ada", now);
    assert!(book.decide(
        "thread-ada",
        MemberProbe::Read { working: true },
        now + Duration::from_millis(100)
    ));
    assert!(
        !book.decide(
            "thread-ada",
            MemberProbe::Read { working: false },
            now + Duration::from_millis(200)
        ),
        "the first live item ends the grace period"
    );

    book.note("thread-bea", now);
    assert!(book.decide(
        "thread-bea",
        MemberProbe::Failed,
        now + Duration::from_millis(400)
    ));
}

#[tokio::test]
pub(crate) async fn group_thread_reads_stay_within_the_fan_out_limit() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let current = std::sync::Arc::new(AtomicUsize::new(0));
    let peak = std::sync::Arc::new(AtomicUsize::new(0));
    let ids: Vec<String> = (0..6).map(|i| format!("thread-{i}")).collect();
    let current_load = current.clone();
    let peak_load = peak.clone();
    let reads = read_group_threads(ids, move |_thread| {
        let current_load = current_load.clone();
        let peak_load = peak_load.clone();
        async move {
            let now = current_load.fetch_add(1, Ordering::SeqCst) + 1;
            peak_load.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(40)).await;
            current_load.fetch_sub(1, Ordering::SeqCst);
            Ok((false, Vec::new()))
        }
    })
    .await;
    assert_eq!(reads.len(), 6);
    assert!(peak.load(Ordering::SeqCst) > 1);
    assert!(peak.load(Ordering::SeqCst) <= READ_LIMIT);
    assert!(reads
        .values()
        .all(|(probe, _)| *probe == MemberProbe::Read { working: false }));
}
