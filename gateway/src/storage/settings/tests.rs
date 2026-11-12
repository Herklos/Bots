use super::*;

use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};

fn env_root(path: &Path) -> ProcessEnv {
    ProcessEnv {
        root: Some(path.display().to_string()),
        ..ProcessEnv::default()
    }
}

fn write_file(home: &Path, stored: &Stored) {
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(storage_path(home), stored.to_json().to_string()).unwrap();
}

#[test]
fn environment_wins_over_the_file_and_the_file_over_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("hbots");
    let from_file = dir.path().join("from-file");
    let from_env = dir.path().join("from-env");
    std::fs::create_dir_all(&from_file).unwrap();
    std::fs::create_dir_all(&from_env).unwrap();
    std::fs::create_dir_all(home.join("bots")).unwrap();
    let mut stored = Stored::defaults();
    stored.root = Some(from_file.display().to_string());
    write_file(&home, &stored);

    let resolved = resolve_root(&home, &env_root(&from_env));
    assert_eq!(resolved, from_env);

    let resolved = resolve_root(&home, &ProcessEnv::default());
    assert_eq!(resolved, std::fs::canonicalize(&from_file).unwrap());

    std::fs::remove_file(storage_path(&home)).unwrap();
    let resolved = resolve_root(&home, &ProcessEnv::default());
    assert_eq!(resolved, home.join("bots"));
}

#[test]
fn a_bad_saved_folder_falls_back_and_the_reason_is_on_the_view() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("hbots");
    std::fs::create_dir_all(home.join("bots")).unwrap();
    let mut stored = Stored::defaults();
    stored.root = Some(home.display().to_string());
    write_file(&home, &stored);
    let resolved = resolve_root(&home, &ProcessEnv::default());
    assert_eq!(resolved, home.join("bots"));
    let body = view(
        &home,
        &home.join("bots"),
        &ProcessEnv::default(),
        false,
        None,
    );
    assert_eq!(body["effective"]["backend"], "filesystem");
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("cannot be the hbots home"));
    assert_eq!(body["restartRequired"], false);
}

#[tokio::test]
async fn a_locked_folder_rejects_a_different_value() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("hbots");
    let locked = dir.path().join("locked");
    let other = dir.path().join("other");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let env = env_root(&locked);
    let error = save(
        &home,
        &locked,
        &env,
        false,
        None,
        &json!({ "root": other.display().to_string() }),
    )
    .await
    .unwrap_err();
    assert_eq!(error.status, StatusCode::CONFLICT);
    assert!(!storage_path(&home).exists());
}

#[test]
fn folder_rules_reject_the_home_a_bot_folder_and_a_relative_path() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("hbots");
    std::fs::create_dir_all(&home).unwrap();
    let bot = dir.path().join("library").join("ada");
    std::fs::create_dir_all(bot.join(".bot")).unwrap();
    std::fs::write(bot.join(".bot").join("bot.json"), b"{}").unwrap();
    let app = dir.path().join("tree").join(".botapp").join("inside");
    std::fs::create_dir_all(app.parent().unwrap()).unwrap();

    assert!(validate_folder(&home, home.to_str().unwrap())
        .unwrap_err()
        .contains("hbots home"));
    assert!(validate_folder(&home, bot.join("notes").to_str().unwrap())
        .unwrap_err()
        .contains(".botapp"));
    assert!(validate_folder(&home, app.to_str().unwrap())
        .unwrap_err()
        .contains(".botapp"));
    assert!(validate_folder(&home, "bots")
        .unwrap_err()
        .contains("absolute"));
}

#[test]
fn copy_matches_hashes_and_leaves_the_old_folder() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("hbots");
    let source = home.join("bots");
    std::fs::create_dir_all(source.join("ada")).unwrap();
    std::fs::write(source.join("ada").join("note.txt"), b"keep-me").unwrap();
    let dest = dir.path().join("next");
    let body = copy_folder(&source, &home, dest.to_str().unwrap(), None).unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["files"], 1);
    assert_eq!(
        std::fs::read(source.join("ada").join("note.txt")).unwrap(),
        b"keep-me"
    );
    assert_eq!(
        std::fs::read(dest.join("ada").join("note.txt")).unwrap(),
        b"keep-me"
    );
    std::fs::remove_dir_all(&dest).unwrap();
    assert_eq!(
        std::fs::read(source.join("ada").join("note.txt")).unwrap(),
        b"keep-me"
    );
}

#[tokio::test]
async fn saving_a_new_folder_asks_for_a_restart_and_does_not_move_files() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("hbots");
    let running = home.join("bots");
    std::fs::create_dir_all(running.join("ada")).unwrap();
    std::fs::write(running.join("ada").join("note.txt"), b"stay").unwrap();
    let next = dir.path().join("next");
    std::fs::create_dir_all(&home).unwrap();
    let before = view(&home, &running, &ProcessEnv::default(), false, None);
    assert_eq!(before["restartRequired"], false);
    let after = save(
        &home,
        &running,
        &ProcessEnv::default(),
        true,
        None,
        &json!({ "root": next.display().to_string() }),
    )
    .await
    .unwrap();
    assert_eq!(after["restartRequired"], true);
    assert_eq!(after["canRestart"], true);
    assert_eq!(
        after["effective"]["root"],
        std::fs::canonicalize(&next).unwrap().display().to_string()
    );
    assert_eq!(
        std::fs::read(running.join("ada").join("note.txt")).unwrap(),
        b"stay"
    );
    assert!(!next.join("ada").join("note.txt").exists());
}

#[tokio::test]
async fn anytype_stays_on_the_file_system_until_the_space_is_reachable() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("hbots");
    let running = home.join("bots");
    std::fs::create_dir_all(&running).unwrap();
    let missing = save(
        &home,
        &running,
        &ProcessEnv::default(),
        false,
        None,
        &json!({
            "backend": "anytype",
            "host": "http://127.0.0.1:31009/",
            "space": "hbots"
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(missing.status, StatusCode::CONFLICT);
    assert!(!storage_path(&home).exists());

    let host = anytype_mock().await;
    write_key(&home, &ProcessEnv::default(), "anytype_secret")
        .await
        .unwrap();
    let body = save(
        &home,
        &running,
        &ProcessEnv::default(),
        false,
        None,
        &json!({ "backend": "anytype", "host": host, "space": "hbots" }),
    )
    .await
    .unwrap();
    assert_eq!(body["saved"]["backend"], "anytype");
    assert_eq!(body["saved"]["host"], host);
    assert_eq!(body["effective"]["backend"], "filesystem");
    assert!(!body.to_string().contains("anytype_secret"));
    let on_disk: Value =
        serde_json::from_str(&std::fs::read_to_string(storage_path(&home)).unwrap()).unwrap();
    assert_eq!(on_disk["backend"], "anytype");
    assert_eq!(on_disk["anytype"]["keyName"], "hbots");
    let absent = save(
        &home,
        &running,
        &ProcessEnv::default(),
        false,
        None,
        &json!({ "backend": "anytype", "space": "missing" }),
    )
    .await
    .unwrap_err();
    assert_eq!(absent.status, StatusCode::BAD_REQUEST);
    let still: Value =
        serde_json::from_str(&std::fs::read_to_string(storage_path(&home)).unwrap()).unwrap();
    assert_eq!(still["backend"], "anytype");
    let rejected = test_choice(
        &home,
        None,
        &json!({ "host": "http://127.0.0.1:31009/space" }),
    )
    .unwrap_err();
    assert_eq!(rejected.status, StatusCode::BAD_REQUEST);
}

/// A local Anytype stand-in: one private space named `hbots`.
async fn anytype_mock() -> String {
    let app = Router::new()
        .route(
            "/v2/auth/whoami",
            get(|| async {
                Json(json!({
                    "api": { "version": "2025-11-08" },
                    "key": { "name": "hbots" },
                    "grant": { "scoped": true, "restricted": true, "permission": "readwrite" }
                }))
            }),
        )
        .route(
            "/v2/spaces",
            get(|| async {
                Json(json!({
                    "has_not_granted_spaces": false,
                    "data": [{ "id": "s1", "name": "hbots" }]
                }))
            }),
        )
        .route(
            "/v2/spaces/{id}/members",
            get(|| async { Json(json!({ "data": [{ "name": "me" }] })) }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn responses_say_whether_a_key_is_set_and_never_include_it() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("hbots");
    let running = home.join("bots");
    std::fs::create_dir_all(&running).unwrap();
    let secret = "super-secret-anytype-key-value";
    let written = write_key(&home, &ProcessEnv::default(), secret)
        .await
        .unwrap();
    assert_eq!(written, json!({ "keySet": true }));
    let saved = save(
        &home,
        &running,
        &ProcessEnv::default(),
        false,
        None,
        &json!({ "backend": "filesystem", "key": secret }),
    )
    .await
    .unwrap();
    let tested = test_choice(&home, None, &json!({ "backend": "filesystem" })).unwrap();
    let copied = {
        let dest = dir.path().join("copy");
        copy_folder(&running, &home, dest.to_str().unwrap(), None).unwrap()
    };
    let status = view(&home, &running, &ProcessEnv::default(), false, None);
    assert_eq!(status["effective"]["keySet"], true);
    assert_eq!(status["saved"]["keySet"], true);
    let removed = remove_key(&home, &ProcessEnv::default()).await.unwrap();
    assert_eq!(removed, json!({ "keySet": false }));
    assert!(!key_path(&home).exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        write_key(&home, &ProcessEnv::default(), secret)
            .await
            .unwrap();
        let mode = std::fs::metadata(key_path(&home))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    let blob = format!("{written}{saved}{tested}{copied}{status}{removed}");
    assert!(!blob.contains(secret));
    let disk = std::fs::read_to_string(storage_path(&home)).unwrap();
    assert!(!disk.contains(secret));
}

#[test]
fn a_tilde_expands_and_a_trailing_host_slash_is_removed() {
    let home = home_dir();
    assert_eq!(expand_tilde("~/bots-data"), home.join("bots-data"));
    assert_eq!(
        normalize_host("http://127.0.0.1:31009/").unwrap(),
        DEFAULT_HOST
    );
}

#[test]
fn tunnel_and_remote_callers_cannot_change_storage() {
    let mut headers = HeaderMap::new();
    let local: SocketAddr = "127.0.0.1:9".parse().unwrap();
    assert!(ensure_local(local, &headers, None).is_ok());
    headers.insert("x-forwarded-for", "203.0.113.4".parse().unwrap());
    assert_eq!(
        ensure_local(local, &headers, None).unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    let remote: SocketAddr = "203.0.113.8:9".parse().unwrap();
    assert_eq!(
        ensure_local(remote, &HeaderMap::new(), Some(&Surface::Tailnet))
            .unwrap_err()
            .status,
        StatusCode::FORBIDDEN
    );
}
