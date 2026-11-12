use super::journal::recover;
use super::state::revision_of;
use super::MemoryStore;
use crate::storage::adapters::memory::MemoryWorkspace;
use std::sync::Arc;

#[tokio::test]
async fn recovery_applies_a_complete_write_and_keeps_a_conflict() {
    let store = MemoryStore::new(Arc::new(MemoryWorkspace::new()));
    store.bootstrap("alpha", "Alpha").await.unwrap();
    let file = MemoryStore::bot_memory("alpha");
    let state_file = MemoryStore::bot_state("alpha");
    let before = store.read_text(&file).await.unwrap();
    let after = format!("{before}- [fact] Recovered fact.\n");
    let journal_dir = MemoryStore::bot_journal_dir("alpha");
    let recoverable = serde_json::json!({
        "version": 1,
        "opId": "recover-me",
        "file": file,
        "beforeHash": revision_of(&before),
        "after": after,
        "stateFile": state_file,
        "afterState": { "version": 1, "revisions": { "MEMORY.md": revision_of(&after) } },
        "createdAt": "2026-11-09T10:00:00.000Z"
    });
    store
        .write_text(
            &format!("{journal_dir}/recover-me.json"),
            &recoverable.to_string(),
            Some(0o600),
        )
        .await
        .unwrap();
    recover(&store, "alpha").await.unwrap();
    assert_eq!(store.read_text(&file).await.unwrap(), after);
    assert!(store.ws.list(&journal_dir).await.unwrap().is_empty());

    let conflict = serde_json::json!({
        "version": 1,
        "opId": "conflict",
        "file": file,
        "beforeHash": "not-the-current-hash",
        "after": "evil",
        "stateFile": state_file,
        "afterState": { "version": 1 },
        "createdAt": "2026-11-09T10:00:00.000Z"
    });
    store
        .write_text(
            &format!("{journal_dir}/conflict.json"),
            &conflict.to_string(),
            Some(0o600),
        )
        .await
        .unwrap();
    recover(&store, "alpha").await.unwrap();
    assert_eq!(store.read_text(&file).await.unwrap(), after);
    assert!(store
        .ws
        .list(&journal_dir)
        .await
        .unwrap()
        .iter()
        .any(|entry| entry.name.starts_with("failed-conflict")));

    let stale_before = "# Alpha memory\n\n- [fact] Something else.\n";
    let stale = serde_json::json!({
        "version": 1,
        "opId": "stale",
        "file": file,
        "beforeHash": revision_of(stale_before),
        "after": "# Alpha memory\n\n- [fact] Brought back.\n",
        "stateFile": state_file,
        "afterState": { "version": 1 },
        "createdAt": "2026-11-09T10:00:00.000Z"
    });
    store
        .write_text(
            &format!("{journal_dir}/stale.json"),
            &stale.to_string(),
            Some(0o600),
        )
        .await
        .unwrap();
    recover(&store, "alpha").await.unwrap();
    store
        .write_text(&file, stale_before, Some(0o644))
        .await
        .unwrap();
    recover(&store, "alpha").await.unwrap();
    recover(&store, "alpha").await.unwrap();
    assert_eq!(store.read_text(&file).await.unwrap(), stale_before);
    let mut names: Vec<_> = store
        .ws
        .list(&journal_dir)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "failed-conflict.json".to_string(),
            "failed-stale.json".to_string()
        ]
    );
}

#[tokio::test]
async fn recovery_can_target_a_note_without_touching_memory() {
    let store = MemoryStore::new(Arc::new(MemoryWorkspace::new()));
    store.bootstrap("alpha", "Alpha").await.unwrap();
    let note = "alpha/memory/notes/projects/alpha.md";
    let before_note = "---\ndescription: Alpha\ntype: note\n---\n# Alpha\n";
    store
        .write_text(note, before_note, Some(0o644))
        .await
        .unwrap();
    let memory = MemoryStore::bot_memory("alpha");
    let before_memory = store.read_text(&memory).await.unwrap();
    let after_note = format!("{before_note}\nRecovered detail.\n");
    let journal_dir = MemoryStore::bot_journal_dir("alpha");
    let body = serde_json::json!({
        "version": 1,
        "opId": "note",
        "file": note,
        "beforeHash": revision_of(before_note),
        "after": after_note,
        "stateFile": MemoryStore::bot_state("alpha"),
        "afterState": { "version": 1, "revisions": { "memory/notes/projects/alpha.md": revision_of(&after_note) } },
        "createdAt": "2026-11-09T10:00:00.000Z"
    });
    store
        .write_text(
            &format!("{journal_dir}/note.json"),
            &body.to_string(),
            Some(0o600),
        )
        .await
        .unwrap();
    recover(&store, "alpha").await.unwrap();
    assert_eq!(store.read_text(note).await.unwrap(), after_note);
    assert_eq!(store.read_text(&memory).await.unwrap(), before_memory);
}
