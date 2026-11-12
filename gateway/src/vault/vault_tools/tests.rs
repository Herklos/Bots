use super::*;
use crate::harness::adapters::mock::MockHarness;

fn bots() -> Vec<Value> {
    vec![json!({ "mainThreadId": "thread-main", "slug": "main", "name": "Main" })]
}

fn tab(id: &str, url: &str) -> Tab {
    Tab {
        tab_id: id.into(),
        url: url.into(),
    }
}

#[test]
fn names_are_served_here() {
    assert!(is_vault_tool("vault_fill_password"));
    assert!(is_vault_tool("vault_save_contact"));
    assert!(!is_vault_tool("vault_list"));
    assert!(!is_vault_tool("projects_save"));
    let defs = tool_defs();
    let names: Vec<&str> = defs.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for n in NAMES {
        assert!(names.contains(&n), "{n} is listed");
    }
}

#[test]
fn login_sites_prefer_the_list_and_drop_empty_ones() {
    let login = json!({ "website": "https://github.com", "websites": ["https://github.com", "not a site", "https://gist.github.com"] });
    assert_eq!(
        login_sites(&login),
        vec!["https://github.com", "https://gist.github.com"]
    );
    assert_eq!(
        login_sites(&json!({ "website": "example.org" })),
        vec!["example.org"]
    );
    assert!(login_sites(&json!({ "website": "" })).is_empty());
}

#[test]
fn domain_matching_refuses_lookalikes() {
    let sites = vec!["https://github.com".to_string()];
    assert!(site_for_page("https://github.com/login", &sites).is_some());
    assert!(site_for_page("https://gist.github.com/x", &sites).is_some());
    assert!(site_for_page("https://github.com.evil.io/login", &sites).is_none());
    assert!(site_for_page("https://githubXcom.io/", &sites).is_none());
    // Plain http is only ever a match when the login was saved as http.
    assert!(site_for_page("http://github.com/", &sites).is_none());
}

#[test]
fn server_tabs_keep_only_server_runtime() {
    let result = json!({ "sessions": [
        { "tabId": "a", "runtime": "server", "navStatus": { "url": "https://github.com/", "title": "x" } },
        { "tabId": "b", "runtime": "browser", "navStatus": { "url": "https://github.com/" } },
        { "runtime": "server" },
    ] });
    assert_eq!(server_tabs(&result), vec![tab("a", "https://github.com/")]);
    assert!(server_tabs(&json!({})).is_empty());
}

#[test]
fn pick_tab_chooses_or_explains() {
    let sites = vec!["https://github.com".to_string()];
    let one = vec![tab("a", "https://example.com/")];
    assert_eq!(pick_tab(&one, None, &sites).unwrap().tab_id, "a");
    assert!(pick_tab(&[], None, &sites)
        .unwrap_err()
        .contains("no browser tab"));
    let two = vec![
        tab("a", "https://example.com/"),
        tab("b", "https://github.com/login"),
    ];
    assert_eq!(pick_tab(&two, None, &sites).unwrap().tab_id, "b");
    assert_eq!(pick_tab(&two, Some("a"), &sites).unwrap().tab_id, "a");
    assert!(pick_tab(&two, Some("zz"), &sites)
        .unwrap_err()
        .contains("isn't open"));
    let ambiguous = vec![
        tab("a", "https://github.com/"),
        tab("b", "https://github.com/x"),
        tab("c", "https://example.com/"),
    ];
    let err = pick_tab(&ambiguous, None, &sites).unwrap_err();
    assert!(err.contains("pass tabId") && err.contains("a (github.com)"));
}

#[test]
fn stream_steps_follow_hbots_order() {
    let you = json!({ "type": "control", "controller": "you", "canOperate": true });
    let other = json!({ "type": "control", "controller": "another-viewer", "canOperate": true });
    let cannot = json!({ "type": "control", "controller": "you", "canOperate": false });
    let probe = json!({ "type": "probe", "x": 1, "y": 1 });
    assert_eq!(step(Stage::Taking, &you), Step::SendText);
    assert!(matches!(step(Stage::Taking, &other), Step::Fail(_)));
    assert!(matches!(step(Stage::Taking, &cannot), Step::Fail(_)));
    assert_eq!(step(Stage::Typing, &probe), Step::Release);
    assert_eq!(step(Stage::Taking, &probe), Step::Ignore);
}

#[test]
fn secret_files_must_be_small_and_inside_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("secrets");
    std::fs::create_dir_all(&root).unwrap();
    let inside = root.join("s1");
    std::fs::write(&inside, "tok-123\n").unwrap();
    let (value, real) = read_secret(&root, inside.to_str().unwrap()).unwrap();
    assert_eq!(value, "tok-123");
    assert_eq!(real, std::fs::canonicalize(&inside).unwrap());

    let outside = dir.path().join("outside");
    std::fs::write(&outside, "nope").unwrap();
    assert!(read_secret(&root, outside.to_str().unwrap())
        .unwrap_err()
        .contains("must be the file"));

    let empty = root.join("empty");
    std::fs::write(&empty, "\n").unwrap();
    assert!(read_secret(&root, empty.to_str().unwrap())
        .unwrap_err()
        .contains("empty"));

    let big = root.join("big");
    std::fs::write(
        &big,
        vec![b'a'; usize::try_from(MAX_SECRET_BYTES).unwrap_or(0) + 1],
    )
    .unwrap();
    assert!(read_secret(&root, big.to_str().unwrap())
        .unwrap_err()
        .contains("isn't a secret"));

    assert!(read_secret(&root, root.join("gone").to_str().unwrap())
        .unwrap_err()
        .contains("doesn't exist"));
    assert!(read_secret(&root, "").is_err());
}

#[test]
fn sweep_removes_only_files_older_than_the_cutoff() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("fresh"), "x").unwrap();
    sweep(dir.path(), SystemTime::now() - Duration::from_secs(3600));
    assert!(dir.path().join("fresh").exists());
    sweep(dir.path(), SystemTime::now() + Duration::from_secs(60));
    assert!(!dir.path().join("fresh").exists());
}

#[tokio::test]
async fn a_contact_is_saved_and_needs_a_name_or_a_way_to_reach_them() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
    let browser = MockHarness::new();
    let b = bots();
    let deps = Deps {
        browser: &browser,
        vault: &vault,
        bots: &b,
        secrets_dir: dir.path(),
        token_dir: dir.path(),
    };
    let saved = dispatch(&deps, "vault_save_contact", &json!({ "threadId": "thread-main", "name": "Ada", "emails": ["ada@example.com"], "city": "London" })).await.unwrap();
    assert_eq!(saved["saved"], true);
    let stored = vault.list().await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0]["type"], "contact");
    assert_eq!(stored[0]["email"], "ada@example.com");
    let err = dispatch(
        &deps,
        "vault_save_contact",
        &json!({ "threadId": "thread-main", "label": "Home" }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("Give at least"));
    let err = dispatch(
        &deps,
        "vault_save_contact",
        &json!({ "threadId": "nobody", "name": "x" }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("isn't a Bot's main conversation"));
}

#[tokio::test]
async fn saving_a_login_moves_the_secret_and_never_returns_it() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = dir.path().join("secrets");
    std::fs::create_dir_all(&secrets).unwrap();
    let vault = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
    let browser = MockHarness::new();
    let b = bots();
    let deps = Deps {
        browser: &browser,
        vault: &vault,
        bots: &b,
        secrets_dir: &secrets,
        token_dir: dir.path(),
    };

    let file = secrets.join("one");
    std::fs::write(&file, "first-secret\n").unwrap();
    let args = json!({ "threadId": "thread-main", "website": "https://github.com", "username": "me", "secretFile": file.to_str().unwrap() });
    let created = dispatch(&deps, "vault_save_login", &args).await.unwrap();
    assert_eq!(created["saved"], true);
    assert_eq!(created["updated"], false);
    assert!(!file.exists(), "the secret file is deleted");
    assert!(!created.to_string().contains("first-secret"));

    // The same site and username updates the login instead of adding one.
    let file2 = secrets.join("two");
    std::fs::write(&file2, "second-secret").unwrap();
    let args2 = json!({ "threadId": "thread-main", "website": "https://github.com", "username": "me", "secretFile": file2.to_str().unwrap(), "apiToken": true });
    let updated = dispatch(&deps, "vault_save_login", &args2).await.unwrap();
    assert_eq!(updated["updated"], true);
    assert_eq!(updated["loginId"], created["loginId"]);
    assert_eq!(updated["apiToken"], true);
    assert_eq!(vault.list().await.unwrap().len(), 1);
    let id = created["loginId"].as_str().unwrap();
    assert_eq!(
        vault.get(id, true).await.unwrap()["password"],
        "second-secret"
    );
}

#[tokio::test]
async fn a_token_file_is_private_and_only_for_api_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let tokens = dir.path().join("bot-secrets");
    let vault = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
    let browser = MockHarness::new();
    let b = bots();
    let deps = Deps {
        browser: &browser,
        vault: &vault,
        bots: &b,
        secrets_dir: dir.path(),
        token_dir: &tokens,
    };
    let site = vault.create(&json!({ "type": "login", "website": "https://ha.local", "username": "", "password": "pw-login" })).await.unwrap();
    let token = vault.create(&json!({ "type": "login", "website": "https://ha.local", "username": "api", "password": "tok-xyz", "botCanRead": true })).await.unwrap();

    let refused = dispatch(
        &deps,
        "vault_secret_file",
        &json!({ "threadId": "thread-main", "loginId": site["id"] }),
    )
    .await
    .unwrap_err();
    assert!(!refused.contains("pw-login"));
    let out = dispatch(
        &deps,
        "vault_secret_file",
        &json!({ "threadId": "thread-main", "loginId": token["id"] }),
    )
    .await
    .unwrap();
    assert!(!out.to_string().contains("tok-xyz"));
    let file = PathBuf::from(out["file"].as_str().unwrap());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "tok-xyz");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn fill_refuses_without_a_bot_or_a_login() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
    let browser = MockHarness::new();
    let b = bots();
    let deps = Deps {
        browser: &browser,
        vault: &vault,
        bots: &b,
        secrets_dir: dir.path(),
        token_dir: dir.path(),
    };
    let err = dispatch(
        &deps,
        "vault_fill_password",
        &json!({ "threadId": "nobody", "loginId": "x" }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("isn't a Bot's main conversation"));
    let err = dispatch(
        &deps,
        "vault_fill_password",
        &json!({ "threadId": "thread-main", "loginId": "x" }),
    )
    .await
    .unwrap_err();
    assert_eq!(err, NO_LOGIN);
}

#[tokio::test]
async fn fills_a_password_through_the_browser_tab() {
    let mut browser = MockHarness::new();
    browser.tabs = vec![
        crate::harness::BrowserTab {
            id: "t-evil".into(),
            url: "https://evil.example/login".into(),
        },
        crate::harness::BrowserTab {
            id: "busy".into(),
            url: "https://github.com/login".into(),
        },
        crate::harness::BrowserTab {
            id: "t-github".into(),
            url: "https://github.com/login".into(),
        },
    ];
    browser.busy.push("busy".into());
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::new(dir.path().join("vault.enc"), dir.path().join(".vault-key"));
    let item = vault
        .create(&json!({ "type": "login", "website": "https://github.com", "username": "me", "password": "s3cret-pass" }))
        .await
        .unwrap();
    let id = item["id"].as_str().unwrap();
    let b = bots();
    let deps = Deps {
        browser: &browser,
        vault: &vault,
        bots: &b,
        secrets_dir: dir.path(),
        token_dir: dir.path(),
    };

    let refused = dispatch(
        &deps,
        "vault_fill_password",
        &json!({ "threadId": "thread-main", "loginId": id, "tabId": "t-evil" }),
    )
    .await
    .unwrap_err();
    assert!(refused.contains("Refused"));
    assert!(!refused.contains("s3cret-pass"));
    assert_eq!(browser.typed("t-evil"), "");

    let busy = dispatch(
        &deps,
        "vault_fill_password",
        &json!({ "threadId": "thread-main", "loginId": id, "tabId": "busy" }),
    )
    .await
    .unwrap_err();
    assert!(busy.contains("controlling this tab"));

    let ok = dispatch(
        &deps,
        "vault_fill_password",
        &json!({ "threadId": "thread-main", "loginId": id, "tabId": "t-github" }),
    )
    .await
    .unwrap();
    assert_eq!(
        ok,
        json!({ "filled": true, "field": "password", "site": "github.com", "tabId": "t-github" })
    );
    assert!(!ok.to_string().contains("s3cret-pass"));
    assert_eq!(browser.typed("t-github"), "s3cret-pass");

    let user = dispatch(
        &deps,
        "vault_fill_username",
        &json!({ "threadId": "thread-main", "loginId": id, "tabId": "t-github" }),
    )
    .await
    .unwrap();
    assert_eq!(user["field"], "username");
    assert_eq!(browser.typed("t-github"), "s3cret-passme");
}
