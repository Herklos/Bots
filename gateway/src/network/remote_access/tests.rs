use super::*;

fn running() -> Value {
    json!({
        "BackendState": "Running",
        "Self": { "DNSName": "box.tail1.ts.net.", "UserID": 7, "CapMap": { "https": null } },
        "User": { "7": { "LoginName": "Me@Example.com" } },
    })
}

#[test]
fn tailscale_status_becomes_the_few_facts_the_setup_needs() {
    let f = parse_tailscale_status(&running());
    assert_eq!(
        f,
        TailscaleFacts {
            state: "Running".into(),
            running: true,
            needs_login: false,
            auth_url: None,
            dns_name: Some("box.tail1.ts.net".into()),
            https: true,
            login: Some("Me@Example.com".into()),
        }
    );
    let login = parse_tailscale_status(
        &json!({ "BackendState": "NeedsLogin", "AuthURL": "https://login.tailscale.com/a/x", "Self": {} }),
    );
    assert!(login.needs_login);
    assert_eq!(
        login.auth_url.as_deref(),
        Some("https://login.tailscale.com/a/x")
    );
    assert!(
        !parse_tailscale_status(
            &json!({ "BackendState": "Running", "Self": { "CapMap": {} }, "CertDomains": [] })
        )
        .https
    );
    assert!(
        parse_tailscale_status(
            &json!({ "BackendState": "Running", "CertDomains": ["box.ts.net"] })
        )
        .https
    );
}

#[test]
fn serve_gets_a_free_https_port_and_reuses_a_route_to_this_listener() {
    let serve = json!({
        "TCP": { "443": { "HTTPS": true }, "8443": { "HTTPS": true } },
        "Web": {
            "box:443": { "Handlers": { "/": { "Proxy": "http://127.0.0.1:8770" } } },
            "box:8443": { "Handlers": { "/": { "Proxy": "http://127.0.0.1:3773" } } },
        },
    });
    assert_eq!(
        pick_serve_port(&serve, "http://127.0.0.1:3781"),
        Some((8444, false))
    );
    assert_eq!(
        pick_serve_port(&json!({}), "http://127.0.0.1:3781"),
        Some((443, false))
    );
    let mut mine = serve;
    mine["Web"]["box:8444"] = json!({ "Handlers": { "/": { "Proxy": "http://127.0.0.1:3781" } } });
    assert_eq!(
        pick_serve_port(&mine, "http://127.0.0.1:3781"),
        Some((8444, true))
    );
    assert_eq!(
        serve_owner(&mine, 8443).as_deref(),
        Some("http://127.0.0.1:3773")
    );
    assert_eq!(tailnet_url("box.ts.net", 443), "https://box.ts.net");
    assert_eq!(tailnet_url("box.ts.net", 8444), "https://box.ts.net:8444");
    let all_taken = json!({ "Web": { "box:443": {}, "box:8443": {}, "box:8444": {}, "box:8445": {}, "box:9443": {} } });
    assert_eq!(pick_serve_port(&all_taken, "http://127.0.0.1:3781"), None);
}

#[tokio::test]
async fn passwords_are_stored_as_scrypt_hashes_and_checked_against_them() {
    let stored = hash_password("correct horse".into()).await.unwrap();
    assert!(!serde_json::to_string(&stored)
        .unwrap()
        .contains("correct horse"));
    assert!(verify_password("correct horse".into(), stored.clone()).await);
    assert!(!verify_password("correct hors".into(), stored).await);
}

#[test]
fn set_at_is_an_iso_timestamp() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    let now = iso_now();
    assert!(
        now.len() == 24 && now.ends_with('Z') && now.as_bytes()[10] == b'T',
        "{now}"
    );
}

#[test]
fn the_quick_link_is_found_in_cloudflared_output() {
    assert_eq!(
        find_quick_url("INF |  https://brave-otter-12.trycloudflare.com  |").as_deref(),
        Some("https://brave-otter-12.trycloudflare.com")
    );
    assert_eq!(find_quick_url("https://example.com and nothing else"), None);
    assert_eq!(
        find_quick_url("https://Bad-Case.trycloudflare.com").as_deref(),
        None
    );
}

#[test]
fn the_pinned_cloudflared_assets_are_the_ones_the_node_gateway_pins() {
    let linux = cloudflared_asset("linux", "x64").unwrap();
    assert!(!linux.tgz);
    assert_eq!(
        linux.sha256,
        "ae5e8add7884b0531d66bbca4570d1de491ff6da51e4ebe7eb160cdf241d9e42"
    );
    assert!(linux.url.ends_with("/cloudflared-linux-amd64"));
    assert!(cloudflared_asset("darwin", "arm64").unwrap().tgz);
    assert!(cloudflared_asset("windows", "x64").is_none());
}

#[test]
fn a_download_that_doesnt_match_its_checksum_is_refused() {
    let sum = hex::encode(Sha256::digest(b"tool"));
    assert!(verify_sha256(b"tool", &sum).is_ok());
    let err = verify_sha256(b"other", &sum).unwrap_err().to_string();
    assert!(err.contains("checksum mismatch"), "{err}");
}

/// An access.json written by Node (`gateway/tests/fixtures/t4/access.json`) is read and checked here.
#[tokio::test]
async fn a_node_written_access_json_verifies_in_rust() {
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join("access.json");
    std::fs::write(
        &store,
        include_str!("../../../tests/fixtures/t4/access.json"),
    )
    .unwrap();
    let remote = RemoteAccess::new(
        RemoteConfig {
            home: dir.path().to_path_buf(),
            store_file: store,
            url_file: dir.path().join("tunnel-url"),
            local_port: 3780,
            env_password: String::new(),
            env_tailnet: false,
            env_tailnet_users: vec![],
            tailscale_bin: None,
            network_env: None,
            process_host: None,
            merged_host: None,
        },
        Arc::new(Tailnet::new()),
    );
    assert!(remote.password_required());
    assert!(remote.password_matches("correct horse").await);
    assert!(!remote.password_matches("correct hors").await);
    let status = remote.status().await;
    assert_eq!(status["password"]["set"], true);
    assert_eq!(status["tailscale"]["users"], json!(["me@example.com"]));
    assert_eq!(status["tailscale"]["enabled"], true);
}

#[test]
fn the_saved_file_keeps_the_node_gateways_field_names() {
    let state = AccessState {
        password: Some(StoredPassword {
            salt: "s".into(),
            hash: "h".into(),
            set_at: "t".into(),
        }),
        tailscale: Some(TailscaleSaved {
            enabled: true,
            port: Some(3781),
            https_port: Some(443),
            users: Some(vec!["me@example.com".into()]),
        }),
        tunnel: Some(TunnelSaved { enabled: false }),
        network: None,
    };
    let text = serde_json::to_value(&state).unwrap();
    assert_eq!(text["password"]["setAt"], "t");
    assert_eq!(text["tailscale"]["httpsPort"], 443);
    assert_eq!(text["tunnel"]["enabled"], false);
    // A file written by the Node gateway reads back the same way.
    let node_written = r#"{"password":{"salt":"s","hash":"h","setAt":"t"},"tailscale":{"enabled":false,"port":3781,"httpsPort":443,"users":["me@example.com"]},"tunnel":{"enabled":true}}"#;
    let read: AccessState = serde_json::from_str(node_written).unwrap();
    assert!(read.tunnel.unwrap().enabled);
    assert_eq!(
        read.tailscale.unwrap().users.unwrap(),
        vec!["me@example.com"]
    );
}

fn remote_at(dir: &Path, network_env: Option<String>) -> RemoteAccess {
    RemoteAccess::new(
        RemoteConfig {
            home: dir.to_path_buf(),
            store_file: dir.join("access.json"),
            url_file: dir.join("tunnel-url"),
            local_port: 3780,
            env_password: String::new(),
            env_tailnet: false,
            env_tailnet_users: vec![],
            tailscale_bin: None,
            network_env,
            process_host: None,
            merged_host: Some("127.0.0.1".into()),
        },
        Arc::new(Tailnet::new()),
    )
}

#[tokio::test]
async fn lan_is_saved_only_with_a_password_and_waits_for_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let remote = remote_at(dir.path(), None);
    remote.note_listener("127.0.0.1", None, false);
    let refused = remote.set_network("lan", Router::new()).await.unwrap_err();
    assert_eq!(refused.code, Some("needs-password"));
    assert!(remote.saved().network.is_none());

    remote
        .set_password(Some("correct horse".into()))
        .await
        .unwrap();
    remote.set_network("lan", Router::new()).await.unwrap();
    let status = remote.status().await;
    assert_eq!(status["network"]["mode"], "lan");
    assert_eq!(status["network"]["locked"], false);
    assert_eq!(status["network"]["restartRequired"], true);
    assert_eq!(status["network"]["http"], true);
    assert!(remote.set_password(None).await.is_err());

    remote.set_network("local", Router::new()).await.unwrap();
    remote.set_password(None).await.unwrap();
    assert_eq!(remote.saved().network.as_deref(), Some("local"));

    let locked = remote_at(dir.path(), Some("local".into()));
    let err = locked.set_network("lan", Router::new()).await.unwrap_err();
    assert!(err.message.contains("environment"), "{}", err.message);
}
