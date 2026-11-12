use super::super::{dispatch, Deps};
use crate::bots::memory::{document, MemoryStore};
use crate::storage::adapters::memory::MemoryWorkspace;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn bot(slug: &str, thread: &str, role: &str, parent: Option<&str>) -> Value {
    json!({
        "slug": slug,
        "name": slug,
        "mainThreadId": thread,
        "role": role,
        "parentSlug": parent,
    })
}

fn owner(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "type": "user_message",
        "text": text,
        "createdBy": "user",
        "creationSource": "web",
    })
}

async fn store() -> MemoryStore {
    let store = MemoryStore::new(Arc::new(MemoryWorkspace::new()));
    store.bootstrap("ada", "Ada").await.unwrap();
    store.bootstrap("ada-jr", "Junior").await.unwrap();
    store
}

#[tokio::test]
async fn a_subagent_reflects_only_its_own_memory() {
    let store = store().await;
    let parent_before = store
        .read_text(&MemoryStore::bot_memory("ada"))
        .await
        .unwrap();
    let roster = vec![bot("ada-jr", "thread-jr", "subagent", Some("ada"))];
    let turn = vec![owner(
        "m1",
        "Please remember that I prefer dark mode at night.",
    )];
    let refreshed = Mutex::new(Vec::new());
    let deps = Deps {
        memory: &store,
        roster: &roster,
        turn_items: &turn,
        refreshed: Some(&refreshed),
        bots_root: None,
    };
    let revision = document(&store, &MemoryStore::bot_memory("ada-jr"), 6_000)
        .await
        .unwrap()
        .revision;
    let saved = dispatch(
        &deps,
        "memory_reflect_apply",
        &json!({
            "threadId": "thread-jr",
            "expectedRevision": revision,
            "throughPosition": 0,
            "lastMessageId": "m1",
            "slug": "ada",
            "operations": [{
                "op": "add",
                "fact": "Prefers dark mode.",
                "sourceMessageId": "m1",
                "sourceQuote": "prefer dark mode at night"
            }]
        }),
    )
    .await
    .unwrap();
    assert_eq!(saved["ok"], true);
    assert!(store
        .read_text(&MemoryStore::bot_memory("ada-jr"))
        .await
        .unwrap()
        .contains("Prefers dark mode."));
    assert_eq!(
        store
            .read_text(&MemoryStore::bot_memory("ada"))
            .await
            .unwrap(),
        parent_before
    );
    assert_eq!(refreshed.lock().unwrap().as_slice(), ["ada-jr"]);
}

#[tokio::test]
async fn an_edit_cannot_use_a_message_already_reflected() {
    let store = store().await;
    let roster = vec![bot("ada", "thread-ada", "bot", None)];
    let turn = vec![
        owner("m0", "Please remember that I prefer dark mode at night."),
        owner(
            "m1",
            "Please remember that I prefer tea in the morning now.",
        ),
    ];
    let deps = Deps {
        memory: &store,
        roster: &roster,
        turn_items: &turn,
        refreshed: None,
        bots_root: None,
    };
    let revision = document(&store, &MemoryStore::bot_memory("ada"), 6_000)
        .await
        .unwrap()
        .revision;
    dispatch(
        &deps,
        "memory_reflect_apply",
        &json!({
            "threadId": "thread-ada",
            "expectedRevision": revision,
            "throughPosition": 0,
            "lastMessageId": "m0",
            "operations": []
        }),
    )
    .await
    .unwrap();
    let err = dispatch(
        &deps,
        "memory_edit",
        &json!({
            "threadId": "thread-ada",
            "op": "add",
            "fact": "Prefers dark mode.",
            "expectedRevision": revision,
            "sourceMessageId": "m0",
            "sourceQuote": "prefer dark mode at night"
        }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("already reflected"));
    let fresh = document(&store, &MemoryStore::bot_memory("ada"), 6_000)
        .await
        .unwrap()
        .revision;
    let saved = dispatch(
        &deps,
        "memory_edit",
        &json!({
            "threadId": "thread-ada",
            "op": "add",
            "fact": "Prefers tea in the morning.",
            "expectedRevision": fresh,
            "sourceMessageId": "m1",
            "sourceQuote": "prefer tea in the morning"
        }),
    )
    .await
    .unwrap();
    assert_eq!(saved["ok"], true);
}
