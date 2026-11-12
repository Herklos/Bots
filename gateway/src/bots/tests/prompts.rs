use super::*;

#[tokio::test]
pub(crate) async fn a_main_prompt_is_listed_written_and_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    bots.create(&json!({ "name": "Ada", "prompt": "Be brief." }))
        .await
        .unwrap();
    let created = std::fs::read_to_string(dir.path().join("ada/AGENTS.md")).unwrap();
    assert!(created.contains("## Instructions\n\nBe brief."));
    let listed = bots.list().await.unwrap();
    let ada = listed.iter().find(|b| b["slug"] == "ada").unwrap();
    assert_eq!(ada["prompt"], "Be brief.");
    bots.create(&json!({
        "parentSlug": "ada",
        "name": "Scout",
        "subagent": { "inheritParentPrompt": true, "prompt": "Look things up." }
    }))
    .await
    .unwrap();
    bots.save("ada", &json!({ "prompt": "Be shorter." }), false)
        .await
        .unwrap();
    let agents = std::fs::read_to_string(dir.path().join("ada/AGENTS.md")).unwrap();
    let scout = std::fs::read_to_string(dir.path().join("scout/AGENTS.md")).unwrap();
    assert!(agents.contains("## Instructions\n\nBe shorter."));
    assert!(!agents.contains("Be brief."));
    assert!(scout.contains("## Parent prompt\n\nBe shorter."));
    bots.save("ada", &json!({ "prompt": "" }), false)
        .await
        .unwrap();
    let cleared = std::fs::read_to_string(dir.path().join("ada/AGENTS.md")).unwrap();
    let scout = std::fs::read_to_string(dir.path().join("scout/AGENTS.md")).unwrap();
    assert!(!cleared.contains("## Instructions"));
    assert!(!scout.contains("## Parent prompt"));
    let listed = bots.list().await.unwrap();
    assert_eq!(
        listed.iter().find(|b| b["slug"] == "ada").unwrap()["prompt"],
        ""
    );
}

#[tokio::test]
pub(crate) async fn saving_a_main_bot_rewrites_its_children() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    bots.create(&json!({ "name": "Ada", "prompt": "Be brief." }))
        .await
        .unwrap();
    bots.create(&json!({
        "parentSlug": "ada",
        "name": "Scout",
        "subagent": { "inheritParentPrompt": true, "prompt": "Look things up." }
    }))
    .await
    .unwrap();
    bots.create(&json!({
        "parentSlug": "ada",
        "name": "Quiet",
        "subagent": { "inheritParentPrompt": false, "prompt": "Stay quiet." }
    }))
    .await
    .unwrap();
    bots.save("ada", &json!({ "prompt": "Be shorter." }), false)
        .await
        .unwrap();
    let scout = std::fs::read_to_string(dir.path().join("scout/AGENTS.md")).unwrap();
    let quiet = std::fs::read_to_string(dir.path().join("quiet/AGENTS.md")).unwrap();
    assert!(scout.contains("## Parent prompt\n\nBe shorter."));
    assert!(!scout.contains("Be brief."));
    assert!(!quiet.contains("Be shorter."));
    assert!(!quiet.contains("Be brief."));
    assert!(quiet.contains("## Your prompt\n\nStay quiet."));
    assert!(!quiet.contains("## Parent prompt"));
}

#[tokio::test]
pub(crate) async fn a_second_write_instructions_does_not_change_mtime() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots
        .create(&json!({ "name": "Ada", "prompt": "Be brief." }))
        .await
        .unwrap();
    let agents = dir.path().join("ada/AGENTS.md");
    let mut text = std::fs::read_to_string(&agents).unwrap();
    text.push_str("\nMy own notes\n");
    std::fs::write(&agents, &text).unwrap();
    bots.write_instructions(&ada).await.unwrap();
    let kept = std::fs::read_to_string(&agents).unwrap();
    assert!(kept.contains("My own notes"));
    assert!(kept.contains("## Instructions\n\nBe brief."));
    let outside = kept.split(MANAGED_END).nth(1).unwrap_or("");
    assert!(outside.contains("My own notes"));
    let mtime = std::fs::metadata(&agents).unwrap().modified().unwrap();
    bots.write_instructions(&ada).await.unwrap();
    let again = std::fs::metadata(&agents).unwrap().modified().unwrap();
    assert_eq!(mtime, again);
    assert_eq!(std::fs::read_to_string(&agents).unwrap(), kept);
}

#[tokio::test]
pub(crate) async fn a_real_claude_md_file_receives_the_managed_block() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots
        .create(&json!({ "name": "Ada", "prompt": "Be brief." }))
        .await
        .unwrap();
    let claude = dir.path().join("ada/CLAUDE.md");
    std::fs::remove_file(&claude).unwrap();
    std::fs::write(&claude, "Claude notes\n").unwrap();
    bots.write_instructions(&ada).await.unwrap();
    let text = std::fs::read_to_string(&claude).unwrap();
    assert!(text.contains(MANAGED_START) && text.contains(MANAGED_END));
    assert!(text.contains("## Instructions\n\nBe brief."));
    assert!(text.contains("Claude notes"));
    assert!(std::fs::symlink_metadata(&claude)
        .unwrap()
        .file_type()
        .is_file());
    let agents = std::fs::read_to_string(dir.path().join("ada/AGENTS.md")).unwrap();
    assert!(agents.contains("## Instructions\n\nBe brief."));
}

#[tokio::test]
pub(crate) async fn a_prompt_over_16_kib_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let too_long = "x".repeat(crate::bots::agent_context::PROMPT_MAX + 1);
    let err = bots
        .create(&json!({ "name": "Ada", "prompt": too_long }))
        .await
        .unwrap_err();
    assert_eq!(err.status.as_u16(), 400);
    assert_eq!(err.message, "prompt is too long (max 16 KiB)");
    let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
    let err = bots
        .save(
            "ada",
            &json!({ "prompt": "y".repeat(crate::bots::agent_context::PROMPT_MAX + 1) }),
            false,
        )
        .await
        .unwrap_err();
    assert_eq!(err.status.as_u16(), 400);
    assert_eq!(err.message, "prompt is too long (max 16 KiB)");
    assert_eq!(ada["prompt"], "");
    let err = bots
        .create(&json!({
            "parentSlug": "ada",
            "name": "Scout",
            "subagent": { "prompt": "z".repeat(crate::bots::agent_context::PROMPT_MAX + 1) }
        }))
        .await
        .unwrap_err();
    assert_eq!(err.status.as_u16(), 400);
    assert_eq!(err.message, "prompt is too long (max 16 KiB)");
    let exact = "q".repeat(crate::bots::agent_context::PROMPT_MAX);
    assert!(bots
        .save("ada", &json!({ "prompt": exact }), false)
        .await
        .is_ok());
}

#[test]
pub(crate) fn broken_markers_do_not_duplicate_beyond_merge_managed() {
    let block = render_agents_md(
        &json!({ "name": "Ada", "prompt": "Be brief." }),
        std::path::Path::new("/bots"),
        None,
        &[],
        &MemorySnapshot::default(),
    );
    let missing_end = format!("notes before\n{MANAGED_START}\nno end\n");
    let once = merge_managed(&missing_end, &block);
    let twice = merge_managed(&once, &block);
    assert_eq!(twice, once);
    assert_eq!(twice.matches(MANAGED_END).count(), 1);
    assert!(twice.contains("notes before"));
    assert!(twice.contains("## Instructions\n\nBe brief."));

    let missing_start = format!("notes before\n{MANAGED_END}\n");
    let first = merge_managed(&missing_start, &block);
    let second = merge_managed(&first, &block);
    assert_eq!(second, first);
    assert_eq!(second.matches("## Instructions\n\nBe brief.").count(), 1);
    assert!(second.contains("notes before"));

    let reversed = format!("{MANAGED_END}\nnotes between\n{MANAGED_START}\n");
    let once_reversed = merge_managed(&reversed, &block);
    let twice_reversed = merge_managed(&once_reversed, &block);
    assert_eq!(twice_reversed, once_reversed);
    assert_eq!(
        twice_reversed
            .matches("## Instructions\n\nBe brief.")
            .count(),
        1
    );
    assert!(twice_reversed.contains("notes between"));
}

#[tokio::test]
pub(crate) async fn a_memory_edit_refreshes_the_snapshot_and_inheritance_follows_the_flag() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    bots.create(&json!({ "name": "Ada" })).await.unwrap();
    let agents = dir.path().join("ada/AGENTS.md");
    let created = std::fs::read_to_string(&agents).unwrap();
    assert!(!created.contains("<memory "));
    assert!(created.contains("## Child threads"));
    assert!(created.contains(crate::bots::agent_context::CHILD_THREADS));
    std::fs::write(dir.path().join("ada/memory/MEMORY.md"), "Likes tea\n").unwrap();
    bots.refresh_agents("ada").await.unwrap();
    let refreshed = std::fs::read_to_string(&agents).unwrap();
    let bullets = refreshed.find("## Memory").unwrap();
    let tag = refreshed.find("<memory ").unwrap();
    let tools = refreshed.find("## Tools").unwrap();
    assert!(bullets < tag && tag < tools);
    assert!(refreshed.contains("Likes tea"));
    assert!(refreshed.contains("source=\"memory/MEMORY.md\""));
    assert!(refreshed.contains("Do not store secrets in memory. Use the Vault."));

    bots.create(&json!({
        "parentSlug": "ada",
        "name": "Quiet",
        "subagent": { "inheritParentPrompt": false }
    }))
    .await
    .unwrap();
    bots.create(&json!({
        "parentSlug": "ada",
        "name": "Scout",
        "subagent": { "inheritParentPrompt": true }
    }))
    .await
    .unwrap();
    let quiet = std::fs::read_to_string(dir.path().join("quiet/AGENTS.md")).unwrap();
    let scout = std::fs::read_to_string(dir.path().join("scout/AGENTS.md")).unwrap();
    assert!(!quiet.contains("Likes tea"));
    assert!(!quiet.contains("<parent-memory"));
    assert!(scout.contains("Likes tea"));
    assert!(scout.contains("<parent-memory"));
    assert!(scout.contains("read-only=\"true\""));
    std::fs::write(dir.path().join("ada/memory/MEMORY.md"), "Likes coffee\n").unwrap();
    bots.refresh_agents("ada").await.unwrap();
    let scout = std::fs::read_to_string(dir.path().join("scout/AGENTS.md")).unwrap();
    let quiet = std::fs::read_to_string(dir.path().join("quiet/AGENTS.md")).unwrap();
    assert!(scout.contains("Likes coffee"));
    assert!(!scout.contains("Likes tea"));
    assert!(!quiet.contains("Likes coffee"));
}

#[tokio::test]
pub(crate) async fn two_writes_leave_one_managed_block() {
    let dir = tempfile::tempdir().unwrap();
    let bots = std::sync::Arc::new(Bots::new(
        dir.path().to_path_buf(),
        String::new(),
        String::new(),
    ));
    let ada = bots
        .create(&json!({ "name": "Ada", "prompt": "Be brief." }))
        .await
        .unwrap();
    let agents = dir.path().join("ada/AGENTS.md");
    let mut text = std::fs::read_to_string(&agents).unwrap();
    text.push_str("\nMy own notes\n");
    std::fs::write(&agents, &text).unwrap();
    let left = bots.clone();
    let right = bots.clone();
    let ada_left = ada.clone();
    let ada_right = ada.clone();
    let (a, b) = tokio::join!(
        left.write_instructions(&ada_left),
        right.write_instructions(&ada_right),
    );
    a.unwrap();
    b.unwrap();
    let text = std::fs::read_to_string(&agents).unwrap();
    assert_eq!(text.matches(MANAGED_START).count(), 1);
    assert_eq!(text.matches(MANAGED_END).count(), 1);
    assert!(text.contains("My own notes"));
    assert!(text.contains("## Instructions\n\nBe brief."));
}
