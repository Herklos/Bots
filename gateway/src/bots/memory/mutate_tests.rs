use super::facts::hash_fact;
use super::search::{self, read_view};
use super::state::{self, revision_of};
use super::{mutate, MemoryStore, MutateFail, Mutation, Op, Scope};
use crate::storage::adapters::memory::MemoryWorkspace;
use std::sync::Arc;

const NOW: &str = "2026-11-09T10:00:00.000Z";

fn workspace() -> MemoryStore {
    MemoryStore::new(Arc::new(MemoryWorkspace::new()))
}

async fn alpha() -> MemoryStore {
    let store = workspace();
    store.bootstrap("alpha", "Alpha").await.unwrap();
    store
}

fn base<'a>(slug: &'a str, op: Op, revision: &'a str) -> Mutation<'a> {
    let mut mutation = Mutation::new(slug, op, revision);
    mutation.now = NOW;
    mutation
}

async fn bot_revision(store: &MemoryStore, slug: &str) -> String {
    search::document(store, &MemoryStore::bot_memory(slug), 6_000)
        .await
        .unwrap()
        .revision
}

async fn shared_revision(store: &MemoryStore) -> String {
    search::document(store, MemoryStore::shared_memory(), 6_000)
        .await
        .unwrap()
        .revision
}

fn code(err: MutateFail) -> String {
    match err {
        MutateFail::Code(coded) => coded.code.to_string(),
        MutateFail::Io(_) => "io".to_string(),
    }
}

#[tokio::test]
async fn add_duplicate_conflict_provenance_and_history() {
    let store = alpha().await;
    let before = bot_revision(&store, "alpha").await;
    let mut added = base("alpha", Op::Add, &before);
    added.fact = Some("Prefers short answers.");
    added.category = Some("preference");
    added.source_message_id = "m42";
    let result = mutate(&store, added).await.unwrap();
    let line = result.line.unwrap();
    assert!(line.contains("[preference] Prefers short answers."));
    assert!(line.contains("[source: thread/m42; added: 2026-11-09]"));
    let text = store
        .read_text(&MemoryStore::bot_memory("alpha"))
        .await
        .unwrap();
    assert!(text.contains("Prefers short answers."));
    let state = state::read_state(&store, &MemoryStore::bot_state("alpha"))
        .await
        .unwrap();
    assert_eq!(
        state.provenance[&hash_fact("Prefers short answers.")]
            .source_message_id
            .as_deref(),
        Some("m42")
    );
    assert_eq!(
        store
            .ws
            .list(&MemoryStore::bot_history_dir("alpha"))
            .await
            .unwrap()
            .len(),
        1
    );

    let mut duplicate = base("alpha", Op::Add, &result.revision);
    duplicate.fact = Some("Prefers short answers.");
    duplicate.source_message_id = "m43";
    assert_eq!(
        code(mutate(&store, duplicate).await.unwrap_err()),
        "duplicate"
    );

    let mut conflict = base("alpha", Op::Add, &before);
    conflict.fact = Some("Another fact.");
    conflict.source_message_id = "m44";
    assert_eq!(
        code(mutate(&store, conflict).await.unwrap_err()),
        "conflict"
    );
}

#[tokio::test]
async fn two_writers_cannot_both_win() {
    let store = alpha().await;
    let expected = shared_revision(&store).await;
    let add = |author: &'static str, fact: &'static str| {
        let mut mutation = base(author, Op::Add, &expected);
        mutation.scope = Scope::Shared;
        mutation.fact = Some(fact);
        mutate(&store, mutation)
    };
    let (left, right) = tokio::join!(
        add("alpha", "Timezone is Bogota."),
        add("beta", "The manager is Ana.")
    );
    let wins = [&left, &right]
        .iter()
        .filter(|result| result.is_ok())
        .count();
    assert_eq!(wins, 1);
    let failed = if left.is_err() { left } else { right };
    let MutateFail::Code(conflict) = failed.unwrap_err() else {
        panic!("expected a conflict");
    };
    assert_eq!(conflict.code, "conflict");
    let revision = conflict.revision.unwrap();
    let mut retry = base("beta", Op::Add, &revision);
    retry.scope = Scope::Shared;
    retry.fact = Some("The manager is Ana.");
    assert!(mutate(&store, retry).await.is_ok());
    let text = store.read_text(MemoryStore::shared_memory()).await.unwrap();
    assert!(text.contains("Timezone is Bogota."));
    assert!(text.contains("The manager is Ana."));

    let state = MemoryStore::shared_state();
    let revision = shared_revision(&store).await;
    store.ws.remove(state).await.unwrap();
    store.ws.ensure_dir(state).await.unwrap();
    let mut blocked = base("alpha", Op::Add, &revision);
    blocked.scope = Scope::Shared;
    blocked.fact = Some("This write fails.");
    assert_eq!(code(mutate(&store, blocked).await.unwrap_err()), "io");
    store.ws.remove(state).await.unwrap();
    let revision = shared_revision(&store).await;
    let mut landed = base("alpha", Op::Add, &revision);
    landed.scope = Scope::Shared;
    landed.fact = Some("This write lands.");
    assert!(mutate(&store, landed).await.is_ok());
}

#[tokio::test]
async fn pasted_words_are_refused() {
    let store = alpha().await;
    let before = bot_revision(&store, "alpha").await;
    let mut mutation = base("alpha", Op::Add, &before);
    mutation.source_quote = "the launch is Thursday";
    mutation.source_text = "> The launch is Thursday\n\nPlease remember this.";
    mutation.fact = Some("The launch is Thursday.");
    let MutateFail::Code(err) = mutate(&store, mutation).await.unwrap_err() else {
        panic!("expected a coded refusal");
    };
    let message = err.message.to_lowercase();
    assert!(
        message.contains("pasted")
            || message.contains("quoted")
            || message.contains("code block")
            || message.contains("forwarded"),
        "{}",
        err.message
    );
}

#[tokio::test]
async fn supersede_closes_the_old_range() {
    let store = alpha().await;
    let before = bot_revision(&store, "alpha").await;
    let mut added = base("alpha", Op::Add, &before);
    added.fact = Some("Use OpenCode for merges.");
    added.category = Some("decision");
    added.valid_from = Some("2026-09-01");
    let added = mutate(&store, added).await.unwrap();
    let mut next = base("alpha", Op::Supersede, &added.revision);
    next.source_quote = "switch to T3 for merges";
    next.source_text = "Please switch to T3 for merges from now on.";
    next.source_message_id = "m2";
    next.old = added.line.as_deref();
    next.new_fact = Some("Use T3 for merges.");
    next.valid_from = Some("2026-10-14");
    assert!(mutate(&store, next).await.is_ok());
    let text = store
        .read_text(&MemoryStore::bot_memory("alpha"))
        .await
        .unwrap();
    assert!(text.contains("@effective[2026-09-01,2026-10-14) Use OpenCode for merges."));
    assert!(text.contains("@effective:2026-10-14 Use T3 for merges."));
    let state = state::read_state(&store, &MemoryStore::bot_state("alpha"))
        .await
        .unwrap();
    let temporal = &state.temporal[&hash_fact("Use T3 for merges.")];
    assert_eq!(temporal.from.as_deref(), Some("2026-10-14"));
    assert_eq!(temporal.to, None);
}

#[tokio::test]
async fn forget_writes_a_tombstone() {
    let store = alpha().await;
    let before = bot_revision(&store, "alpha").await;
    let mut added = base("alpha", Op::Add, &before);
    added.fact = Some("Likes coffee.");
    let added = mutate(&store, added).await.unwrap();
    let mut forgotten = base("alpha", Op::Forget, &added.revision);
    forgotten.source_quote = "forget that I like coffee";
    forgotten.source_text = "Please forget that I like coffee.";
    forgotten.source_message_id = "m2";
    forgotten.fact = added.line.as_deref();
    let forgotten = mutate(&store, forgotten).await.unwrap();
    assert!(forgotten.forgotten.is_some());
    let state = state::read_state(&store, &MemoryStore::bot_state("alpha"))
        .await
        .unwrap();
    assert!(state.forgotten.contains(&hash_fact("Likes coffee.")));
    let mut again = base("alpha", Op::Add, &forgotten.revision);
    again.fact = Some("Likes coffee.");
    again.source_message_id = "m3";
    assert_eq!(code(mutate(&store, again).await.unwrap_err()), "forgotten");
    let mut restored = base("alpha", Op::Add, &forgotten.revision);
    restored.fact = Some("Likes coffee.");
    restored.restore = true;
    restored.source_quote = "remember that I like coffee";
    restored.source_text = "Please remember that I like coffee.";
    restored.source_message_id = "m4";
    assert!(mutate(&store, restored).await.is_ok());
}

#[tokio::test]
async fn a_forgotten_fact_cannot_return_through_another_line() {
    let store = alpha().await;
    let coffee_revision = bot_revision(&store, "alpha").await;
    let mut coffee = base("alpha", Op::Add, &coffee_revision);
    coffee.fact = Some("Likes coffee.");
    let coffee = mutate(&store, coffee).await.unwrap();
    let tea_revision = bot_revision(&store, "alpha").await;
    let mut tea = base("alpha", Op::Add, &tea_revision);
    tea.fact = Some("Likes tea.");
    let tea = mutate(&store, tea).await.unwrap();
    let forget_revision = bot_revision(&store, "alpha").await;
    let mut forget = base("alpha", Op::Forget, &forget_revision);
    forget.fact = coffee.line.as_deref();
    mutate(&store, forget).await.unwrap();
    for op in [Op::Replace, Op::Supersede] {
        let current = bot_revision(&store, "alpha").await;
        let mut sneaked = base("alpha", op, &current);
        sneaked.old = tea.line.as_deref();
        sneaked.new_fact = Some("Likes coffee.");
        assert_eq!(
            code(mutate(&store, sneaked).await.unwrap_err()),
            "forgotten"
        );
    }
    let text = store
        .read_text(&MemoryStore::bot_memory("alpha"))
        .await
        .unwrap();
    assert!(!text.to_lowercase().contains("coffee"));
    let restore_revision = bot_revision(&store, "alpha").await;
    let mut restored = base("alpha", Op::Replace, &restore_revision);
    restored.old = tea.line.as_deref();
    restored.new_fact = Some("Likes coffee.");
    restored.restore = true;
    assert!(mutate(&store, restored).await.is_ok());
    let state = state::read_state(&store, &MemoryStore::bot_state("alpha"))
        .await
        .unwrap();
    assert!(!state.forgotten.contains(&hash_fact("Likes coffee.")));
}

#[tokio::test]
async fn notes_get_frontmatter_and_path_checks() {
    let store = alpha().await;
    let mut created = base("alpha", Op::Note, "");
    created.source_quote = "write a note about the deploy";
    created.source_text = "Please write a note about the deploy.";
    created.path = Some("memory/notes/projects/alpha.md");
    created.description = Some("Alpha deploy");
    created.content = Some("# Alpha deploy\n\nRuns on the HP server.");
    let created = mutate(&store, created).await.unwrap();
    let note = search::read_note(&store, "alpha", "memory/notes/projects/alpha.md", 8_000)
        .await
        .unwrap();
    assert!(note.text.starts_with("---\ndescription: Alpha deploy"));
    assert_eq!(note.revision, created.revision);
    let mut stale = base("alpha", Op::Note, "");
    stale.source_quote = "update the note";
    stale.source_text = "Please update the note.";
    stale.source_message_id = "m2";
    stale.path = Some(&note.path);
    stale.description = Some("Alpha deploy");
    stale.content = Some("New.");
    assert_eq!(code(mutate(&store, stale).await.unwrap_err()), "conflict");
    let mut escape = base("alpha", Op::Note, "");
    escape.source_quote = "write a note";
    escape.source_text = "Please write a note.";
    escape.source_message_id = "m3";
    escape.path = Some("memory/notes/../../escape.md");
    escape.description = Some("Escape");
    escape.content = Some("No.");
    assert!(mutate(&store, escape).await.is_err());
}

#[tokio::test]
async fn shared_memory_has_its_own_revision() {
    let store = alpha().await;
    let before = shared_revision(&store).await;
    let mut added = base("alpha", Op::Add, &before);
    added.scope = Scope::Shared;
    added.source_quote = "my name is Cristian";
    added.source_text = "My name is Cristian and I live in Ås.";
    added.fact = Some("The user's name is Cristian.");
    let added = mutate(&store, added).await.unwrap();
    let text = store.read_text(MemoryStore::shared_memory()).await.unwrap();
    assert!(text.contains("The user's name is Cristian."));
    let raw = store.read_text(MemoryStore::shared_state()).await.unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value["revisions"]["shared-memory.md"], added.revision);
}

#[tokio::test]
async fn search_finds_a_fact_and_the_view_carries_provenance() {
    let store = alpha().await;
    let before = bot_revision(&store, "alpha").await;
    let mut added = base("alpha", Op::Add, &before);
    added.fact = Some("Prefers short answers.");
    added.category = Some("preference");
    let added = mutate(&store, added).await.unwrap();
    let hits = search::search(&store, "alpha", "short answers", 20, true)
        .await
        .unwrap();
    assert_eq!(hits[0].path, "MEMORY.md");
    assert!(hits[0].text.to_lowercase().contains("short answers"));
    let view = read_view(&store, "alpha").await.unwrap();
    assert_eq!(view.bot.revision, added.revision);
    assert_eq!(view.bot.facts.len(), 1);
    assert_eq!(view.bot.facts[0].text, "Prefers short answers.");
    assert_eq!(view.bot.facts[0].category.as_deref(), Some("preference"));
    let provenance = view.bot.facts[0].provenance.as_ref().unwrap();
    assert_eq!(provenance.author, "alpha");
    assert_eq!(provenance.source_message_id.as_deref(), Some("m1"));
    assert!(view.shared.facts.is_empty());
    assert_eq!(view.forgotten, 0);
}

#[tokio::test]
async fn missing_or_damaged_state_is_empty() {
    let store = alpha().await;
    let key = MemoryStore::bot_state("alpha");
    let missing = state::read_state(&store, &key).await.unwrap();
    assert!(missing.revisions.is_empty());
    store
        .write_text(&key, "not json", Some(0o600))
        .await
        .unwrap();
    let damaged = state::read_state(&store, &key).await.unwrap();
    assert!(damaged.revisions.is_empty());
    assert_eq!(revision_of(""), revision_of(""));
}
