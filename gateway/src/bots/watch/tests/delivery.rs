//! Self-delivery: a fire POSTs to this machine's own T3 hook, exactly once, and a
//! private URL never does unless the app confirmed allowPrivate.

use super::super::*;
use super::*;
use axum::response::IntoResponse;

async fn fake_hook() -> (String, Arc<std::sync::Mutex<Vec<Value>>>) {
    let hits: Arc<std::sync::Mutex<Vec<Value>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let records = Arc::clone(&hits);
    let router =
        axum::Router::new().fallback(axum::routing::any(move |req: axum::extract::Request| {
            let records = Arc::clone(&records);
            async move {
                let (parts, body) = req.into_parts();
                let bytes = axum::body::to_bytes(body, 2 << 20)
                    .await
                    .unwrap_or_default()
                    .to_vec();
                records.lock().unwrap().push(json!({
                    "path": parts.uri.path(),
                    "body": String::from_utf8_lossy(&bytes).to_string(),
                }));
                axum::Json(json!({"ok": true})).into_response()
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await });
    (format!("http://{addr}"), hits)
}

/// A local page server whose body can change between polls.
async fn local_page() -> (String, Arc<std::sync::Mutex<String>>) {
    let body: Arc<std::sync::Mutex<String>> = Arc::new(std::sync::Mutex::new("ok".into()));
    let served = Arc::clone(&body);
    let router = axum::Router::new().fallback(axum::routing::any(move || {
        let served = Arc::clone(&served);
        async move {
            let text = served.lock().unwrap().clone();
            axum::body::Body::from(text).into_response()
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await });
    (format!("http://{addr}"), body)
}

async fn hidden_record(state: &AppState, record: &Value) -> Value {
    let mut record = record.clone();
    let bot = state.bots.get("ada").await.unwrap().unwrap();
    let routines = state
        .harness
        .routines()
        .map_err(|err| err.to_string())
        .unwrap();
    hidden_routine(state, routines, &bot, &mut record)
        .await
        .unwrap();
    record
}

async fn push(state: &AppState, watch: Value) {
    let file = WatchFile::new(&state.cfg.bots_root, "ada");
    file.update(move |watches| {
        watches.push(watch);
        Ok(())
    })
    .await
    .unwrap();
}

async fn stored(state: &AppState, id: &str) -> Value {
    let file = WatchFile::new(&state.cfg.bots_root, "ada");
    file.watches(&file.load().await)
        .into_iter()
        .find(|w| w["id"].as_str() == Some(id))
        .unwrap_or(Value::Null)
}

async fn run(state: &AppState, id: &str, now: u64) {
    let watch = stored(state, id).await;
    poller::watch_poll(state, "ada", watch, now).await;
}

fn body_of(hit: &Value) -> Value {
    serde_json::from_str(hit["body"].as_str().unwrap()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_page_change_and_a_file_change_each_fire_one_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    let mut state = watch_state(dir.path(), Arc::clone(&mock)).await;
    let (t3, hits) = fake_hook().await;
    Arc::get_mut(&mut state).unwrap().cfg.t3_url = t3;
    let (page_url, page_body) = local_page().await;

    // The page watch targets this machine: the app-confirmed allowPrivate case.
    let page = {
        let mut w = watch_with(
            &mk_check(
                "contains",
                json!({ "type": "contains", "substring": "BROKEN" }),
            ),
            fire("on_change", 60),
            blank_state(),
        );
        w["id"] = json!("w_page001");
        w["name"] = json!("Build page");
        w["target"] = json!({ "type": "http", "url": page_url, "method": "GET" });
        w["allowPrivate"] = json!(true);
        w
    };
    let note = {
        let mut n = watch_with(
            &mk_check(
                "contains",
                json!({ "type": "contains", "substring": "shipped" }),
            ),
            fire("on_change", 60),
            blank_state(),
        );
        n["id"] = json!("w_note001");
        n["name"] = json!("Release note");
        n["target"] = json!({ "type": "file", "path": "notes/status.md" });
        n
    };
    push(&state, hidden_record(&state, &page).await).await;
    push(&state, hidden_record(&state, &note).await).await;
    state
        .workspace
        .write("ada/notes/status.md", b"not yet", None)
        .await
        .unwrap();

    let clock = 1_000_000;
    // First poll: the baseline.
    run(&state, "w_page001", clock).await;
    run(&state, "w_note001", clock).await;
    assert_eq!(
        hits.lock().unwrap().len(),
        0,
        "a first poll only records the baseline"
    );

    *page_body.lock().unwrap() = "page is BROKEN".into();
    state
        .workspace
        .write("ada/notes/status.md", b"we shipped", None)
        .await
        .unwrap();
    run(&state, "w_page001", clock + 120_000).await;
    run(&state, "w_note001", clock + 120_000).await;
    let fired = hits.lock().unwrap().clone();
    assert_eq!(fired.len(), 2, "one delivery each: {fired:?}");
    let page_hit = fired
        .iter()
        .find(|h| body_of(h)["watch"] == json!("Build page"))
        .expect("the page watch fired");
    let note_hit = fired
        .iter()
        .find(|h| body_of(h)["watch"] == json!("Release note"))
        .expect("the file watch fired");
    for hit in [page_hit, note_hit] {
        assert!(
            hit["path"].as_str().unwrap().starts_with("/api/hooks/"),
            "{hit:?}"
        );
    }
    let payload = body_of(page_hit);
    assert!(payload["reason"].as_str().unwrap().contains("BROKEN"));
    assert!(payload["previous"].as_str().unwrap().contains("absent"));
    let note_payload = body_of(note_hit);
    assert_eq!(
        note_payload["watchId"],
        json!("w_note001"),
        "the coalesce key travels"
    );

    // A matching poll inside the cooldown records the value without a refire.
    run(&state, "w_note001", clock + 180_000).await;
    assert_eq!(
        hits.lock().unwrap().len(),
        2,
        "no refire inside the cooldown"
    );
    let seen = stored(&state, "w_note001").await;
    assert!(
        seen["state"]["lastFireAt"].as_u64().unwrap() > 0,
        "the fire time is kept"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_private_url_without_allow_private_is_refused_and_backs_off() {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    let mut state = watch_state(dir.path(), Arc::clone(&mock)).await;
    let (t3, hits) = fake_hook().await;
    Arc::get_mut(&mut state).unwrap().cfg.t3_url = t3;
    let (page_url, _body) = local_page().await;

    let watch = {
        let mut w = watch_with(
            &mk_check("hash", json!({})),
            fire("on_change", 60),
            blank_state(),
        );
        w["id"] = json!("w_priv01");
        w["name"] = json!("Local page");
        w["target"] = json!({ "type": "http", "url": page_url, "method": "GET" });
        w["allowPrivate"] = json!(false);
        w
    };
    push(&state, hidden_record(&state, &watch).await).await;

    let clock = 1_000_000;
    for i in 0..5u64 {
        run(&state, "w_priv01", clock + i * 60_000).await;
    }
    assert_eq!(
        hits.lock().unwrap().len(),
        0,
        "a private URL never reaches T3"
    );
    let seen = stored(&state, "w_priv01").await;
    assert_eq!(seen["state"]["failures"], json!(5));
    assert!(
        seen["state"]["lastError"]
            .as_str()
            .is_some_and(|e| !e.is_empty()),
        "five failures show in the row: {}",
        seen["state"]
    );
    // The backoff doubles: the fifth failure waits at least two intervals.
    let next = seen["state"]["nextAt"].as_u64().unwrap();
    let fifth_at = clock + 4 * 60_000;
    assert!(next >= fifth_at + 120_000, "backoff doubles: {next}");

    // A day of that disables the watch and says so on the row.
    run(
        &state,
        "w_priv01",
        clock + poller::DISABLE_AFTER_MS + 61_000,
    )
    .await;
    let disabled = stored(&state, "w_priv01").await;
    assert_eq!(
        disabled["enabled"],
        json!(false),
        "disabled after 24 hours of failure"
    );
    assert!(
        disabled["pausedReason"].as_str().is_some(),
        "the Routines row says why: {}",
        disabled
    );
    assert!(disabled["state"]["lastError"]
        .as_str()
        .is_some_and(|msg| msg.contains("24 hours")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_routine_is_recreated_once_and_deleting_the_watch_deletes_it() {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    let mut state = watch_state(dir.path(), Arc::clone(&mock)).await;
    let (t3, hits) = fake_hook().await;
    Arc::get_mut(&mut state).unwrap().cfg.t3_url = t3;

    let watch = {
        let mut w = watch_with(
            &mk_check("hash", json!({})),
            fire("on_change", 60),
            blank_state(),
        );
        w["id"] = json!("w_lost001");
        w["name"] = json!("Lost and found");
        w["target"] = json!({ "type": "file", "path": "seed.txt" });
        w
    };
    let watch = hidden_record(&state, &watch).await;
    state
        .workspace
        .write("ada/seed.txt", b"first", None)
        .await
        .unwrap();
    push(&state, watch.clone()).await;
    let id = "w_lost001";
    let first_id = watch["routineId"].as_str().unwrap().to_string();

    // A first poll records the baseline; T3 then loses the task.
    let clock = 1_000_000;
    run(&state, id, clock).await;
    assert_eq!(hits.lock().unwrap().len(), 0);
    {
        let routines = state.harness.routines().unwrap();
        routines.delete(&json!({ "id": first_id })).await.unwrap();
    }

    state
        .workspace
        .write("ada/seed.txt", b"second", None)
        .await
        .unwrap();
    run(&state, id, clock + 120_000).await;
    assert_eq!(
        hits.lock().unwrap().len(),
        1,
        "the recreated routine takes the fire"
    );
    let seen = stored(&state, id).await;
    let new_id = seen["routineId"].as_str().unwrap();
    // T3 may restore the task under the same id; what matters is that a task with
    // a live hook path is back, and the stored copy now carries it.
    assert!(
        seen["task"]["webhook"]["path"]
            .as_str()
            .is_some_and(|p| !p.is_empty()),
        "the stored task is refreshed"
    );
    {
        let routines = state.harness.routines().unwrap();
        let listed = routines.list().await.unwrap();
        assert!(
            listed["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["id"] == json!(new_id)),
            "the routine is back"
        );
    }

    // Delete the watch: the hidden routine goes with it.
    crate::bots::watch_tools::call(
        &state,
        "watch_delete",
        &json!({ "threadId": "t-main", "id": id }),
    )
    .await
    .unwrap();
    {
        let routines = state.harness.routines().unwrap();
        let listed = routines.list().await.unwrap();
        assert!(
            listed["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| t["id"] != json!(new_id)),
            "deleting the watch deletes its hidden routine"
        );
    }
    let file = WatchFile::new(&state.cfg.bots_root, "ada");
    assert!(file.watches(&file.load().await).is_empty());
}
