//! Suggest, the pending cap, a dismissed hash, and accept.
use super::evidence::ACCEPTED_VIA;
use super::facts::hash_fact;
use super::state;
use super::suggest::{accept, dismiss, propose, Acceptance, Proposal, ProposalResult};
use super::user::{write, NewFact};
use super::MemoryStore;
use crate::storage::adapters::memory::MemoryWorkspace;
use std::sync::Arc;

const NOW: &str = "2026-11-09T10:00:00.000Z";
const STILL: &str = "2027-02-07T10:00:00.000Z";
const LATER: &str = "2027-02-08T10:00:00.000Z";

fn store() -> MemoryStore {
    MemoryStore::new(Arc::new(MemoryWorkspace::new()))
}

async fn ready() -> MemoryStore {
    let store = store();
    store.bootstrap("ada", "Ada").await.unwrap();
    store
}

fn proposal<'a>(text: &'a str, now: &'a str) -> Proposal<'a> {
    Proposal {
        slug: "ada",
        text,
        why: "The person said this is worth keeping.",
        category: Some("preference"),
        now,
        user_root: None,
    }
}

fn stored(result: ProposalResult) -> String {
    match result {
        ProposalResult::Stored { id, .. } => id,
        ProposalResult::Dropped => panic!("expected a stored suggestion"),
    }
}

async fn memory_text(store: &MemoryStore) -> String {
    store
        .read_text(&MemoryStore::bot_memory("ada"))
        .await
        .unwrap()
}

#[tokio::test]
async fn twenty_pending_is_the_cap_and_memory_stays_put() {
    let store = ready().await;
    let before = memory_text(&store).await;
    for index in 0..20 {
        let text = format!("Fact number {index} is worth keeping.");
        let id = stored(propose(&store, &proposal(&text, NOW)).await.unwrap());
        assert!(id.starts_with("s_"));
    }
    let extra = propose(&store, &proposal("Fact number 20 is worth keeping.", NOW))
        .await
        .unwrap_err();
    assert_eq!(extra.code, "cap");
    assert!(extra.message.contains("20"), "{}", extra.message);
    let queued = store
        .read_text(".botapp/memory-suggestions/ada.json")
        .await
        .unwrap();
    assert_eq!(queued.matches("\"id\"").count(), 20);
    assert!(!queued.contains("Fact number 20"));
    assert_eq!(memory_text(&store).await, before);
}

#[tokio::test]
async fn a_dismissed_hash_suppresses_repeats_for_ninety_days() {
    let store = ready().await;
    let text = "Takes tea at four.";
    let id = stored(propose(&store, &proposal(text, NOW)).await.unwrap());
    dismiss(&store, "ada", &id, NOW).await.unwrap();
    assert!(matches!(
        propose(&store, &proposal(text, NOW)).await.unwrap(),
        ProposalResult::Dropped
    ));
    assert!(matches!(
        propose(&store, &proposal(text, STILL)).await.unwrap(),
        ProposalResult::Dropped
    ));
    let again = stored(propose(&store, &proposal(text, LATER)).await.unwrap());
    assert!(again.starts_with("s_"));
    let queued = store
        .read_text(".botapp/memory-suggestions/ada.json")
        .await
        .unwrap();
    assert!(queued.contains(text));
    assert_eq!(queued.matches("\"hash\"").count(), 1);
}

#[tokio::test]
async fn accept_applies_via_and_an_edit_stores_the_new_text() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready().await;
    let id = stored(
        propose(&store, &proposal("Takes tea at four.", NOW))
            .await
            .unwrap(),
    );
    let saved = accept(
        &store,
        &Acceptance {
            slug: "ada",
            id: &id,
            edited: None,
            now: NOW,
            user_root: Some(dir.path()),
            bot_name: "Ada",
        },
    )
    .await
    .unwrap();
    let line = saved.line.unwrap();
    assert!(line.contains("[preference] Takes tea at four."));
    let memory = memory_text(&store).await;
    assert!(memory.contains("Takes tea at four."));
    let kept = state::read_state(&store, &MemoryStore::bot_state("ada"))
        .await
        .unwrap();
    let provenance = &kept.provenance[&hash_fact("Takes tea at four.")];
    assert_eq!(provenance.author, "ada");
    assert_eq!(provenance.via.as_deref(), Some(ACCEPTED_VIA));
    assert_eq!(
        provenance.quote.as_deref(),
        Some("accepted by the person at 2026-11-09T10:00:00.000Z")
    );
    let queued = store
        .read_text(".botapp/memory-suggestions/ada.json")
        .await
        .unwrap();
    assert!(!queued.contains(&id));

    let edited_id = stored(
        propose(&store, &proposal("Lives in Lyon.", NOW))
            .await
            .unwrap(),
    );
    let edited = accept(
        &store,
        &Acceptance {
            slug: "ada",
            id: &edited_id,
            edited: Some("Lives in Paris."),
            now: NOW,
            user_root: Some(dir.path()),
            bot_name: "Ada",
        },
    )
    .await
    .unwrap();
    assert!(edited.line.unwrap().contains("Lives in Paris."));
    let memory = memory_text(&store).await;
    assert!(memory.contains("Lives in Paris."));
    assert!(!memory.contains("Lives in Lyon."));
    let kept = state::read_state(&store, &MemoryStore::bot_state("ada"))
        .await
        .unwrap();
    assert_eq!(
        kept.provenance[&hash_fact("Lives in Paris.")]
            .via
            .as_deref(),
        Some(ACCEPTED_VIA)
    );
}

#[tokio::test]
async fn a_matching_fact_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let store = ready().await;
    store
        .write_text(
            &MemoryStore::bot_memory("ada"),
            "# Ada memory\n\n- Likes short answers.\n",
            None,
        )
        .await
        .unwrap();
    write(
        dir.path(),
        "ada",
        &[NewFact {
            text: "Lives in Paris.".into(),
            at: NOW.into(),
        }],
        NOW,
    )
    .await
    .unwrap();
    let user_before = std::fs::read(dir.path().join(".botapp/memory-user/ada.md")).unwrap();
    let root = Some(dir.path());
    let bot_fact = Proposal {
        user_root: root,
        ..proposal("Likes short answers.", NOW)
    };
    assert!(matches!(
        propose(&store, &bot_fact).await.unwrap(),
        ProposalResult::Dropped
    ));
    let user_fact = Proposal {
        user_root: root,
        ..proposal("Lives in Paris.", NOW)
    };
    assert!(matches!(
        propose(&store, &user_fact).await.unwrap(),
        ProposalResult::Dropped
    ));
    assert!(store
        .read_text(".botapp/memory-suggestions/ada.json")
        .await
        .is_err());
    assert_eq!(
        std::fs::read(dir.path().join(".botapp/memory-user/ada.md")).unwrap(),
        user_before
    );
}
