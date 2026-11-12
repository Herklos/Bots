use super::*;

fn bot(slug: &str, thread: &str, role: &str, parent: Option<&str>, hide: bool) -> Value {
    json!({
        "slug": slug,
        "name": slug,
        "mainThreadId": thread,
        "role": role,
        "parentSlug": parent,
        "hideFromSidebar": hide,
    })
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, text).unwrap();
}

#[test]
fn names_are_served_here() {
    assert!(is_memory_tool("memory_read"));
    assert!(is_memory_tool("skills_read"));
    assert!(!is_memory_tool("vault_list"));
    let defs = tool_defs();
    let names: Vec<&str> = defs.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for name in NAMES {
        assert!(names.contains(&name), "{name} is listed");
    }
    let described = tool_defs()
        .iter()
        .all(|t| t["description"].as_str().unwrap().contains("16384"));
    assert!(described);
    assert!(skill_file_name("Open.MD"));
    assert!(!skill_file_name(".md"));
    assert!(!skill_file_name("notes.txt"));
}

#[tokio::test]
async fn a_bot_reads_its_own_memory_and_skills() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        &root.join("ada/memory/MEMORY.md"),
        "ada remembers the gate code",
    );
    write(&root.join("ada/skills/open.md"), "turn the key");
    let roster = vec![bot("ada", "thread-ada", "bot", None, false)];
    let deps = Deps {
        root,
        roster: &roster,
    };
    let args = json!({ "threadId": "thread-ada" });
    let memory = dispatch(&deps, "memory_read", &args).await.unwrap();
    assert_eq!(
        memory["memory"]["MEMORY.md"]["text"],
        "ada remembers the gate code"
    );
    assert_eq!(memory["memory"]["log.md"]["text"], "");
    assert_eq!(memory["memory"]["log.md"]["truncated"], false);
    assert!(memory.get("parent").is_none());
    let skills = dispatch(&deps, "skills_read", &args).await.unwrap();
    assert_eq!(skills["skills"][0]["name"], "open.md");
    assert_eq!(skills["skills"][0]["text"], "turn the key");
    assert!(skills.get("parent").is_none());
}

#[tokio::test]
async fn a_subagent_reads_its_parents_memory_and_skills() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("ada/memory/MEMORY.md"), "parent fact");
    write(&root.join("ada/memory/log.md"), "parent log");
    write(&root.join("ada/skills/shared.md"), "parent skill");
    write(&root.join("ada-jr/memory/MEMORY.md"), "child fact");
    write(&root.join("ada-jr/skills/own.md"), "child skill");
    let roster = vec![
        bot("ada", "thread-ada", "bot", None, false),
        bot("ada-jr", "thread-jr", "subagent", Some("ada"), true),
    ];
    let deps = Deps {
        root,
        roster: &roster,
    };
    let args = json!({ "threadId": "thread-jr", "slug": "ada", "parentSlug": "nope" });
    let memory = dispatch(&deps, "memory_read", &args).await.unwrap();
    assert_eq!(memory["memory"]["MEMORY.md"]["text"], "child fact");
    assert_eq!(
        memory["parent"]["memory"]["MEMORY.md"]["text"],
        "parent fact"
    );
    assert_eq!(memory["parent"]["memory"]["log.md"]["text"], "parent log");
    let skills = dispatch(&deps, "skills_read", &args).await.unwrap();
    assert_eq!(skills["skills"][0]["text"], "child skill");
    assert_eq!(skills["parent"]["skills"][0]["name"], "shared.md");
    assert_eq!(skills["parent"]["skills"][0]["text"], "parent skill");
}

#[tokio::test]
async fn hide_from_sidebar_also_reads_the_parent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("ada/memory/log.md"), "from the parent");
    write(&root.join("ada-jr/memory/log.md"), "from the child");
    let roster = vec![bot("ada-jr", "thread-jr", "bot", Some("ada"), true)];
    let deps = Deps {
        root,
        roster: &roster,
    };
    let memory = dispatch(&deps, "memory_read", &json!({ "threadId": "thread-jr" }))
        .await
        .unwrap();
    assert_eq!(
        memory["parent"]["memory"]["log.md"]["text"],
        "from the parent"
    );
}

#[tokio::test]
async fn a_normal_bot_cannot_read_another_bots_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("ada/memory/MEMORY.md"), "ada-secret");
    write(&root.join("ada/skills/ada.md"), "ada-skill");
    write(&root.join("bea/memory/MEMORY.md"), "bea-secret");
    write(&root.join("bea/skills/bea.md"), "bea-skill");
    let roster = vec![
        bot("ada", "thread-ada", "bot", Some("bea"), false),
        bot("bea", "thread-bea", "bot", None, false),
    ];
    let deps = Deps {
        root,
        roster: &roster,
    };
    let memory = dispatch(
        &deps,
        "memory_read",
        &json!({ "threadId": "thread-ada", "slug": "bea", "path": "../bea/memory/MEMORY.md" }),
    )
    .await
    .unwrap();
    let dumped = memory.to_string();
    assert!(dumped.contains("ada-secret"));
    assert!(!dumped.contains("bea-secret"));
    assert!(memory.get("parent").is_none());
    let skills = dispatch(
        &deps,
        "skills_read",
        &json!({ "threadId": "thread-ada", "slug": "bea" }),
    )
    .await
    .unwrap();
    let dumped = skills.to_string();
    assert!(dumped.contains("ada-skill"));
    assert!(!dumped.contains("bea-skill"));
}

#[tokio::test]
async fn a_forged_slug_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("ada/memory/MEMORY.md"), "only-ada");
    write(&root.join("bea/memory/MEMORY.md"), "only-bea");
    let roster = vec![bot("ada", "thread-ada", "bot", None, false)];
    let deps = Deps {
        root,
        roster: &roster,
    };
    let memory = dispatch(
        &deps,
        "memory_read",
        &json!({
            "threadId": "thread-ada",
            "slug": "bea",
            "parentSlug": "bea",
            "path": "bea/memory/MEMORY.md",
        }),
    )
    .await
    .unwrap();
    assert_eq!(memory["memory"]["MEMORY.md"]["text"], "only-ada");
    assert!(!memory.to_string().contains("only-bea"));
}

#[tokio::test]
async fn a_missing_parent_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("ada-jr/memory/MEMORY.md"), "child");
    let roster = vec![bot(
        "ada-jr",
        "thread-jr",
        "subagent",
        Some("missing"),
        true,
    )];
    let deps = Deps {
        root,
        roster: &roster,
    };
    let err = dispatch(&deps, "memory_read", &json!({ "threadId": "thread-jr" }))
        .await
        .unwrap_err();
    assert_eq!(err, PARENT_MISSING);
    let forged = dispatch(
        &deps,
        "skills_read",
        &json!({ "threadId": "thread-jr", "parentSlug": "ada" }),
    )
    .await
    .unwrap_err();
    assert_eq!(forged, PARENT_MISSING);
}

#[tokio::test]
async fn a_parent_slug_that_escapes_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("ada-jr/memory/MEMORY.md"), "child");
    write(&dir.path().join("../outside.md"), "outside");
    let roster = vec![bot(
        "ada-jr",
        "thread-jr",
        "subagent",
        Some("../outside"),
        false,
    )];
    let deps = Deps {
        root,
        roster: &roster,
    };
    let err = dispatch(&deps, "memory_read", &json!({ "threadId": "thread-jr" }))
        .await
        .unwrap_err();
    assert_eq!(err, PARENT_MISSING);
    assert!(!err.contains("outside"));
}

#[tokio::test]
async fn files_are_capped() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let huge = "x".repeat(FILE_CAP + 40);
    write(&root.join("ada/memory/MEMORY.md"), &huge);
    write(&root.join("ada/skills/big.md"), &huge);
    let roster = vec![bot("ada", "thread-ada", "bot", None, false)];
    let deps = Deps {
        root,
        roster: &roster,
    };
    let args = json!({ "threadId": "thread-ada" });
    let memory = dispatch(&deps, "memory_read", &args).await.unwrap();
    let text = memory["memory"]["MEMORY.md"]["text"].as_str().unwrap();
    assert_eq!(text.len(), FILE_CAP);
    assert_eq!(memory["memory"]["MEMORY.md"]["truncated"], true);
    assert_ne!(text, huge);
    let skills = dispatch(&deps, "skills_read", &args).await.unwrap();
    assert_eq!(
        skills["skills"][0]["text"].as_str().unwrap().len(),
        FILE_CAP
    );
    assert_eq!(skills["skills"][0]["truncated"], true);
}

#[tokio::test]
async fn a_symlink_is_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("bea/memory/MEMORY.md"), "bea-secret");
    write(&root.join("bea/skills/hidden.md"), "bea-skill");
    std::fs::create_dir_all(root.join("ada/memory")).unwrap();
    std::fs::create_dir_all(root.join("ada/skills")).unwrap();
    std::os::unix::fs::symlink(
        root.join("bea/memory/MEMORY.md"),
        root.join("ada/memory/MEMORY.md"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        root.join("bea/skills/hidden.md"),
        root.join("ada/skills/link.md"),
    )
    .unwrap();
    write(&root.join("ada/skills/real.md"), "ada-skill");
    let roster = vec![bot("ada", "thread-ada", "bot", None, false)];
    let deps = Deps {
        root,
        roster: &roster,
    };
    let args = json!({ "threadId": "thread-ada" });
    let memory = dispatch(&deps, "memory_read", &args).await.unwrap();
    assert_eq!(memory["memory"]["MEMORY.md"]["text"], "");
    assert!(!memory.to_string().contains("bea-secret"));
    let skills = dispatch(&deps, "skills_read", &args).await.unwrap();
    assert_eq!(skills["skills"].as_array().unwrap().len(), 1);
    assert_eq!(skills["skills"][0]["name"], "real.md");
    assert!(!skills.to_string().contains("bea-skill"));
}
