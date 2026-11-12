use super::super::{dispatch, Deps};
use crate::bots::memory::{document, MemoryStore};
use crate::storage::adapters::memory::MemoryWorkspace;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

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

fn items(text: &str) -> Vec<Value> {
    vec![json!({
        "type": "user_message",
        "id": "m1",
        "text": text,
        "createdBy": "user"
    })]
}

async fn store() -> MemoryStore {
    let store = MemoryStore::new(Arc::new(MemoryWorkspace::new()));
    store.bootstrap("ada", "Ada").await.unwrap();
    store.bootstrap("ada-jr", "Junior").await.unwrap();
    store
}

#[tokio::test]
async fn a_subagent_writes_only_its_own_store() {
    let store = store().await;
    let parent_before = store
        .read_text(&MemoryStore::bot_memory("ada"))
        .await
        .unwrap();
    let shared_before = store.read_text(MemoryStore::shared_memory()).await.unwrap();
    let roster = vec![bot("ada-jr", "thread-jr", "subagent", Some("ada"), false)];
    let turn = items("Please remember that I prefer dark mode.");
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
    let args = json!({
        "threadId": "thread-jr",
        "op": "add",
        "fact": "Prefers dark mode.",
        "expectedRevision": revision,
        "sourceQuote": "prefer dark mode",
        "slug": "ada",
        "scope": "bot"
    });
    let saved = dispatch(&deps, "memory_edit", &args).await.unwrap();
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

    let shared = json!({
        "threadId": "thread-jr",
        "op": "add",
        "fact": "Shared from a child.",
        "expectedRevision": document(&store, MemoryStore::shared_memory(), 6_000).await.unwrap().revision,
        "sourceQuote": "prefer dark mode",
        "scope": "shared"
    });
    let err = dispatch(&deps, "memory_edit", &shared).await.unwrap_err();
    assert!(err.contains("own memory"), "{err}");
    assert_eq!(
        store.read_text(MemoryStore::shared_memory()).await.unwrap(),
        shared_before
    );

    let escape = json!({
        "threadId": "thread-jr",
        "op": "note",
        "path": "memory/notes/../../escape.md",
        "description": "Escape",
        "content": "No.",
        "expectedRevision": "",
        "sourceQuote": "prefer dark mode"
    });
    assert!(dispatch(&deps, "memory_edit", &escape).await.is_err());
    assert_eq!(
        store
            .read_text(&MemoryStore::bot_memory("ada"))
            .await
            .unwrap(),
        parent_before
    );
}

#[tokio::test]
async fn a_forged_slug_is_ignored_and_a_conflict_does_not_refresh() {
    let store = store().await;
    let roster = vec![bot("ada", "thread-ada", "bot", None, false)];
    let turn = items("I prefer short answers, please.");
    let refreshed = Mutex::new(Vec::new());
    let deps = Deps {
        memory: &store,
        roster: &roster,
        turn_items: &turn,
        refreshed: Some(&refreshed),
        bots_root: None,
    };
    let revision = document(&store, &MemoryStore::bot_memory("ada"), 6_000)
        .await
        .unwrap()
        .revision;
    let args = json!({
        "threadId": "thread-ada",
        "slug": "bea",
        "op": "add",
        "fact": "Prefers short answers.",
        "expectedRevision": revision,
        "sourceQuote": "prefer short answers"
    });
    assert_eq!(
        dispatch(&deps, "memory_edit", &args).await.unwrap()["ok"],
        true
    );
    assert!(store
        .read_text(&MemoryStore::bot_memory("ada"))
        .await
        .unwrap()
        .contains("Prefers short answers."));
    assert!(store
        .read_text(&MemoryStore::bot_memory("bea"))
        .await
        .is_err());
    refreshed.lock().unwrap().clear();
    let stale = json!({
        "threadId": "thread-ada",
        "op": "add",
        "fact": "Another fact entirely.",
        "expectedRevision": revision,
        "sourceQuote": "prefer short answers"
    });
    assert!(dispatch(&deps, "memory_edit", &stale).await.is_err());
    assert!(refreshed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn extended_read_returns_revision_notes_and_a_topic() {
    let store = store().await;
    let roster = vec![bot("ada", "thread-ada", "bot", None, false)];
    let deps = Deps::reading(&store, &roster);
    let revision = document(&store, &MemoryStore::bot_memory("ada"), 6_000)
        .await
        .unwrap()
        .revision;
    let turn = items("Please write a note about the deploy.");
    let writing = Deps {
        memory: &store,
        roster: &roster,
        turn_items: &turn,
        refreshed: None,
        bots_root: None,
    };
    dispatch(
        &writing,
        "memory_edit",
        &json!({
            "threadId": "thread-ada",
            "op": "note",
            "path": "memory/notes/projects/alpha.md",
            "description": "Alpha deploy",
            "content": "# Alpha deploy\n\nRuns on the HP server.",
            "expectedRevision": "",
            "sourceQuote": "write a note about the deploy"
        }),
    )
    .await
    .unwrap();
    let read = dispatch(&deps, "memory_read", &json!({ "threadId": "thread-ada" }))
        .await
        .unwrap();
    assert_eq!(read["memory"]["MEMORY.md"]["text"], "# Ada memory\n\n");
    assert_eq!(read["revision"], revision);
    assert_eq!(read["notes"][0]["path"], "memory/notes/projects/alpha.md");
    assert!(read["shared"].as_str().unwrap().contains("Shared facts"));
    assert!(read.get("sharedRevision").is_some());
    let topic = dispatch(
        &deps,
        "memory_read",
        &json!({ "threadId": "thread-ada", "topic": "memory/notes/projects/alpha.md", "full": true }),
    )
    .await
    .unwrap();
    assert!(topic["content"]
        .as_str()
        .unwrap()
        .starts_with("---\ndescription: Alpha deploy"));
    assert_eq!(topic["full"], true);
    let parent = dispatch(
        &Deps::reading(
            &store,
            &[bot("ada-jr", "thread-jr", "subagent", Some("ada"), true)],
        ),
        "memory_read",
        &json!({ "threadId": "thread-jr" }),
    )
    .await
    .unwrap();
    assert_eq!(
        parent["parent"]["memory"]["MEMORY.md"]["text"],
        "# Ada memory\n\n"
    );
}
