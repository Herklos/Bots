use super::*;

#[tokio::test]
async fn a_matching_if_none_match_is_not_modified_and_a_new_message_changes_the_tag() {
    let view = json!({
        "messages": [{ "id": "m1", "text": "Hello" }],
        "events": [],
        "requests": [],
        "working": false,
        "pins": [],
        "readOnly": false,
        "threadId": "t",
        "now": 10,
    });
    let mut later = view.clone();
    later["now"] = json!(99);
    let tag = chat::transcript_tag(&view);
    assert_eq!(tag, chat::transcript_tag(&later));
    assert!(chat::none_match(Some(&tag), &tag));
    let same = with_etag(&tag, Value::Null, false);
    assert_eq!(same.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        same.headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok()),
        Some(tag.as_str())
    );
    let bytes = axum::body::to_bytes(same.into_body(), 1024).await.unwrap();
    assert!(bytes.is_empty());
    later["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "m2", "text": "Next" }));
    let next = chat::transcript_tag(&later);
    assert_ne!(tag, next);
    let fresh = with_etag(&next, later, true);
    assert_eq!(fresh.status(), StatusCode::OK);
    assert_eq!(
        fresh
            .headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok()),
        Some(next.as_str())
    );
}

#[test]
fn origin_comes_from_the_request() {
    let mut h = HeaderMap::new();
    h.insert("host", "bots.tailnet.ts.net:8443".parse().unwrap());
    h.insert("x-forwarded-proto", "https".parse().unwrap());
    assert_eq!(request_origin(&h), "https://bots.tailnet.ts.net:8443");
    h.insert("host", "a\"+alert(1)+\"".parse().unwrap());
    assert_eq!(
        request_origin(&h),
        "https://127.0.0.1",
        "a host that could break out of a string is replaced"
    );
}

#[test]
fn library_mime_ignores_extension_case() {
    assert_eq!(library_mime("page.HTML"), "text/html; charset=utf-8");
    assert_eq!(library_mime("page.HTM"), "text/html; charset=utf-8");
    assert_eq!(library_mime("shot.PNG"), "image/png");
    assert_eq!(library_mime("shot.JPEG"), "image/jpeg");
    assert_eq!(library_mime("shot.JPG"), "image/jpeg");
    assert_eq!(library_mime("notes.TXT"), "application/octet-stream");
}

fn settings_state(dir: &std::path::Path) -> Arc<AppState> {
    let app_dir = dir.join("bots/.botapp");
    let cfg = crate::platform::config::Config {
        host: "127.0.0.1".into(),
        port: 0,
        t3_url: "http://127.0.0.1:1".into(),
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
        storage: storage::settings::ProcessEnv::default(),
    };
    let tailnet = Arc::new(crate::network::exposure::Tailnet::new());
    Arc::new(AppState {
        auth: crate::network::auth::Auth::load(&app_dir, String::new(), vec![]).unwrap(),
        t3: crate::harness::t3::T3::new(cfg.t3_url.clone(), cfg.t3_token.clone()),
        bots: bots::Bots::new(
            cfg.bots_root.clone(),
            cfg.bot_mcp_url.clone(),
            "fixture-only".into(),
        ),
        vault: vault::Vault::new(cfg.vault_file.clone(), cfg.vault_key_file.clone()),
        projects: projects::Projects::new(app_dir.join("projects.json")),
        threads: crate::chat::thread_meta::ThreadMeta::new(app_dir.join("thread-meta.json")),
        roster: crate::bots::roster::Roster::default(),
        providers: crate::harness::providers::Providers::default(),
        rooms: crate::chat::rooms::Rooms::new(app_dir.join("groups.json")),
        room_working: Mutex::new(crate::chat::rooms::WorkingBook::default()),
        routine_history: crate::chat::routines::RoutineHistory::new(
            app_dir.join("routine-history.json"),
        ),
        icons: crate::bots::icons::Icons::new(dir.join("icons")),
        usage: crate::bots::usage::Usage::new(app_dir.join("usage.json")),
        library: crate::bots::library::Library::new(
            Arc::new(storage::LocalDisk::new(cfg.bots_root.clone())),
            cfg.bot_mcp_url.clone(),
            "fixture-only".into(),
        ),
        system: crate::platform::release::System::new(cfg.data_root.clone(), cfg.app_dir()),
        mcp_token: "fixture-only".into(),
        provision_now: tokio::sync::Notify::new(),
        holds: chat::HeldReads::new(),
        access: None,
        exposure: crate::network::exposure::Exposure::from_env(|_| None),
        remote: Arc::new(crate::network::remote_access::RemoteAccess::new(
            crate::network::remote_access::RemoteConfig {
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
        cfg,
    })
}

fn json_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    h
}

async fn put(state: &Arc<AppState>, body: Value) -> Result<Value, ApiError> {
    put_settings(
        State(state.clone()),
        json_headers(),
        Bytes::from(body.to_string()),
    )
    .await
    .map(|Json(v)| v)
}

#[tokio::test]
async fn user_name_put_keeps_notifications() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    let saved = json!({
        "userName": "Ada",
        "notifications": { "enabled": true, "sound": false, "preview": "hidden", "when": "background" }
    });
    storage::write_json(&state.settings_path(), &saved)
        .await
        .unwrap();
    let next = put(&state, json!({ "userName": "Bea" })).await.unwrap();
    assert_eq!(next["userName"], "Bea");
    assert_eq!(next["notifications"], saved["notifications"]);
    let on_disk = storage::read_json(&state.settings_path()).await.unwrap();
    assert_eq!(on_disk["notifications"], saved["notifications"]);
}

#[tokio::test]
async fn bad_notifications_when_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    storage::write_json(&state.settings_path(), &json!({ "userName": "Ada" }))
        .await
        .unwrap();
    let err = put(&state, json!({ "notifications": { "when": "always" } }))
        .await
        .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    let on_disk = storage::read_json(&state.settings_path()).await.unwrap();
    assert!(on_disk.get("notifications").is_none());
}

#[tokio::test]
async fn notifications_patch_keeps_other_fields() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    storage::write_json(
        &state.settings_path(),
        &json!({
            "notifications": { "enabled": false, "sound": false, "preview": "hidden", "when": "background" }
        }),
    )
    .await
    .unwrap();
    let next = put(&state, json!({ "notifications": { "enabled": true } }))
        .await
        .unwrap();
    assert_eq!(
        next["notifications"],
        json!({ "enabled": true, "sound": false, "preview": "hidden", "when": "background" })
    );
}

#[tokio::test]
async fn group_answer_policy_defaults_to_last_and_put_rejects_anything_else() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    assert_eq!(
        AppState::settings_defaults()["groupAnswerPolicy"],
        json!("last")
    );
    storage::write_json(
        &state.settings_path(),
        &json!({ "groupAnswerPolicy": "nope", "userName": "Ada" }),
    )
    .await
    .unwrap();
    assert_eq!(state.settings().await["groupAnswerPolicy"], "last");
    let err = put(&state, json!({ "groupAnswerPolicy": "default" }))
        .await
        .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        err.message,
        "groupAnswerPolicy must be last, everyone, or chief"
    );
    let next = put(&state, json!({ "groupAnswerPolicy": "chief" }))
        .await
        .unwrap();
    assert_eq!(next["groupAnswerPolicy"], "chief");
    assert_eq!(next["userName"], "Ada");
    let again = state.settings().await;
    assert_eq!(again["groupAnswerPolicy"], "chief");
}
