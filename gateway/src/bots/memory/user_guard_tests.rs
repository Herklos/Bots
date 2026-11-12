//! A Bot cannot replace a person-owned fact, and a direct edit of that file is restored.
use super::state::revision_of;
use super::user::{write, NewFact};
use super::{mutate, verify_user_facts, MemoryStore, MutateFail, Mutation, Op};
use crate::bots::agent_context::{read_snapshot, render_snapshot};
use crate::storage::adapters::memory::MemoryWorkspace;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;

const NOW: &str = "2026-11-09T10:00:00.000Z";
const OWNED: &str = "That memory is yours to change. Ask the person to edit it in Memory.";
const RESTORED: &str = "Restored 1 memory you wrote";

fn fact(text: &str) -> NewFact {
    NewFact {
        text: text.to_string(),
        at: NOW.to_string(),
    }
}

fn store() -> MemoryStore {
    MemoryStore::new(Arc::new(MemoryWorkspace::new()))
}

async fn ready() -> MemoryStore {
    let store = store();
    store.bootstrap("alpha", "Alpha").await.unwrap();
    store
}

async fn bot_revision_text(store: &MemoryStore) -> String {
    super::search::document(store, &MemoryStore::bot_memory("alpha"), 6_000)
        .await
        .unwrap()
        .revision
}

fn notices(root: &Path) -> Vec<Value> {
    let path = root.join(".botapp/memory-user/alpha.state.json");
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    let value: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    value["notices"].as_array().cloned().unwrap_or_default()
}

fn code_of(err: MutateFail) -> (String, String) {
    match err {
        MutateFail::Code(coded) => (coded.code.to_string(), coded.message),
        MutateFail::Io(err) => ("io".to_string(), err.to_string()),
    }
}

#[tokio::test]
async fn a_bot_cannot_replace_or_forget_a_user_fact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "alpha", &[fact("likes tea")], NOW)
        .await
        .unwrap();
    let store = ready().await;
    let revision = bot_revision_text(&store).await;
    let mut replace = Mutation::new("alpha", Op::Replace, &revision);
    replace.now = NOW;
    replace.user_root = Some(root);
    replace.bot_name = "Ada";
    replace.old = Some("likes tea");
    replace.new_fact = Some("likes coffee");
    let (code, message) = code_of(mutate(&store, replace).await.unwrap_err());
    assert_eq!(code, "user_owned");
    assert_eq!(message, OWNED);
    assert_eq!(notices(root).len(), 1);
    assert_eq!(
        notices(root)[0]["text"],
        "Ada tried to change a memory you wrote"
    );
    assert_eq!(notices(root)[0]["kind"], "rejected");
    assert_eq!(
        std::fs::read_to_string(root.join(".botapp/memory-user/alpha.md")).unwrap(),
        "likes tea\n"
    );
    assert!(!store
        .read_text(&MemoryStore::bot_memory("alpha"))
        .await
        .unwrap()
        .contains("likes coffee"));

    let mut again = Mutation::new("alpha", Op::Forget, &revision);
    again.now = NOW;
    again.user_root = Some(root);
    again.bot_name = "Ada";
    again.fact = Some("likes tea");
    let (code, message) = code_of(mutate(&store, again).await.unwrap_err());
    assert_eq!(code, "user_owned");
    assert_eq!(message, OWNED);
    assert_eq!(notices(root).len(), 1);

    let mut tomorrow = Mutation::new("alpha", Op::Forget, &revision);
    tomorrow.now = "2026-11-10T10:00:00.000Z";
    tomorrow.user_root = Some(root);
    tomorrow.bot_name = "Ada";
    tomorrow.fact = Some("- [preference] likes tea [source: thread/m1; added: 2026-11-09]");
    assert_eq!(
        code_of(mutate(&store, tomorrow).await.unwrap_err()).0,
        "user_owned"
    );
    assert_eq!(notices(root).len(), 2);
}

#[tokio::test]
async fn an_overlapping_add_is_rejected_and_shows_the_fact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let existing = "alpha beta gamma delta";
    write(root, "alpha", &[fact(existing)], NOW).await.unwrap();
    let store = ready().await;
    let revision = bot_revision_text(&store).await;
    let mut added = Mutation::new("alpha", Op::Add, &revision);
    added.now = NOW;
    added.user_root = Some(root);
    added.bot_name = "Ada";
    added.fact = Some("alpha beta gamma delta epsilon");
    let (code, message) = code_of(mutate(&store, added).await.unwrap_err());
    assert_eq!(code, "overlap");
    assert!(message.contains(existing));
    assert!(message.contains("Ask the person to edit it in Memory."));
    assert_eq!(notices(root).len(), 1);

    let mut fresh = Mutation::new("alpha", Op::Add, &revision);
    fresh.now = NOW;
    fresh.user_root = Some(root);
    fresh.bot_name = "Ada";
    fresh.fact = Some("The office is in Oslo.");
    let saved = mutate(&store, fresh).await.unwrap();
    assert!(saved.line.unwrap().contains("The office is in Oslo."));
    assert_eq!(notices(root).len(), 1);
}

#[tokio::test]
async fn a_direct_edit_is_restored_and_the_restore_writes_one_notice() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "alpha", &[fact("likes tea")], NOW)
        .await
        .unwrap();
    let file = root.join(".botapp/memory-user/alpha.md");
    std::fs::write(&file, "hacked\n").unwrap();
    assert!(verify_user_facts(root, "alpha", NOW).await.unwrap());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "likes tea\n");
    assert_eq!(notices(root).len(), 1);
    assert_eq!(notices(root)[0]["text"], RESTORED);
    assert_eq!(notices(root)[0]["kind"], "restored");
    let state: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(".botapp/memory-user/alpha.state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["revision"], revision_of("likes tea\n"));
    assert!(!verify_user_facts(root, "alpha", NOW).await.unwrap());
    assert_eq!(notices(root).len(), 1);
    let journal = root.join(".botapp/memory-user/alpha.journal");
    let pending: Vec<_> = std::fs::read_dir(&journal)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| {
            std::path::Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                && !name.starts_with("failed-")
        })
        .collect();
    assert!(pending.is_empty());

    std::fs::write(&file, "hacked again\n").unwrap();
    let bot = serde_json::json!({ "name": "Ada", "slug": "alpha", "role": "bot" });
    let rendered = render_snapshot(&read_snapshot(root, &bot, None).await);
    assert!(rendered.contains("likes tea"));
    assert!(!rendered.contains("hacked"));
    assert_eq!(notices(root).len(), 2);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "likes tea\n");
}
