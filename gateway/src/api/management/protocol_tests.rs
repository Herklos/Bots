use super::*;

#[tokio::test]
async fn transport_distinguishes_structure_errors_from_tool_failures() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    storage::write_json(
        &s.settings_path(),
        &json!({"managementMcp":{"enabled":true}}),
    )
    .await
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert(header::HOST, "localhost:3780".parse().unwrap());
    let Json(issued) = credentials_create(
        State(s.clone()),
        headers.clone(),
        Bytes::from(json!({"label":"Protocol fixture","scopes":["admin"]}).to_string()),
    )
    .await
    .unwrap();
    headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", issued["token"].as_str().unwrap())
            .parse()
            .unwrap(),
    );
    let peer = "127.0.0.1:1234".parse().unwrap();
    for (request, code) in [
        (json!([]), -32600),
        (json!({"jsonrpc":"2.0","id":[],"method":"ping"}), -32600),
        (
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            -32602,
        ),
        (
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":42}}),
            -32602,
        ),
        (
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hbots_list_bots","arguments":[]}}),
            -32602,
        ),
        (
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"unknown"}}),
            -32602,
        ),
        (
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"cursor":"unknown"}}),
            -32602,
        ),
    ] {
        let response = serve(
            State(s.clone()),
            ConnectInfo(peer),
            Method::POST,
            headers.clone(),
            Bytes::from(request.to_string()),
        )
        .await;
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1 << 20)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["error"]["code"], code, "{request}");
        if code == -32600 {
            assert!(body["id"].is_null());
        }
    }
    let notification = serve(
        State(s.clone()),
        ConnectInfo(peer),
        Method::POST,
        headers.clone(),
        Bytes::from_static(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
    )
    .await;
    assert_eq!(notification.status(), 202);
    assert!(axum::body::to_bytes(notification.into_body(), 1024)
        .await
        .unwrap()
        .is_empty());
    let Json(_) = credentials_delete(
        State(s.clone()),
        Path(issued["id"].as_str().unwrap().into()),
    )
    .await
    .unwrap();
    let response = serve(
        State(s),
        ConnectInfo(peer),
        Method::POST,
        headers,
        Bytes::from_static(br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#),
    )
    .await;
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn revoked_actor_cannot_execute_a_previously_authorized_operation() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    storage::write_json(
        &s.settings_path(),
        &json!({"managementMcp":{"enabled":true}}),
    )
    .await
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    let Json(issued) = credentials_create(
        State(s.clone()),
        headers.clone(),
        Bytes::from(json!({"label":"Actor fixture","scopes":["admin"]}).to_string()),
    )
    .await
    .unwrap();
    headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", issued["token"].as_str().unwrap())
            .parse()
            .unwrap(),
    );
    let actor = credentials::authenticate(&s, &headers).await.unwrap();
    let Json(_) = credentials_delete(
        State(s.clone()),
        Path(issued["id"].as_str().unwrap().into()),
    )
    .await
    .unwrap();
    assert!(execute_as(
        &s,
        "127.0.0.1:1234".parse().unwrap(),
        &json!({"name":"hbots_create_bot","arguments":{"body":{"name":"Revoked"}}}),
        &actor
    )
    .await
    .is_err());
    assert!(s.bots.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn tailnet_proxy_cannot_expose_mcp_even_when_its_peer_and_host_are_loopback() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    storage::write_json(
        &s.settings_path(),
        &json!({"managementMcp":{"enabled":true}}),
    )
    .await
    .unwrap();
    let router = crate::network::exposure::on_tailnet(super::super::router::app(s));
    let mut request = axum::http::Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("host", "localhost:3780")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{}"))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo("127.0.0.1:1234".parse::<SocketAddr>().unwrap()));
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn malformed_credential_storage_is_preserved_and_new_tokens_default_to_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    let Json(created) = credentials_create(
        State(s.clone()),
        h.clone(),
        Bytes::from_static(br#"{"label":"Default"}"#),
    )
    .await
    .unwrap();
    h.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", created["token"].as_str().unwrap())
            .parse()
            .unwrap(),
    );
    let actor = credentials::authenticate(&s, &h).await.unwrap();
    assert_eq!(actor.scopes, vec!["read"]);
    let path = s.cfg.app_dir().join("management-mcp-credentials.json");
    let mut records: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    records[created["id"].as_str().unwrap()] = json!("broken record");
    std::fs::write(&path, records.to_string()).unwrap();
    assert!(execute_as(
        &s,
        "127.0.0.1:1234".parse().unwrap(),
        &json!({"name":"hbots_create_bot","arguments":{"body":{"name":"Corrupt credential"}}}),
        &actor
    )
    .await
    .is_err());
    assert!(s.bots.list().await.unwrap().is_empty());
    std::fs::write(&path, "broken fixture").unwrap();
    assert!(credentials_create(
        State(s.clone()),
        h.clone(),
        Bytes::from_static(br#"{"label":"Cannot overwrite"}"#)
    )
    .await
    .is_err());
    assert!(credentials::authenticate(&s, &h).await.is_none());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "broken fixture");
}
