use super::*;

#[test]
fn the_picker_keeps_haiku_and_the_current_choice() {
    let choices = model_choices(
        &json!({ "instanceId": "claudeAgent", "model": "claude-haiku-5-5" }),
        &Value::Null,
    );
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0]["label"], "Haiku");
    let extra = model_choices(
        &json!({ "instanceId": "other", "model": "local-model" }),
        &Value::Null,
    );
    assert_eq!(extra.len(), 2);
    assert_eq!(extra[1]["label"], "local-model");
    assert!(model_selection("claudeAgent", "claude-haiku-5-5").is_ok());
    assert!(model_selection("claudeAgent", "has space").is_err());
    let update = project_model_update("p", &default_model());
    assert_eq!(update["type"], "project.update");
    assert_eq!(update["defaultModelSelection"]["model"], "claude-haiku-5-5");
    assert_eq!(
        thread_model_command("t", &default_model())["type"],
        "thread.model-selection.set"
    );
}

fn listed(title: &str, favicon: Option<&str>, icon: Option<&str>) -> Value {
    json!({
        "id": "p",
        "title": title,
        "faviconPath": favicon,
        "projectIcon": icon,
    })
}

#[test]
fn a_matching_face_is_left_alone_and_any_drift_updates_the_project() {
    let path = ".bot/avatar-abc.svg";
    let project = listed("Ada", Some(path), None);
    assert!(face_in_sync(Some(&project), "Ada", path));

    let cases = [
        listed("Scout", Some(path), None),
        listed("Ada", Some(".bot/avatar-def.svg"), None),
        json!({ "id": "p", "title": "Ada", "projectIcon": null }),
        listed("Ada", Some(path), Some("icon")),
    ];
    for drifted in &cases {
        assert!(!face_in_sync(Some(drifted), "Ada", path));
        let update = project_face_update("p", "Ada", path);
        assert_eq!(update["type"], "project.update");
        assert_eq!(update["projectId"], "p");
        assert_eq!(update["title"], "Ada");
        assert_eq!(update["faviconPath"], path);
        assert!(update["projectIcon"].is_null());
        assert!(update["commandId"]
            .as_str()
            .is_some_and(|id| !id.is_empty()));
    }
    assert!(!face_in_sync(None, "Ada", path));
}

#[test]
fn a_missing_mode_follows_auto_review_and_a_stored_mode_wins() {
    let review_on = json!({ "autoReview": true });
    let review_off = json!({ "autoReview": false });
    assert_eq!(launch_runtime_mode(&json!({}), &review_on), "auto");
    assert_eq!(
        launch_runtime_mode(
            &json!({}),
            &json!({"defaultRuntimeMode": "approval-required"})
        ),
        "approval-required"
    );
    assert_eq!(
        launch_runtime_mode(
            &json!({"runtimeMode": "auto"}),
            &json!({"defaultRuntimeMode": "approval-required"})
        ),
        "auto"
    );
    assert_eq!(launch_runtime_mode(&json!({}), &review_off), "full-access");
    assert_eq!(
        launch_runtime_mode(&json!({ "runtimeMode": "approval-required" }), &review_off),
        "approval-required"
    );
    assert_eq!(
        launch_runtime_mode(&json!({ "runtimeMode": "auto" }), &review_off),
        "auto"
    );
    assert_eq!(
        launch_runtime_mode(&json!({ "runtimeMode": "full-access" }), &review_on),
        "full-access"
    );
    assert_eq!(
        launch_runtime_mode(&json!({ "runtimeMode": "ask" }), &review_on),
        "auto"
    );
    let capped = json!({ "maxRuntimeMode": "auto", "autoReview": false });
    assert_eq!(
        launch_runtime_mode(&json!({ "runtimeMode": "full-access" }), &capped),
        "auto"
    );
    assert_eq!(launch_runtime_mode(&json!({}), &capped), "auto");
    assert_eq!(
        launch_runtime_mode(
            &json!({}),
            &json!({ "defaultRuntimeMode": "full-access", "maxRuntimeMode": "approval-required" })
        ),
        "approval-required"
    );
    assert_eq!(
        launch_runtime_mode(&json!({ "runtimeMode": "approval-required" }), &capped),
        "approval-required"
    );
}

#[test]
fn stored_independent_launches_as_full_access_and_the_default_stays() {
    let review_on = json!({ "autoReview": true });
    let review_off = json!({ "autoReview": false });
    assert_eq!(
        launch_runtime_mode(&json!({ "runtimeMode": "independent" }), &review_on),
        "full-access"
    );
    assert_eq!(
        launch_runtime_mode(&json!({ "runtimeMode": "independent" }), &review_off),
        "full-access"
    );
    assert_eq!(
        launch_runtime_mode(
            &json!({}),
            &json!({ "defaultRuntimeMode": "independent", "autoReview": true })
        ),
        "full-access"
    );
    assert_eq!(
        launch_runtime_mode(
            &json!({ "runtimeMode": "independent" }),
            &json!({ "maxRuntimeMode": "auto", "autoReview": false })
        ),
        "auto"
    );
    assert_eq!(
        launch_runtime_mode(&json!({ "runtimeMode": "nope" }), &review_on),
        "auto"
    );
    assert_eq!(
        launch_runtime_mode(&json!({ "runtimeMode": "nope" }), &review_off),
        "full-access"
    );
    assert_eq!(launch_runtime_mode(&json!({}), &review_on), "auto");
    assert_eq!(launch_runtime_mode(&json!({}), &review_off), "full-access");
}

#[test]
fn empty_and_odd_models_are_refused_and_a_settings_default_is_listed_once() {
    assert!(model_selection("", "claude-haiku-5-5").is_err());
    assert!(model_selection("claudeAgent", "").is_err());
    assert!(model_selection("claudeAgent", "claude/haiku").is_err());
    let choices = model_choices(
        &json!({ "instanceId": "claudeAgent", "model": "claude-haiku-5-5" }),
        &json!({ "instanceId": "other", "model": "desk-model" }),
    );
    assert_eq!(choices.len(), 2);
    assert_eq!(choices[1]["model"], "desk-model");
    let again = model_choices(
        &json!({ "instanceId": "other", "model": "desk-model" }),
        &json!({ "instanceId": "other", "model": "desk-model" }),
    );
    assert_eq!(again.len(), 2);
}

/// A local harness that records each RPC payload and answers success.
async fn fake_harness() -> (String, Arc<std::sync::Mutex<Vec<Value>>>) {
    use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
    use axum::extract::State;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use axum::Router;

    #[derive(Clone)]
    struct Log(Arc<std::sync::Mutex<Vec<Value>>>);

    async fn projects() -> axum::Json<Value> {
        axum::Json(json!({ "projects": [] }))
    }

    async fn socket(ws: WebSocketUpgrade, State(log): State<Log>) -> axum::response::Response {
        ws.on_upgrade(move |sock| read_rpc(sock, log))
            .into_response()
    }

    async fn read_rpc(mut sock: WebSocket, log: Log) {
        while let Some(Ok(msg)) = sock.recv().await {
            let Message::Text(text) = msg else {
                continue;
            };
            let Ok(data) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if data["_tag"] != "Request" {
                continue;
            }
            if let Some(payload) = data.get("payload") {
                log.0.lock().expect("log").push(payload.clone());
            }
            let reply = json!({
                "_tag": "Exit",
                "requestId": data["id"],
                "exit": { "_tag": "Success", "value": { "ok": true } },
            });
            if sock
                .send(Message::Text(reply.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    }

    let log = Arc::new(std::sync::Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/api/projects", get(projects))
        .route("/ws", get(socket))
        .with_state(Log(log.clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://127.0.0.1:{port}"), log)
}

fn fixture_state(dir: &std::path::Path, t3_url: String) -> Arc<AppState> {
    use crate::bots::Bots;
    use crate::network::auth::Auth;
    use crate::network::exposure::{Exposure, Tailnet};
    use crate::network::remote_access::{RemoteAccess, RemoteConfig};
    use crate::platform::config::Config;
    use crate::projects::Projects;
    use crate::vault::Vault;

    let app_dir = dir.join("bots/.botapp");
    let cfg = Config {
        host: "127.0.0.1".into(),
        port: 0,
        t3_url: t3_url.clone(),
        t3_token: "fixture-only".into(),
        data_root: dir.to_path_buf(),
        bots_root: dir.join("bots"),
        static_dir: dir.join("dist"),
        password: String::new(),
        vault_file: dir.join("vault.enc"),
        vault_key_file: dir.join("vault-key"),
        bot_mcp_url: "http://127.0.0.1:1/mcp/bot".into(),
        allowed_origins: vec![],
        app_url: String::new(),
        access_team: String::new(),
        access_aud: String::new(),
        access_emails: vec![],
        storage: crate::storage::settings::ProcessEnv::default(),
    };
    let tailnet = Arc::new(Tailnet::new());
    let workspace: Arc<dyn crate::storage::workspace::Workspace> =
        Arc::new(crate::storage::LocalDisk::new(cfg.bots_root.clone()));
    Arc::new(AppState {
        auth: Auth::load(&app_dir, String::new(), vec![]).unwrap(),
        harness: crate::harness::connect(t3_url, cfg.t3_token.clone()),
        bots: Bots::with_workspace(
            cfg.bots_root.clone(),
            cfg.bot_mcp_url.clone(),
            "fixture-only".into(),
            Arc::clone(&workspace),
        ),
        vault: Vault::new(cfg.vault_file.clone(), cfg.vault_key_file.clone()),
        projects: Projects::new(app_dir.join("projects.json")),
        threads: crate::chat::thread_meta::ThreadMeta::new(app_dir.join("thread-meta.json")),
        roster: crate::bots::roster::Roster::default(),
        providers: crate::harness::providers::Providers::default(),
        rooms: crate::chat::rooms::Rooms::new(app_dir.join("groups.json")),
        room_working: std::sync::Mutex::new(crate::chat::rooms::WorkingBook::default()),
        routine_history: crate::chat::routines::RoutineHistory::new(
            app_dir.join("routine-history.json"),
        ),
        icons: crate::bots::icons::Icons::new(dir.join("icons")),
        usage: crate::bots::usage::Usage::new(app_dir.join("usage.json")),
        memory: crate::bots::memory::MemoryStore::new(Arc::clone(&workspace)),
        workspace: Arc::clone(&workspace),
        library: crate::bots::library::Library::new(
            workspace,
            cfg.bot_mcp_url.clone(),
            "fixture-only".into(),
        ),
        images: crate::chat::images::Resolver::new(),
        code_signals: crate::chat::code::SignalCache::default(),
        code_reads: crate::chat::code::ReadCache::default(),
        system: crate::platform::release::System::new(cfg.data_root.clone(), cfg.app_dir()),
        mcp_token: "fixture-only".into(),
        provision_now: tokio::sync::Notify::new(),
        holds: crate::chat::HeldReads::new(),
        hook_limit: Arc::new(crate::network::ratelimit::Limiter::new()),
        access: None,
        exposure: Exposure::from_env(|_| None),
        remote: Arc::new(RemoteAccess::new(
            RemoteConfig {
                home: dir.to_path_buf(),
                store_file: app_dir.join("access.json"),
                url_file: app_dir.join("tunnel-url"),
                local_port: 0,
                env_password: String::new(),
                env_tailnet: false,
                env_tailnet_users: vec![],
                tailscale_bin: None,
                network_env: None,
                process_host: None,
                merged_host: None,
            },
            tailnet.clone(),
        )),
        tailnet,
        sync: crate::storage::sync::SyncSlot::default(),
        cfg,
    })
}

#[tokio::test]
async fn a_child_is_launched_and_its_model_stays_on_the_child() {
    let (url, log) = fake_harness().await;
    let dir = tempfile::tempdir().unwrap();
    let state = fixture_state(dir.path(), url);
    state.bots.create(&json!({ "name": "Ada" })).await.unwrap();
    state
        .bots
        .create(&json!({
            "name": "Scout",
            "parentSlug": "ada",
            "role": "subagent",
        }))
        .await
        .unwrap();

    tick(&state).await.unwrap();

    let parent = state.bots.get("ada").await.unwrap().unwrap();
    let child = state.bots.get("scout").await.unwrap().unwrap();
    assert_eq!(child["role"], "subagent");
    assert_eq!(child["parentSlug"], "ada");
    let parent_project = parent["projectId"].as_str().unwrap();
    let child_project = child["projectId"].as_str().unwrap();
    let parent_thread = parent["mainThreadId"].as_str().unwrap();
    let child_thread = child["mainThreadId"].as_str().unwrap();
    assert_ne!(parent_project, child_project);
    assert_ne!(parent_thread, child_thread);

    let calls = log.lock().expect("log").clone();
    let child_root = child["workspaceRoot"].as_str().unwrap();
    assert!(calls
        .iter()
        .any(|p| { p["type"] == "project.create" && p["workspaceRoot"] == child_root }));
    assert!(calls.iter().any(|p| {
        p["threadId"] == child_thread && p["projectId"] == child_project && p["title"] == "Scout"
    }));

    log.lock().expect("log").clear();
    let selection = json!({
        "instanceId": "claudeAgent",
        "model": "claude-haiku-5-5",
        "options": [{ "id": "effort", "value": "low" }],
    });
    let provisioning = state.harness.provisioning().unwrap();
    push_model(provisioning, &child, &selection).await.unwrap();

    let calls = log.lock().expect("log").clone();
    let updates: Vec<_> = calls
        .iter()
        .filter(|p| p["type"] == "project.update")
        .collect();
    let threads: Vec<_> = calls
        .iter()
        .filter(|p| p["type"] == "thread.model-selection.set")
        .collect();
    assert_eq!(updates.len(), 1);
    assert_eq!(threads.len(), 1);
    assert_eq!(updates[0]["projectId"], child_project);
    assert_eq!(
        updates[0]["defaultModelSelection"]["options"][0]["value"],
        "low"
    );
    assert_eq!(threads[0]["threadId"], child_thread);
    assert!(calls.iter().all(|p| p["projectId"] != parent_project));
    assert!(calls.iter().all(|p| p["threadId"] != parent_thread));
}

#[tokio::test]
async fn pending_routines_are_applied_once_and_then_cleared() {
    let (url, log) = fake_harness().await;
    let dir = tempfile::tempdir().unwrap();
    let state = fixture_state(dir.path(), url);
    state
        .bots
        .create(&json!({
            "name": "Ada",
            "runtimeMode": "full-access",
            "pendingRoutines": [
                { "preset": "day", "prompt": "Send the note." },
                { "time": "08:30", "weekday": 1, "prompt": "Plan Monday." }
            ]
        }))
        .await
        .unwrap();

    tick(&state).await.unwrap();

    let ada = state.bots.get("ada").await.unwrap().unwrap();
    assert!(ada["pendingRoutines"].is_null());
    assert_eq!(ada["runtimeMode"], "full-access");
    assert!(ada["mainThreadId"]
        .as_str()
        .is_some_and(|id| !id.is_empty()));
    let calls = log.lock().expect("log").clone();
    assert!(calls.iter().any(|payload| {
        payload["runtimeMode"] == "full-access" && payload["interactionMode"] == "default"
    }));
    let upserts: Vec<_> = calls
        .iter()
        .filter(|payload| payload.get("schedule").is_some())
        .collect();
    assert_eq!(upserts.len(), 2);
    assert_eq!(upserts[0]["prompt"], "Send the note.");
    assert_eq!(upserts[0]["threadId"], ada["mainThreadId"]);
    assert_eq!(upserts[1]["schedule"]["timeOfDay"], "08:30");
    assert_eq!(upserts[1]["schedule"]["weekdays"], json!([1]));

    log.lock().expect("log").clear();
    tick(&state).await.unwrap();
    let again = log.lock().expect("log").clone();
    assert!(again
        .iter()
        .all(|payload| payload.get("schedule").is_none()));
}
