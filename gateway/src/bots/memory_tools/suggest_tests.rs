//! A routine turn cannot edit memory, and it can propose a fact.
use super::super::{dispatch, Deps};
use crate::bots::memory::MemoryStore;
use crate::storage::adapters::memory::MemoryWorkspace;
use serde_json::{json, Value};
use std::sync::Arc;

fn bot(slug: &str, thread: &str) -> Value {
    json!({
        "slug": slug,
        "name": "Ada",
        "mainThreadId": thread,
        "role": "bot",
    })
}

fn routine_turn() -> Vec<Value> {
    vec![json!({
        "type": "user_message",
        "id": "routine-1",
        "text": "Remember that I take tea at four.",
        "scheduledTaskId": "task-1",
        "createdBy": "user"
    })]
}

#[tokio::test]
async fn a_routine_turn_cannot_edit_but_can_suggest() {
    let store = MemoryStore::new(Arc::new(MemoryWorkspace::new()));
    store.bootstrap("ada", "Ada").await.unwrap();
    let before = store
        .read_text(&MemoryStore::bot_memory("ada"))
        .await
        .unwrap();
    let roster = vec![bot("ada", "thread-ada")];
    let turn = routine_turn();
    let deps = Deps {
        memory: &store,
        roster: &roster,
        turn_items: &turn,
        refreshed: None,
        bots_root: None,
    };
    let revision = crate::bots::memory::document(&store, &MemoryStore::bot_memory("ada"), 6_000)
        .await
        .unwrap()
        .revision;
    let edit = json!({
        "threadId": "thread-ada",
        "op": "add",
        "fact": "Takes tea at four.",
        "expectedRevision": revision,
        "sourceQuote": "take tea at four"
    });
    let err = dispatch(&deps, "memory_edit", &edit).await.unwrap_err();
    assert!(err.contains("memory_suggest"), "{err}");
    assert_eq!(
        store
            .read_text(&MemoryStore::bot_memory("ada"))
            .await
            .unwrap(),
        before
    );

    let suggested = dispatch(
        &deps,
        "memory_suggest",
        &json!({
            "threadId": "thread-ada",
            "text": "Takes tea at four.",
            "why": "The afternoon routine mentioned tea."
        }),
    )
    .await
    .unwrap();
    assert_eq!(suggested["ok"], true);
    assert!(suggested["id"].as_str().unwrap().starts_with("s_"));
    assert_eq!(suggested["pending"], 1);
    assert_eq!(
        store
            .read_text(&MemoryStore::bot_memory("ada"))
            .await
            .unwrap(),
        before
    );
    let queued = store
        .read_text(".botapp/memory-suggestions/ada.json")
        .await
        .unwrap();
    assert!(queued.contains("Takes tea at four."));
    assert!(!queued.contains("MEMORY.md"));
}
