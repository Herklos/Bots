use super::*;

#[tokio::test]
pub(crate) async fn a_broken_connector_file_is_left_unread() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(
        dir.path().to_path_buf(),
        "http://127.0.0.1:3780/mcp/bot".into(),
        "tok".into(),
    );
    let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
    let slug = ada["slug"].as_str().unwrap();
    let folder = dir.path().join(slug);
    let before = tokio::fs::read_to_string(folder.join(".mcp.json"))
        .await
        .unwrap();
    tokio::fs::write(folder.join(".bot").join("connectors.json"), "{")
        .await
        .unwrap();
    assert!(bots.write_instructions(&ada).await.is_err());
    let after = tokio::fs::read_to_string(folder.join(".mcp.json"))
        .await
        .unwrap();
    assert_eq!(before, after);
    assert!(after.contains("\"bot\""));
}

#[tokio::test]
pub(crate) async fn create_select_and_remove() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(
        dir.path().to_path_buf(),
        "http://127.0.0.1:3780/mcp/bot".into(),
        "tok".into(),
    );
    let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
    assert_eq!(ada["primary"], true, "the first Bot is the main Bot");
    let bob = bots.create(&json!({ "name": "Ada" })).await.unwrap();
    assert_eq!(bob["slug"], "ada-2");
    assert!(
        bots.remove("ada").await.is_err(),
        "the main Bot can't be deleted while others exist"
    );
    let list = bots.select("ada-2").await.unwrap();
    assert_eq!(list.iter().filter(|b| b["primary"] == true).count(), 1);
    bots.remove("ada").await.unwrap();
    assert_eq!(bots.list().await.unwrap().len(), 1);
    let agents = std::fs::read_to_string(dir.path().join("ada-2/AGENTS.md")).unwrap();
    assert!(agents.starts_with(MANAGED_START));
    let mcp: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("ada-2/.mcp.json")).unwrap())
            .unwrap();
    assert_eq!(
        mcp["mcpServers"]["bot"]["headers"]["Authorization"],
        "Bearer tok"
    );
}

#[tokio::test]
pub(crate) async fn an_unreadable_main_bot_choice_is_an_error_and_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(
        dir.path().to_path_buf(),
        "http://127.0.0.1:3780/mcp/bot".into(),
        "tok".into(),
    );
    bots.create(&json!({ "name": "Ada" })).await.unwrap();
    let choice = dir.path().join(".botapp/main-bot.json");
    std::fs::write(&choice, "{not json").unwrap();
    assert!(bots.list().await.is_err());
    assert_eq!(std::fs::read_to_string(&choice).unwrap(), "{not json");
}

pub(crate) fn status_of(err: ApiError) -> u16 {
    err.status.as_u16()
}

#[tokio::test]
pub(crate) async fn a_bot_record_defaults_role_and_keeps_a_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots
        .create(&json!({ "name": "Ada", "prompt": "Be brief." }))
        .await
        .unwrap();
    assert_eq!(ada["role"], "bot");
    assert!(ada["parentSlug"].is_null());
    assert_eq!(ada["hideFromSidebar"], false);
    assert!(ada["subagent"].is_null());
    assert_eq!(ada["prompt"], "Be brief.");
    assert!(dir.path().join("ada/skills").is_dir());
    assert_eq!(
        std::fs::read_dir(dir.path().join("ada/skills"))
            .unwrap()
            .count(),
        0
    );
    let saved = bots
        .save("ada", &json!({ "prompt": "Be shorter." }), false)
        .await
        .unwrap();
    assert_eq!(saved["prompt"], "Be shorter.");
    assert!(bots
        .save("ada", &json!({ "hideFromSidebar": true }), false)
        .await
        .is_err_and(|e| status_of(e) == 409));
    assert!(bots.create(&json!({})).await.is_err());
}

#[tokio::test]
pub(crate) async fn a_child_is_a_bot_with_a_parent_and_one_level() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots
        .create(&json!({ "name": "Ada", "shape": "cloud", "color": "#3366cc" }))
        .await
        .unwrap();
    let junior = bots
        .create(&json!({
            "parentSlug": "ada",
            "pinned": true,
            "primary": true,
            "subagent": { "prompt": "Look things up.", "inheritParentPrompt": true, "toolAllow": ["read"] }
        }))
        .await
        .unwrap();
    assert_eq!(junior["name"], "Ada junior 1");
    assert_eq!(junior["slug"], "ada-junior-1");
    assert_eq!(junior["role"], "subagent");
    assert_eq!(junior["parentSlug"], "ada");
    assert_eq!(junior["hideFromSidebar"], true);
    assert_eq!(junior["hidden"], false);
    assert_eq!(junior["pinned"], false);
    assert_eq!(junior["primary"], false);
    assert_eq!(junior["shape"], "cloud");
    assert_eq!(junior["color"], "#3366cc");
    assert_eq!(junior["subagent"]["prompt"], "Look things up.");
    assert_eq!(junior["subagent"]["inheritParentPrompt"], true);
    assert_eq!(junior["subagent"]["toolAllow"], json!(["read"]));
    assert!(dir.path().join("ada-junior-1/skills").is_dir());
    assert_eq!(ada["primary"], true);

    let named = bots
        .create(
            &json!({ "name": "Research", "parentSlug": "ada", "shape": "hex", "color": "#111111" }),
        )
        .await
        .unwrap();
    assert_eq!(named["name"], "Research");
    assert_eq!(named["slug"], "research");
    assert_eq!(named["shape"], "hex");
    assert_eq!(named["color"], "#111111");

    let second = bots.create(&json!({ "parentSlug": "ada" })).await.unwrap();
    assert_eq!(second["name"], "Ada junior 2");
    let taken = bots
        .create(&json!({ "name": "Ada junior 1", "parentSlug": "ada" }))
        .await
        .unwrap();
    assert_eq!(taken["slug"], "ada-junior-1-2");

    let nested = bots
        .create(&json!({ "name": "Nope", "parentSlug": "ada-junior-1" }))
        .await
        .unwrap_err();
    assert_eq!(status_of(nested), 409);
    let missing = bots
        .create(&json!({ "name": "Nope", "parentSlug": "missing" }))
        .await
        .unwrap_err();
    assert_eq!(status_of(missing), 400);

    let replaced = bots
        .save(
            "ada-junior-1",
            &json!({ "subagent": { "prompt": "New.", "cap": 3 } }),
            false,
        )
        .await
        .unwrap();
    assert_eq!(replaced["subagent"]["prompt"], "New.");
    assert_eq!(replaced["subagent"]["cap"], 3);
    assert!(replaced["subagent"].get("toolAllow").is_none());
    assert!(replaced["subagent"].get("inheritParentPrompt").is_none());

    for patch in [
        json!({ "pinned": true }),
        json!({ "hidden": true }),
        json!({ "primary": true }),
        json!({ "hideFromSidebar": false }),
        json!({ "parentSlug": "research" }),
        json!({ "role": "bot" }),
    ] {
        let err = bots.save("ada-junior-1", &patch, false).await.unwrap_err();
        assert_eq!(status_of(err), 409, "{patch}");
    }
    let same = bots
        .save(
            "ada-junior-1",
            &json!({ "role": "subagent", "parentSlug": "ada", "hideFromSidebar": true, "name": "Scout" }),
            false,
        )
        .await
        .unwrap();
    assert_eq!(same["name"], "Scout");
    assert_eq!(same["parentSlug"], "ada");

    assert_eq!(
        status_of(bots.select("ada-junior-1").await.unwrap_err()),
        409
    );
    assert_eq!(status_of(bots.remove("ada").await.unwrap_err()), 409);
    bots.remove("ada-junior-1").await.unwrap();
    bots.remove("research").await.unwrap();
    bots.remove("ada-junior-2").await.unwrap();
    bots.remove(taken["slug"].as_str().unwrap()).await.unwrap();
    assert!(bots.remove("ada").await.is_ok());
}

#[tokio::test]
pub(crate) async fn reconcile_does_not_make_a_child_the_main_bot() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    bots.create(&json!({ "name": "Ada" })).await.unwrap();
    bots.create(&json!({ "parentSlug": "ada" })).await.unwrap();
    std::fs::write(
        dir.path().join(".botapp/main-bot.json"),
        r#"{"slug":"ada-junior-1"}"#,
    )
    .unwrap();
    let list = bots.list().await.unwrap();
    let main = list.iter().find(|b| b["primary"] == true).unwrap();
    assert_eq!(main["slug"], "ada");
    assert_eq!(
        list.iter().find(|b| b["slug"] == "ada-junior-1").unwrap()["primary"],
        false
    );
}

#[test]
pub(crate) fn a_child_block_names_its_parent_and_its_own_folders() {
    let parent = json!({ "name": "Ada", "slug": "ada", "prompt": "Be brief." });
    let child = json!({
        "name": "Scout",
        "slug": "scout",
        "role": "subagent",
        "parentSlug": "ada",
        "subagent": { "prompt": "Look things up.", "inheritParentPrompt": false }
    });
    let text = render_agents_md(
        &child,
        std::path::Path::new("/bots"),
        Some(&parent),
        &[],
        &MemorySnapshot::default(),
    );
    assert!(text.starts_with(MANAGED_START) && text.contains(MANAGED_END));
    assert!(text.contains("a subagent of Ada"));
    assert!(text.contains("Ada assigns your work"));
    assert!(text.contains("`memory/`") && text.contains("`skills/`"));
    assert!(!text.contains("Be brief."));
}

#[test]
pub(crate) fn a_child_block_includes_the_parent_prompt_field_when_asked() {
    let parent = json!({
        "name": "Ada",
        "slug": "ada",
        "prompt": "Be brief. This is the prompt field."
    });
    let child = json!({
        "name": "Scout",
        "role": "subagent",
        "parentSlug": "ada",
        "subagent": { "inheritParentPrompt": true, "prompt": "Look things up." }
    });
    let text = render_agents_md(
        &child,
        std::path::Path::new("/bots"),
        Some(&parent),
        &[],
        &MemorySnapshot::default(),
    );
    assert!(text.contains("Be brief. This is the prompt field."));
    let quiet = json!({
        "name": "Scout",
        "role": "subagent",
        "parentSlug": "ada",
        "subagent": { "inheritParentPrompt": false }
    });
    let without = render_agents_md(
        &quiet,
        std::path::Path::new("/bots"),
        Some(&parent),
        &[],
        &MemorySnapshot::default(),
    );
    assert!(!without.contains("Be brief."));
}
