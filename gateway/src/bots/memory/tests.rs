use super::*;
use crate::bots::Bots;
use crate::storage::adapters::memory::MemoryWorkspace;
use crate::storage::LocalDisk;
use serde_json::json;
use std::sync::Arc;

fn disk(root: &std::path::Path) -> MemoryStore {
    MemoryStore::new(Arc::new(LocalDisk::new(root.to_path_buf())))
}

fn memory_ws() -> MemoryStore {
    MemoryStore::new(Arc::new(MemoryWorkspace::new()))
}

async fn fresh_files(store: &MemoryStore) {
    store.bootstrap("ada", "Ada").await.unwrap();
    assert_eq!(
        store
            .read_text(&MemoryStore::bot_memory("ada"))
            .await
            .unwrap(),
        "# Ada memory\n\n"
    );
    assert_eq!(
        store.read_text(&MemoryStore::bot_log("ada")).await.unwrap(),
        "# Work log\n\n"
    );
    assert_eq!(
        store.read_text(MemoryStore::shared_memory()).await.unwrap(),
        "# Shared facts (all Bots)\n\n"
    );
    assert!(store.require_bot_dir("ada").await.is_ok());
    assert!(store.exists("ada/library").await.unwrap());
    assert!(store.exists("ada/skills").await.unwrap());
    store
        .write_text(MemoryStore::shared_memory(), "kept by Ada\n", None)
        .await
        .unwrap();
    store.bootstrap("bea", "Bea").await.unwrap();
    assert_eq!(
        store.read_text(MemoryStore::shared_memory()).await.unwrap(),
        "kept by Ada\n"
    );
    assert_eq!(
        store
            .read_text_or_empty("bea/memory/missing.md")
            .await
            .unwrap(),
        ""
    );
}

#[tokio::test]
async fn bootstrap_on_disk_and_in_memory_keeps_an_existing_shared_file() {
    let dir = tempfile::tempdir().unwrap();
    fresh_files(&disk(dir.path())).await;
    fresh_files(&memory_ws()).await;
}

#[tokio::test]
async fn path_keys_stay_under_the_bot_and_the_shared_folder() {
    assert_eq!(MemoryStore::bot_state("ada"), "ada/memory/.state.json");
    assert_eq!(MemoryStore::bot_dreams("ada"), "ada/memory/DREAMS.md");
    assert_eq!(MemoryStore::bot_history_dir("ada"), "ada/memory/history");
    assert_eq!(MemoryStore::bot_journal_dir("ada"), "ada/memory/.journal");
    assert_eq!(
        MemoryStore::bot_note("ada", "note.md").unwrap(),
        "ada/memory/notes/note.md"
    );
    assert!(MemoryStore::bot_note("ada", "../secret").is_err());
    assert_eq!(
        MemoryStore::shared_state(),
        ".botapp/shared-memory.state.json"
    );
    assert_eq!(
        MemoryStore::shared_history_dir(),
        ".botapp/shared-memory.history"
    );
    assert_eq!(
        MemoryStore::shared_journal_dir(),
        ".botapp/shared-memory.journal"
    );
    assert_eq!(
        MemoryStore::shared_retired("2026-11"),
        ".botapp/shared-history/closed-2026-11.md"
    );
}

#[tokio::test]
async fn a_file_where_the_bot_folder_should_be_is_absent() {
    let store = memory_ws();
    store.ws.write("ada", b"not a folder", None).await.unwrap();
    assert!(matches!(
        store.require_bot_dir("ada").await,
        Err(MemoryScopeError::Absent)
    ));
    assert!(store.ensure_bot_dir("ada").await.is_err());
    assert!(matches!(
        store.require_bot_dir("../outside").await,
        Err(MemoryScopeError::Absent)
    ));
}

#[tokio::test]
async fn a_symlink_bot_folder_is_absent_and_a_symlinked_botapp_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("bea/memory")).unwrap();
    std::fs::write(root.join("bea/memory/MEMORY.md"), "bea-secret").unwrap();
    std::os::unix::fs::symlink(root.join("bea"), root.join("ada")).unwrap();
    let store = disk(root);
    assert!(matches!(
        store.require_bot_dir("ada").await,
        Err(MemoryScopeError::Absent)
    ));
    let (text, truncated) = store
        .read_capped("ada/memory/MEMORY.md", 100)
        .await
        .unwrap();
    assert_eq!(text, "");
    assert!(!truncated);
    assert!(!text.contains("bea-secret"));
    assert!(store.ensure_bot_dir("ada").await.is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("bea/memory/MEMORY.md")).unwrap(),
        "bea-secret"
    );

    let outside = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join(".botapp")).unwrap();
    let err = disk(root.path()).bootstrap("ada", "Ada").await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(!outside.path().join("shared-memory.md").exists());
}

#[tokio::test]
async fn an_intermediate_symlink_reads_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("bea/memory")).unwrap();
    std::fs::write(root.join("bea/memory/MEMORY.md"), "bea-secret").unwrap();
    std::fs::create_dir_all(root.join("ada")).unwrap();
    std::os::unix::fs::symlink(root.join("bea/memory"), root.join("ada/memory")).unwrap();
    let (text, _) = disk(root)
        .read_capped("ada/memory/MEMORY.md", 100)
        .await
        .unwrap();
    assert_eq!(text, "");
    assert!(!text.contains("bea-secret"));
}

#[tokio::test]
async fn creating_a_second_bot_leaves_the_shared_file() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
    let slug = ada["slug"].as_str().unwrap();
    let memory = dir.path().join(slug).join("memory/MEMORY.md");
    assert_eq!(
        std::fs::read_to_string(&memory).unwrap(),
        "# Ada memory\n\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(slug).join("memory/log.md")).unwrap(),
        "# Work log\n\n"
    );
    let shared = dir.path().join(".botapp/shared-memory.md");
    std::fs::write(&shared, "Bot A seed\n").unwrap();
    bots.create(&json!({ "name": "Bea" })).await.unwrap();
    assert_eq!(std::fs::read_to_string(shared).unwrap(), "Bot A seed\n");
}
