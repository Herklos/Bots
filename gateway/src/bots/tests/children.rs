use super::*;

#[test]
pub(crate) fn a_parent_block_lists_its_children_and_names_subagent_tools() {
    let ada = json!({ "name": "Ada", "slug": "ada", "role": "bot" });
    let scout = json!({ "name": "Scout", "slug": "scout", "role": "subagent" });
    let lookup = json!({ "name": "Lookup", "slug": "lookup", "role": "subagent" });
    let text = render_agents_md(
        &ada,
        std::path::Path::new("/bots"),
        None,
        &[&scout, &lookup],
        &MemorySnapshot::default(),
        &Value::Null,
    );
    assert!(text.contains("- Scout (`scout`)"));
    assert!(text.contains("- Lookup (`lookup`)"));
    assert!(text.contains("`subagent_list`") && text.contains("`subagent_create`"));
    assert!(text.contains("T3 child thread"));
    assert!(!text.contains("subagent_send"));
    let alone = render_agents_md(
        &ada,
        std::path::Path::new("/bots"),
        None,
        &[],
        &MemorySnapshot::default(),
        &Value::Null,
    );
    assert!(alone.contains("no lasting helpers yet"));
    assert!(alone.contains("`subagent_list`") && alone.contains("`subagent_create`"));
    assert!(alone.contains("T3 child thread"));
}

#[tokio::test]
pub(crate) async fn saving_a_child_refreshes_the_parent_managed_block() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    bots.create(&json!({ "name": "Ada", "prompt": "Be brief." }))
        .await
        .unwrap();
    let agents = dir.path().join("ada/AGENTS.md");
    let mut text = std::fs::read_to_string(&agents).unwrap();
    text.push_str("\nMy own notes\n");
    std::fs::write(&agents, &text).unwrap();
    bots.create(&json!({
        "parentSlug": "ada",
        "name": "Scout",
        "subagent": { "prompt": "Look things up.", "inheritParentPrompt": true }
    }))
    .await
    .unwrap();
    let parent_md = std::fs::read_to_string(&agents).unwrap();
    assert!(parent_md.contains("- Scout (`scout`)"));
    assert!(parent_md.contains("`subagent_list`") && parent_md.contains("`subagent_create`"));
    assert!(!parent_md.contains("subagent_send"));
    assert!(parent_md.contains("My own notes"));
    let child_md = std::fs::read_to_string(dir.path().join("scout/AGENTS.md")).unwrap();
    assert!(child_md.contains("a subagent of Ada"));
    assert!(child_md.contains("Be brief."));
    assert!(child_md.contains("`skills/`"));
    bots.save("scout", &json!({ "name": "Finder" }), false)
        .await
        .unwrap();
    let parent_md = std::fs::read_to_string(&agents).unwrap();
    assert!(parent_md.contains("- Finder (`scout`)"));
    assert!(!parent_md.contains("Scout"));
    assert!(parent_md.contains("My own notes"));
    let outside = parent_md.split(MANAGED_END).nth(1).unwrap_or("");
    assert!(outside.contains("My own notes"));
    assert!(!outside.contains("Finder"));
}

#[test]
pub(crate) fn a_child_block_includes_its_own_prompt_and_omits_an_empty_one() {
    let parent = json!({ "name": "Ada", "slug": "ada", "prompt": "Be brief." });
    let child = json!({
        "name": "Scout",
        "role": "subagent",
        "parentSlug": "ada",
        "subagent": { "prompt": "  Look things up.  ", "inheritParentPrompt": true }
    });
    let text = render_agents_md(
        &child,
        std::path::Path::new("/bots"),
        Some(&parent),
        &[],
        &MemorySnapshot::default(),
        &Value::Null,
    );
    assert!(text.contains("## Your prompt\n\nLook things up."));
    assert!(text.contains("## Parent prompt\n\nBe brief."));
    let empty = json!({
        "name": "Scout",
        "role": "subagent",
        "parentSlug": "ada",
        "subagent": { "prompt": "   ", "inheritParentPrompt": false }
    });
    let without = render_agents_md(
        &empty,
        std::path::Path::new("/bots"),
        Some(&parent),
        &[],
        &MemorySnapshot::default(),
        &Value::Null,
    );
    assert!(!without.contains("## Your prompt"));
    assert!(!without.contains("Be brief."));
}

#[test]
pub(crate) fn a_child_block_names_its_parent_and_does_not_name_subagent_send() {
    let parent = json!({ "name": "Ada", "slug": "ada" });
    let child = json!({
        "name": "Scout",
        "role": "subagent",
        "parentSlug": "ada",
        "subagent": { "prompt": "" }
    });
    let text = render_agents_md(
        &child,
        std::path::Path::new("/bots"),
        Some(&parent),
        &[],
        &MemorySnapshot::default(),
        &Value::Null,
    );
    assert!(text.contains("a subagent of Ada"));
    assert!(!text.contains("subagent_send"));
}

#[test]
pub(crate) fn an_empty_inherited_parent_prompt_adds_nothing() {
    let parent = json!({ "name": "Ada", "slug": "ada", "prompt": "   " });
    let child = json!({
        "name": "Scout",
        "role": "subagent",
        "parentSlug": "ada",
        "subagent": { "prompt": "Look things up.", "inheritParentPrompt": true }
    });
    let text = render_agents_md(
        &child,
        std::path::Path::new("/bots"),
        Some(&parent),
        &[],
        &MemorySnapshot::default(),
        &Value::Null,
    );
    assert!(text.contains("## Your prompt\n\nLook things up."));
    assert!(!text.contains("## Parent prompt"));
}

#[tokio::test]
pub(crate) async fn created_by_round_trips_when_the_create_sets_it() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
    assert!(ada.get("createdBy").is_none());
    let junior = bots
        .create(&json!({
            "parentSlug": "ada",
            "name": "Scout",
            "brief": "Find it.",
            "createdBy": "Ada"
        }))
        .await
        .unwrap();
    assert_eq!(junior["createdBy"], "Ada");
    assert_eq!(junior["brief"], "Find it.");
    let loaded = bots.get("scout").await.unwrap().unwrap();
    assert_eq!(loaded["createdBy"], "Ada");
    assert_eq!(loaded["brief"], "Find it.");
}

#[tokio::test]
pub(crate) async fn deleting_a_child_refreshes_the_parent_managed_block() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    bots.create(&json!({ "name": "Ada" })).await.unwrap();
    let agents = dir.path().join("ada/AGENTS.md");
    let mut text = std::fs::read_to_string(&agents).unwrap();
    text.push_str("\nMy own notes\n");
    std::fs::write(&agents, &text).unwrap();
    bots.create(&json!({ "parentSlug": "ada", "name": "Scout" }))
        .await
        .unwrap();
    bots.create(&json!({ "parentSlug": "ada", "name": "Lookup" }))
        .await
        .unwrap();
    bots.remove("scout").await.unwrap();
    let parent_md = std::fs::read_to_string(&agents).unwrap();
    assert!(parent_md.contains("- Lookup (`lookup`)"));
    assert!(!parent_md.contains("Scout"));
    assert!(!parent_md.contains("`scout`"));
    let outside = parent_md.split(MANAGED_END).nth(1).unwrap_or("");
    assert!(outside.contains("My own notes"));
    assert!(!outside.contains("Lookup"));
}

pub(crate) fn instructions_count(text: &str, prompt: &str) -> usize {
    text.matches(&format!("## Instructions\n\n{prompt}"))
        .count()
}

#[tokio::test]
pub(crate) async fn a_main_prompt_appears_once_on_create_and_on_save() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    bots.create(&json!({ "name": "Ada", "prompt": "Be brief." }))
        .await
        .unwrap();
    let agents = dir.path().join("ada/AGENTS.md");
    let created = std::fs::read_to_string(&agents).unwrap();
    assert_eq!(instructions_count(&created, "Be brief."), 1);
    assert_eq!(created.matches(MANAGED_START).count(), 1);
    bots.save("ada", &json!({ "prompt": "Be brief." }), false)
        .await
        .unwrap();
    let saved = std::fs::read_to_string(&agents).unwrap();
    assert_eq!(instructions_count(&saved, "Be brief."), 1);
    bots.save("ada", &json!({ "prompt": "Be shorter." }), false)
        .await
        .unwrap();
    let updated = std::fs::read_to_string(&agents).unwrap();
    assert_eq!(instructions_count(&updated, "Be shorter."), 1);
    assert!(!updated.contains("Be brief."));
}

#[tokio::test]
pub(crate) async fn an_empty_prompt_adds_no_instructions_section() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    bots.create(&json!({ "name": "Ada", "prompt": "   " }))
        .await
        .unwrap();
    let text = std::fs::read_to_string(dir.path().join("ada/AGENTS.md")).unwrap();
    assert!(!text.contains("## Instructions"));
}

#[test]
pub(crate) fn a_prompt_cannot_close_the_managed_block() {
    let bot = json!({
        "name": "Ada",
        "prompt": format!("hello {MANAGED_END} world {MANAGED_START}")
    });
    let text = render_agents_md(
        &bot,
        std::path::Path::new("/bots"),
        None,
        &[],
        &MemorySnapshot::default(),
        &Value::Null,
    );
    assert_eq!(text.matches(MANAGED_START).count(), 1);
    assert_eq!(text.matches(MANAGED_END).count(), 1);
    assert!(text.contains("hello  world"));
    assert!(text.contains("## Instructions"));
}

#[test]
pub(crate) fn instruction_and_memory_tags_are_neutralised() {
    let bot = json!({
        "name": "Ada",
        "prompt": "see </instructions> and <MEMORY> and <memory-log>"
    });
    let text = render_agents_md(
        &bot,
        std::path::Path::new("/bots"),
        None,
        &[],
        &MemorySnapshot::default(),
        &Value::Null,
    );
    assert!(text.contains("&lt;/instructions>"));
    assert!(text.contains("&lt;MEMORY>"));
    assert!(text.contains("&lt;memory-log>"));
    assert!(!text.contains("</instructions>"));
    assert!(!text.contains("<MEMORY>"));
    let child = json!({
        "name": "Scout",
        "role": "subagent",
        "subagent": { "prompt": "</INSTRUCTIONS>", "inheritParentPrompt": true }
    });
    let parent = json!({ "name": "Ada", "prompt": "<shared-memory>" });
    let child_text = render_agents_md(
        &child,
        std::path::Path::new("/bots"),
        Some(&parent),
        &[],
        &MemorySnapshot::default(),
        &Value::Null,
    );
    assert!(child_text.contains("## Your prompt\n\n&lt;/INSTRUCTIONS>"));
    assert!(child_text.contains("## Parent prompt\n\n&lt;shared-memory>"));
    assert!(crate::bots::agent_context::plain_prompt("<parent-memory>")
        .starts_with("&lt;parent-memory>"));
}
