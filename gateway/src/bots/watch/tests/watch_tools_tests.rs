//! The Bots' own watch tools: public hosts only, the 5-minute floor, and the cap.

use super::super::*;
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_creation_needs_a_public_host_and_caps_at_ten() {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    let state = watch_state(dir.path(), Arc::clone(&mock)).await;

    let refused = crate::bots::watch_tools::call(
        &state,
        "watch_create",
        &json!({
            "threadId": "t-main", "name": "Home page",
            "url": "http://localhost:9/status",
            "check": { "type": "hash" }, "everyMs": 300_000,
        }),
    )
    .await;
    assert!(refused.is_err(), "a private host is refused: {refused:?}");
    let too_fast = crate::bots::watch_tools::call(
        &state,
        "watch_create",
        &json!({
            "threadId": "t-main", "name": "Fast",
            "url": "https://8.8.8.8/status",
            "check": { "type": "hash" }, "everyMs": 60_000,
        }),
    )
    .await;
    assert!(
        too_fast.is_err(),
        "a tool watch polls at least every 5 minutes"
    );

    // Ten watches fit; literal public addresses keep the preflight off the network.
    for i in 0..crate::bots::watch::MAX_PER_BOT {
        let made = crate::bots::watch_tools::call(
            &state,
            "watch_create",
            &json!({
                "threadId": "t-main", "name": format!("Page {i}"),
                "url": "https://8.8.8.8/status",
                "check": { "type": "hash" }, "everyMs": 300_000,
            }),
        )
        .await;
        assert!(made.is_ok(), "watch {i} creates: {made:?}");
    }
    let eleventh = crate::bots::watch_tools::call(
        &state,
        "watch_create",
        &json!({
            "threadId": "t-main", "name": "One too many",
            "url": "https://8.8.8.8/status",
            "check": { "type": "hash" }, "everyMs": 300_000,
        }),
    )
    .await;
    assert!(eleventh.is_err(), "a Bot keeps at most ten watches");

    let listed =
        crate::bots::watch_tools::call(&state, "watch_list", &json!({ "threadId": "t-main" }))
            .await
            .unwrap();
    let rows = listed["watches"].as_array().unwrap();
    assert_eq!(rows.len(), crate::bots::watch::MAX_PER_BOT);
    assert!(rows.iter().all(|row| row["kind"] == "watch"));
    assert!(
        rows.iter()
            .all(|row| row["watch"]["allowPrivate"] == json!(false)),
        "a tool watch never carries allowPrivate"
    );
}
