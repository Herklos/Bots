use super::*;

#[test]
pub(crate) fn slugs() {
    assert_eq!(slugify("Ada  Lovelace"), "ada-lovelace");
    assert_eq!(slugify("Élodie_B"), "elodie-b");
    assert_eq!(slugify("!!!"), "bot");
    assert!(valid_slug("ada-2") && !valid_slug("-ada") && !valid_slug("../x"));
}

#[test]
pub(crate) fn shapes_come_from_the_design_crate_and_sphere_reads_as_the_ball() {
    assert_eq!(shape_name("hex").as_deref(), Some("hex"));
    assert_eq!(shape_name("sphere").as_deref(), Some("ball"));
    assert_eq!(shape_name("blob"), None);
    assert_eq!(shape_name(""), None);
}

#[tokio::test]
pub(crate) async fn a_bot_takes_a_shape_and_refuses_an_unknown_one() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots
        .create(&json!({ "name": "Ada", "shape": "cloud" }))
        .await
        .unwrap();
    assert_eq!(ada["shape"], "cloud");
    let plain = bots.create(&json!({ "name": "Bob" })).await.unwrap();
    assert_eq!(plain["shape"], "ball");
    assert!(bots
        .create(&json!({ "name": "Cy", "shape": "blob" }))
        .await
        .is_err());
    let slug = ada["slug"].as_str().unwrap();
    let saved = bots
        .save(slug, &json!({ "shape": "sphere" }), false)
        .await
        .unwrap();
    assert_eq!(saved["shape"], "ball");
    assert!(bots
        .save(slug, &json!({ "shape": "blob" }), false)
        .await
        .is_err());
}

#[tokio::test]
pub(crate) async fn only_two_bots_besides_the_main_one_can_be_pinned() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let mut slugs = Vec::new();
    for name in ["Main", "One", "Two", "Three"] {
        slugs.push(
            bots.create(&json!({ "name": name })).await.unwrap()["slug"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    bots.select(&slugs[0]).await.unwrap();
    assert!(bots
        .save(&slugs[1], &json!({ "pinned": true }), false)
        .await
        .is_ok());
    assert!(bots
        .save(&slugs[2], &json!({ "pinned": true }), false)
        .await
        .is_ok());
    let third = bots
        .save(&slugs[3], &json!({ "pinned": true }), false)
        .await
        .unwrap_err();
    assert_eq!(third.status, 409);
    // Pinning one already pinned is fine, and unpinning makes room.
    assert!(bots
        .save(&slugs[1], &json!({ "pinned": true }), false)
        .await
        .is_ok());
    bots.save(&slugs[1], &json!({ "pinned": false }), false)
        .await
        .unwrap();
    assert!(bots
        .save(&slugs[3], &json!({ "pinned": true }), false)
        .await
        .is_ok());
    assert!(bots
        .save(&slugs[0], &json!({ "pinned": "yes" }), false)
        .await
        .is_err());
}

#[tokio::test]
pub(crate) async fn notifications_must_be_a_boolean_and_a_subagent_can_store_false() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
    let slug = ada["slug"].as_str().unwrap();
    let bad = bots
        .save(slug, &json!({ "notifications": "no" }), false)
        .await
        .unwrap_err();
    assert_eq!(status_of(bad), 400);
    assert_eq!(
        bots.get(slug).await.unwrap().unwrap()["notifications"],
        true
    );
    let saved = bots
        .save(slug, &json!({ "notifications": false }), false)
        .await
        .unwrap();
    assert_eq!(saved["notifications"], false);

    let junior = bots.create(&json!({ "parentSlug": slug })).await.unwrap();
    let child = junior["slug"].as_str().unwrap();
    let muted = bots
        .save(child, &json!({ "notifications": false }), false)
        .await
        .unwrap();
    assert_eq!(muted["role"], "subagent");
    assert_eq!(muted["notifications"], false);
    let child_bad = bots
        .save(child, &json!({ "notifications": "off" }), false)
        .await
        .unwrap_err();
    assert_eq!(status_of(child_bad), 400);
}

#[tokio::test]
pub(crate) async fn a_new_bot_starts_seen_and_remembers_its_first_message() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots
        .create(&json!({ "name": "Ada", "firstMessage": "  Plan my day.  " }))
        .await
        .unwrap();
    assert!(ada["lastSeenAt"].as_str().is_some_and(|t| !t.is_empty()));
    assert_eq!(ada["firstMessage"], "Plan my day.");
}

#[tokio::test]
pub(crate) async fn create_accepts_a_runtime_mode_and_rejects_an_unknown_one() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots
        .create(&json!({ "name": "Ada", "runtimeMode": "auto" }))
        .await
        .unwrap();
    assert_eq!(ada["runtimeMode"], "auto");
    let bea = bots
        .create(&json!({ "name": "Bea", "runtimeMode": "independent" }))
        .await
        .unwrap();
    assert_eq!(bea["runtimeMode"], "independent");
    let err = bots
        .create(&json!({ "name": "Cy", "runtimeMode": "ask" }))
        .await
        .unwrap_err();
    assert_eq!(status_of(err), 400);
    assert!(bots.get("cy").await.unwrap().is_none());
    let patched = bots
        .save("ada", &json!({ "runtimeMode": "independent" }), false)
        .await
        .unwrap();
    assert_eq!(patched["runtimeMode"], "independent");
    let garbage = bots
        .save("ada", &json!({ "runtimeMode": "nope" }), false)
        .await
        .unwrap_err();
    assert_eq!(status_of(garbage), 400);
    assert_eq!(
        bots.get("ada").await.unwrap().unwrap()["runtimeMode"],
        "independent"
    );
}

#[tokio::test]
pub(crate) async fn skipping_the_new_fields_matches_todays_bot_record() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
    assert!(ada.get("runtimeMode").is_none());
    assert!(ada.get("code").is_none());
    assert!(ada.get("goal").is_none());
    assert!(ada.get("pendingRoutines").is_none());
    assert_eq!(ada["prompt"], "");
    assert!(ada["mainThreadId"].is_null());
    assert_eq!(ada["shape"], "ball");
    assert_eq!(ada["role"], "bot");
    assert_eq!(ada["pinned"], false);
    assert_eq!(ada["hidden"], false);
    assert_eq!(ada["primary"], true);
    assert_eq!(ada["notifications"], true);
    assert_eq!(ada["hideFromSidebar"], false);
}

#[tokio::test]
pub(crate) async fn a_goal_is_stored_on_its_own_and_routines_wait_for_the_thread() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots
        .create(&json!({
            "name": "Ada",
            "goal": "  Ship the weekly note.  ",
            "prompt": "Be brief.",
            "pendingRoutines": [
                { "preset": "weekdays", "prompt": "Send the note." },
                { "time": "08:30", "weekday": 1, "prompt": "Plan Monday." }
            ]
        }))
        .await
        .unwrap();
    assert_eq!(ada["goal"], "Ship the weekly note.");
    assert_eq!(ada["prompt"], "Be brief.");
    assert_eq!(ada["pendingRoutines"].as_array().unwrap().len(), 2);
    assert!(ada["mainThreadId"].is_null());
    let agents = std::fs::read_to_string(dir.path().join("ada/AGENTS.md")).unwrap();
    assert!(agents.contains("## Goal"));
    assert!(agents.contains("Ship the weekly note."));
    assert_eq!(
        agents.matches("Ship the weekly note.").count(),
        1,
        "the goal stays in the managed block"
    );
    let err = bots
        .create(&json!({
            "name": "Cy",
            "pendingRoutines": [{ "preset": "hourly", "prompt": "Go." }]
        }))
        .await
        .unwrap_err();
    assert_eq!(status_of(err), 400);
}

#[test]
pub(crate) fn the_managed_block_tells_a_bot_about_routines_and_other_bots() {
    let text = render_agents_md(
        &json!({ "name": "Ada" }),
        std::path::Path::new("/bots"),
        None,
        &[],
        &MemorySnapshot::default(),
        &Value::Null,
    );
    assert!(text.contains("schedule_task") && text.contains("t3_thread_send"));
    assert!(text.contains("Avoid Markdown tables. Prefer bullet lists."));
    assert!(text.contains("## Computer") && text.contains("[Computer]"));
    assert!(text.contains("https://www.google.com"));
    assert!(text.contains("Sunday") && text.starts_with(MANAGED_START));
}

#[test]
pub(crate) fn managed_block_keeps_user_text() {
    let first = merge_managed("", "<!-- bot:managed:start -->A<!-- bot:managed:end -->\n");
    let user = format!("{first}\nMy own notes\n");
    let next = merge_managed(
        &user,
        "<!-- bot:managed:start -->B<!-- bot:managed:end -->\n",
    );
    assert!(next.contains('B') && !next.contains(">A<") && next.contains("My own notes"));
}

#[tokio::test]
pub(crate) async fn instructions_keep_enabled_connectors() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(
        dir.path().to_path_buf(),
        "http://127.0.0.1:3780/mcp/bot".into(),
        "tok".into(),
    );
    let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
    let slug = ada["slug"].as_str().unwrap();
    let folder = dir.path().join(slug);
    let connectors = json!({
        "connectors": [
            { "name": "notes", "url": "http://127.0.0.1:9/mcp", "enabled": true, "secret": "s3cret" },
            { "name": "off", "url": "https://example.com/mcp", "enabled": false, "secret": "hidden" }
        ]
    });
    tokio::fs::create_dir_all(folder.join(".bot"))
        .await
        .unwrap();
    tokio::fs::write(
        folder.join(".bot").join("connectors.json"),
        serde_json::to_vec(&connectors).unwrap(),
    )
    .await
    .unwrap();
    bots.write_instructions(&ada).await.unwrap();
    let mcp: Value = serde_json::from_str(
        &tokio::fs::read_to_string(folder.join(".mcp.json"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(mcp["mcpServers"]["bot"]["url"]
        .as_str()
        .unwrap()
        .contains("/mcp/bot"));
    assert_eq!(mcp["mcpServers"]["notes"]["url"], "http://127.0.0.1:9/mcp");
    assert!(mcp["mcpServers"]["notes"]["headers"]["Authorization"]
        .as_str()
        .unwrap()
        .contains("s3cret"));
    assert!(mcp["mcpServers"].get("off").is_none());
    assert!(!mcp.to_string().contains("hidden"));
    let toml = tokio::fs::read_to_string(folder.join(".codex").join("config.toml"))
        .await
        .unwrap();
    assert!(toml.contains("[mcp_servers.bot]"));
    assert!(toml.contains("[mcp_servers.notes]"));
    assert!(!toml.contains("[mcp_servers.off]"));
    assert!(!toml.contains("hidden"));
    let cs: Value = serde_json::from_str(
        &tokio::fs::read_to_string(folder.join(".claude").join("settings.json"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(cs["enableAllProjectMcpServers"], true);
    let disabled = cs["disabledMcpjsonServers"].as_array().unwrap();
    assert!(disabled.iter().any(|n| n == "off"));
    assert!(!disabled.iter().any(|n| n == "notes" || n == "bot"));
    assert!(!cs.to_string().contains("hidden"));
}
