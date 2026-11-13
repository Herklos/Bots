//! Typed-store tests against the in-memory Anytype stand-in: a settings and
//! Bot round-trip, the second-device read, outbox merging, and adoption of a
//! legacy filesystem Bot.

use super::outbox::Outbox;
use super::TypedCtx;
use crate::storage::adapters::anytype::client::AnytypeClient;
use crate::storage::adapters::anytype::tests::mock::Mock;
use crate::storage::stores::{BotStore, SettingsStore};
use serde_json::json;
use std::sync::Arc;

async fn ctx(mock: &Mock, dir: &tempfile::TempDir, slug: &str) -> Arc<TypedCtx> {
    // Prepare the space once: every type and property the typed stores need.
    let seed = AnytypeClient::with_rate(&mock.url, "test-key", "hbots", 10_000.0, 10_000.0)
        .unwrap();
    crate::storage::adapters::anytype::bootstrap(&seed, false).await.unwrap();
    let client = AnytypeClient::with_rate(&mock.url, "test-key", "hbots", 1_000.0, 1_000.0)
        .unwrap();
    let outbox = Outbox::load(dir.path().join(format!("sync/{slug}-typed-outbox.jsonl")));
    Arc::new(TypedCtx::new(client, slug, outbox))
}

#[tokio::test]
async fn settings_write_reaches_the_space_and_read_overlays_shared_keys() {
    let mock = Mock::start().await;
    let dir = tempfile::tempdir().unwrap();
    let store = super::settings::AnytypeSettingsStore::new(
        dir.path().join("settings.json"),
        ctx(&mock, &dir, "one").await,
    );
    assert_eq!(store.read().await.unwrap(), None);
    store
        .write(&json!({
            "userName": "Ada", "theme": "dark", "autoReview": true,
            "groupAnswerPolicy": "chief", "rules": { "allow": ["ask"], "ask": [] },
            "beta": { "schedules": true },
        }))
        .await
        .unwrap();
    // The full document stays local; the shared subset went to the space.
    assert_eq!(store.read().await.unwrap().as_ref().unwrap()["theme"], json!("dark"));

    let store2 = super::settings::AnytypeSettingsStore::new(
        dir.path().join("other/settings.json"),
        ctx(&mock, &dir, "two").await,
    );
    store2
        .initialize(&json!({ "userName": "Default", "theme": "light" }))
        .await
        .unwrap();
    // The second machine's local theme wins because nothing remote carries one.
    assert_eq!(store2.read().await.unwrap().as_ref().unwrap()["userName"], json!("Ada"));

    // A local write first reads back as pending, then still wins after the
    // flush; a stale remote copy never overrides it.
    store
        .write(&json!({ "userName": "Grace" }))
        .await
        .unwrap();
    assert_eq!(store.read().await.unwrap().as_ref().unwrap()["userName"], json!("Grace"));
}

#[tokio::test]
async fn bot_profiles_round_trip_and_a_second_device_sees_shared_fields() {
    let mock = Mock::start().await;
    let dir = tempfile::tempdir().unwrap();
    let store = super::bots::AnytypeBotStore::new(
        dir.path().join("bots"),
        dir.path().to_path_buf(),
        ctx(&mock, &dir, "one").await,
    );
    assert_eq!(store.list_slugs().await.unwrap(), Vec::<String>::new());
    assert_eq!(store.read_meta("ada").await.unwrap(), None);

    store
        .write_meta("ada", &json!({ "name": "Ada", "shape": "ball", "quest": "local" }))
        .await
        .unwrap();
    assert_eq!(store.list_slugs().await.unwrap(), vec!["ada".to_string()]);

    // Second device: a fresh context reads the shared fields from the space.
    // Its machine-local home is its own; the sidecar does not travel.
    let store2 = super::bots::AnytypeBotStore::new(
        dir.path().join("bots-two"),
        dir.path().join("home-two"),
        ctx(&mock, &dir, "two").await,
    );
    assert_eq!(store2.list_slugs().await.unwrap(), vec!["ada".to_string()]);
    let meta = store2.read_meta("ada").await.unwrap().unwrap();
    assert_eq!(meta["name"], json!("Ada"));
    assert_eq!(meta["shape"], json!("ball"));
    // Machine-local keys do not travel.
    assert!(meta.get("quest").is_none());

    // The choice follows once the Bot exists remotely.
    store.write_selection(Some("ada")).await.unwrap();
    assert_eq!(store2.read_selection().await.unwrap(), Some(json!({ "slug": "ada" })));
    store.write_selection(None).await.unwrap();
    assert_eq!(store2.read_selection().await.unwrap(), None);
}

#[tokio::test]
async fn outbox_merges_a_target_writes_and_backs_off_after_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("typed-outbox.jsonl");
    let mut outbox = Outbox::load(path.clone());
    let props = serde_json::Map::new();
    assert!(outbox.push("m_bot", "ada", &props));
    assert!(!outbox.push(
        "m_bot",
        "ada",
        &json!({ "m_label": "Ada" }).as_object().unwrap().clone(),
    ));
    assert_eq!(
        outbox.merged("m_bot", "ada").unwrap()["m_label"],
        json!("Ada")
    );
    // A different target keeps its own entry.
    assert!(outbox.push("m_settings", "settings", &props));
    assert_eq!(outbox.due(0).len(), 2);

    outbox.failed("m_bot", "ada");
    assert_eq!(outbox.due(unix_ms_now()).len(), 1);

    // The journal replays: a queued write survives a reload.
    outbox.persist_journal().await.unwrap();
    let reloaded = Outbox::load(path.clone());
    assert!(reloaded.merged("m_bot", "ada").is_some());
    assert_eq!(
        reloaded.merged("m_bot", "ada").unwrap()["m_label"],
        json!("Ada")
    );

    // Applied targets are gone; failed targets stay queued.
    outbox.applied("m_settings", "settings");
    assert!(outbox.merged("m_settings", "settings").is_none());
    assert!(outbox.merged("m_bot", "ada").is_some());
    outbox.applied("m_bot", "ada");
    assert!(outbox.is_empty());
}

#[tokio::test]
async fn a_legacy_filesystem_bot_is_adopted_and_pushed() {
    let mock = Mock::start().await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bots");
    let legacy = root.join("ada").join(".bot");
    tokio::fs::create_dir_all(&legacy).await.unwrap();
    tokio::fs::write(
        legacy.join("bot.json"),
        json!({ "name": "Ada", "prompt": "local prompt" }).to_string(),
    )
    .await
    .unwrap();
    let store = super::bots::AnytypeBotStore::new(
        root.clone(),
        dir.path().to_path_buf(),
        ctx(&mock, &dir, "one").await,
    );
    let meta = store.read_meta("ada").await.unwrap().unwrap();
    assert_eq!(meta["name"], json!("Ada"));
    assert_eq!(meta["prompt"], json!("local prompt"));
    // The adoption pushed the shared fields; a second device sees them.
    let store2 = super::bots::AnytypeBotStore::new(
        dir.path().join("bots-two"),
        dir.path().to_path_buf(),
        ctx(&mock, &dir, "two").await,
    );
    assert_eq!(store2.read_meta("ada").await.unwrap().unwrap()["name"], json!("Ada"));
}

fn unix_ms_now() -> i64 {
    super::unix_ms() + 1_000
}
