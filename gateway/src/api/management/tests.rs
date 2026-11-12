use super::*;

#[test]
fn catalog_covers_application_routes_without_duplicate_names() {
    let (_, operations) = super::super::router::application_routes();
    let mut names = std::collections::HashSet::new();
    for op in &operations {
        assert!(names.insert(&op.name), "duplicate {}", op.name);
        assert!(op.definition()["inputSchema"].is_object());
    }
    assert!(operations.len() > 80);
    for name in [
        "hbots_create_bot",
        "hbots_save_bot",
        "hbots_rooms_create",
        "hbots_room_update",
        "hbots_system_status",
    ] {
        assert!(names.iter().any(|n| n.as_str() == name), "missing {name}");
    }
    assert!(!operations
        .iter()
        .any(|o| o.path.starts_with("/t3") || o.path.contains("credentials")));
}

#[test]
fn ingress_rejects_remote_proxy_and_dns_rebinding() {
    let peer = "127.0.0.1:1234".parse().unwrap();
    let mut h = HeaderMap::new();
    h.insert("host", "127.0.0.1:3780".parse().unwrap());
    assert!(local(&h, peer));
    assert!(!local(&h, "192.0.2.1:1234".parse().unwrap()));
    h.insert("origin", "https://attacker.example".parse().unwrap());
    assert!(!local(&h, peer));
    h.remove("origin");
    h.insert("host", "attacker.example".parse().unwrap());
    assert!(!local(&h, peer));
    h.insert("host", "localhost:3780".parse().unwrap());
    h.insert("forwarded", "for=127.0.0.1".parse().unwrap());
    assert!(!local(&h, peer));
}

#[tokio::test]
async fn generated_dispatch_preserves_bot_crud_and_settings_policy() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    assert_eq!(s.settings().await["managementMcp"]["enabled"], false);
    storage::write_json(
        &s.settings_path(),
        &json!({"managementMcp":{"enabled":true}}),
    )
    .await
    .unwrap();
    let peer = "127.0.0.1:1234".parse().unwrap();
    let created = execute(
        &s,
        peer,
        &json!({"name":"hbots_create_bot","arguments":{"body":{"name":"Ada"}}}),
    )
    .await
    .unwrap();
    let slug = created["body"]["slug"].as_str().unwrap();
    let updated=execute(&s,peer,&json!({"name":"hbots_save_bot","arguments":{"path":{"slug":slug},"body":{"name":"Ada Lovelace"}}})).await.unwrap();
    assert_eq!(updated["body"]["name"], "Ada Lovelace");
    assert!(execute(&s,peer,&json!({"name":"hbots_put_settings","arguments":{"body":{"managementMcp":{"enabled":true}}}})).await.is_err());
    assert!(execute(
        &s,
        peer,
        &json!({"name":"hbots_get_bot","arguments":{"path":{"slug":"../settings"}}})
    )
    .await
    .is_err());
    execute(
        &s,
        peer,
        &json!({"name":"hbots_delete_bot","arguments":{"path":{"slug":slug}}}),
    )
    .await
    .unwrap();
    assert!(s.bots.get(slug).await.unwrap().is_none());
    storage::write_json(&s.settings_path(), &json!({}))
        .await
        .unwrap();
    assert!(
        execute(&s, peer, &json!({"name":"hbots_list_bots","arguments":{}}))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn tokens_are_hashed_revocable_and_separate_from_bot_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    let Json(created) = credentials_create(
        State(s.clone()),
        h.clone(),
        Bytes::from_static(b"{\"label\":\"Test\"}"),
    )
    .await
    .unwrap();
    let token = created["token"].as_str().unwrap();
    h.insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    assert!(credentials::authorized(&s, &h).await);
    let raw =
        std::fs::read_to_string(s.cfg.app_dir().join("management-mcp-credentials.json")).unwrap();
    assert!(!raw.contains(token));
    let Json(list) = credentials_list(State(s.clone())).await.unwrap();
    assert!(!list.to_string().contains("hash"));
    let _ = credentials_delete(
        State(s.clone()),
        Path(created["id"].as_str().unwrap().into()),
    )
    .await
    .unwrap();
    assert!(!credentials::authorized(&s, &h).await);
    h.insert(
        header::AUTHORIZATION,
        "Bearer fixture-only".parse().unwrap(),
    );
    assert!(!credentials::authorized(&s, &h).await);
}

#[test]
fn generated_catalog_is_current() {
    let (_, operations) = super::super::router::application_routes();
    let text = serde_json::to_string_pretty(
        &json!({"generatedBy":"scripts/generate-mcp.sh","tools": operations.iter().map(Operation::definition).collect::<Vec<_>>()}),
    )
    .unwrap()
        + "\n";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs");
    let openapi = serde_json::to_string_pretty(&contracts::openapi(&operations)).unwrap() + "\n";
    let coverage=serde_json::to_string_pretty(&json!({"generatedBy":"scripts/generate-mcp.sh","operations":operations.iter().map(|op| json!({"path":op.path,"method":op.method.to_uppercase(),"tool":op.name,"permission":op.permission(),"classification":if matches!(op.name.as_str(),"hbots_export"|"hbots_import_csv"|"hbots_download"|"hbots_library_file"|"hbots_get_icon") {"adapted-binary"} else {"generated"}})).chain(super::super::router::excluded_operations()).collect::<Vec<_>>()})).unwrap()+"\n";
    for (name, content) in [
        ("management-mcp-tools.json", text),
        ("gateway-openapi.json", openapi),
        ("gateway-api-coverage.json", coverage),
    ] {
        let path = root.join(name);
        if std::env::var_os("HBOTS_GENERATE_MCP").is_some() {
            std::fs::write(&path, &content).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            content,
            "Run scripts/generate-mcp.sh"
        );
    }
}

#[tokio::test]
async fn group_mutations_use_the_existing_rooms_model() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    storage::write_json(
        &s.settings_path(),
        &json!({"managementMcp":{"enabled":true}}),
    )
    .await
    .unwrap();
    let peer = "127.0.0.1:1234".parse().unwrap();
    let ada = s.bots.create(&json!({"name":"Ada"})).await.unwrap();
    let bob = s.bots.create(&json!({"name":"Bob"})).await.unwrap();
    let members = json!([ada["slug"], bob["slug"]]);
    let created = execute(
        &s,
        peer,
        &json!({"name":"hbots_rooms_create","arguments":{"body":{"title":"Team","members":members}}}),
    )
    .await
    .unwrap();
    let id = created["body"]["id"].as_str().unwrap();
    execute(&s,peer,&json!({"name":"hbots_room_update","arguments":{"path":{"id":id},"body":{"title":"Research","members":members}}})).await.unwrap();
    assert_eq!(s.rooms.get(id).await.unwrap()["title"], "Research");
    execute(
        &s,
        peer,
        &json!({"name":"hbots_room_delete","arguments":{"path":{"id":id}}}),
    )
    .await
    .unwrap();
    assert!(s.rooms.get(id).await.is_err());
}

#[tokio::test]
async fn transport_requires_enablement_token_and_json_and_lists_tools() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    let peer = "127.0.0.1:1234".parse().unwrap();
    let mut h = HeaderMap::new();
    h.insert("host", "localhost:3780".parse().unwrap());
    h.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    let call = Bytes::from_static(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}");
    assert_eq!(
        serve(
            State(s.clone()),
            ConnectInfo(peer),
            Method::POST,
            h.clone(),
            call.clone()
        )
        .await
        .status(),
        404
    );
    storage::write_json(
        &s.settings_path(),
        &json!({"managementMcp":{"enabled":true}}),
    )
    .await
    .unwrap();
    assert_eq!(
        serve(
            State(s.clone()),
            ConnectInfo(peer),
            Method::POST,
            h.clone(),
            call.clone()
        )
        .await
        .status(),
        401
    );
    let Json(token) = credentials_create(
        State(s.clone()),
        h.clone(),
        Bytes::from_static(b"{\"label\":\"Client\",\"scopes\":[\"admin\"]}"),
    )
    .await
    .unwrap();
    h.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", token["token"].as_str().unwrap())
            .parse()
            .unwrap(),
    );
    let response = serve(
        State(s.clone()),
        ConnectInfo(peer),
        Method::POST,
        h.clone(),
        call,
    )
    .await;
    assert_eq!(response.status(), 200);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(body["result"]["tools"].as_array().unwrap().len() > 80);
    h.insert("mcp-protocol-version", "unsupported".parse().unwrap());
    assert_eq!(
        serve(
            State(s),
            ConnectInfo(peer),
            Method::POST,
            h,
            Bytes::from_static(b"{}")
        )
        .await
        .status(),
        400
    );
}

#[tokio::test]
async fn binary_attachments_round_trip_without_treating_json_files_as_api_json() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    storage::write_json(
        &s.settings_path(),
        &json!({"managementMcp":{"enabled":true}}),
    )
    .await
    .unwrap();
    let bot = s.bots.create(&json!({"name":"Ada"})).await.unwrap();
    let slug = &bot["slug"];
    let peer = "127.0.0.1:1234".parse().unwrap();
    let file=execute(&s,peer,&json!({"name":"hbots_create","arguments":{"path":{"slug":slug},"body":{"name":"note.json","mediaType":"application/json","data":STANDARD.encode(br#"{"hello":true}"#)}}})).await.unwrap();
    let id = &file["body"]["id"];
    let downloaded = execute(
        &s,
        peer,
        &json!({"name":"hbots_download","arguments":{"path":{"slug":slug,"id":id}}}),
    )
    .await
    .unwrap();
    assert_eq!(
        downloaded["body"]["bodyBase64"],
        STANDARD.encode(br#"{"hello":true}"#)
    );
}

#[tokio::test]
async fn settings_reject_invalid_flags_and_preserve_partial_mcp_patches() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    for body in [
        json!({"managementMcp":true}),
        json!({"managementMcp":{"enabled":"yes"}}),
        json!({"managementMcp":{"unknown":true}}),
    ] {
        assert!(
            put_settings(State(s.clone()), h.clone(), Bytes::from(body.to_string()))
                .await
                .is_err()
        );
    }
    let _ = put_settings(
        State(s.clone()),
        h.clone(),
        Bytes::from_static(b"{\"managementMcp\":{\"enabled\":true}}"),
    )
    .await
    .unwrap();
    let _ = put_settings(
        State(s.clone()),
        h.clone(),
        Bytes::from_static(b"{\"managementMcp\":{}}"),
    )
    .await
    .unwrap();
    assert_eq!(s.settings().await["managementMcp"]["enabled"], true);
    let _ = put_settings(
        State(s.clone()),
        h,
        Bytes::from_static(b"{\"userName\":\"Test\"}"),
    )
    .await
    .unwrap();
    assert_eq!(s.settings().await["managementMcp"]["enabled"], true);
}

#[tokio::test]
async fn scoped_credentials_filter_discovery_and_deny_mutations_without_execution() {
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
        Bytes::from(json!({"label":"Reader", "scopes":["read"]}).to_string()),
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
    assert_eq!(actor.scopes, vec!["read"]);
    let peer = "127.0.0.1:1234".parse().unwrap();
    let response = serve(
        State(s.clone()),
        ConnectInfo(peer),
        Method::POST,
        headers.clone(),
        Bytes::from(json!({"jsonrpc":"2.0", "id":1, "method":"tools/list"}).to_string()),
    )
    .await;
    let data: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    let names: Vec<_> = data["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"hbots_list_bots"));
    for hidden in [
        "hbots_create_bot",
        "hbots_vault_get",
        "hbots_put_settings",
        "hbots_computer_get",
    ] {
        assert!(!names.contains(&hidden));
    }
    assert!(execute_as(
        &s,
        peer,
        &json!({"name":"hbots_create_bot", "arguments":{"body":{"name":"Forbidden"}}}),
        &actor
    )
    .await
    .is_err());
    assert!(s.bots.list().await.unwrap().is_empty());
    let Json(listed) = credentials_list(State(s.clone())).await.unwrap();
    assert_eq!(listed["credentials"][0]["scopes"], json!(["read"]));
    assert!(listed["credentials"][0]["createdAt"].is_string());
    assert!(listed["credentials"][0]["lastUsedAt"].is_string());
    for scopes in [json!([]), json!(["unknown"]), json!([1]), json!("admin")] {
        assert!(credentials_create(
            State(s.clone()),
            headers.clone(),
            Bytes::from(json!({"label":"Invalid", "scopes":scopes}).to_string())
        )
        .await
        .is_err());
    }
}

#[test]
fn every_operation_has_a_known_permission_and_sensitive_reads_are_not_general_reads() {
    let (_, operations) = super::super::router::application_routes();
    for op in operations {
        assert!(permissions::SCOPES.contains(&op.permission()));
        if op.path.starts_with("/api/vault") {
            assert_eq!(op.permission(), "secrets");
        }
        if op.path.contains("/computer") {
            assert_eq!(op.permission(), "computer");
        }
        if op.path.starts_with("/api/storage") || op.path.starts_with("/api/access") {
            assert_eq!(op.permission(), "admin");
        }
    }
    assert!(
        permissions::parse_scopes(Some(&json!(["read", "read"])))
            .unwrap()
            .len()
            == 1
    );
}
