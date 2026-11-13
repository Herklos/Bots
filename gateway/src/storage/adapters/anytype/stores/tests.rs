use super::*;
use crate::storage::adapters::anytype::tests::mock::Mock;
use serde_json::json;

fn cache() -> TypedCache {
    TypedCache::new("m_settings", Duration::from_secs(30))
}
fn document(id: &str, name: &str) -> Value {
    json!({"id":id, "type":"m_settings", "properties":{"m_user_name":name}})
}

#[tokio::test]
async fn strict_poll_hydrates_every_page() {
    let mock = Mock::start().await;
    for i in 0..31 {
        mock.seed_named(&format!("settings-{i}"), "m_settings");
    }
    let client = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let cache = cache();
    cache.refresh(&client).await.unwrap();
    let view = cache.view();
    assert!(view.complete);
    assert_eq!(view.objects.len(), 31);
}

#[tokio::test]
async fn mock_poll_loads_full_objects_and_refreshes_peer_edits() {
    let mock = Mock::start().await;
    let client = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let created = client
        .create_object(
            &json!({"type":"m_settings", "name":"settings", "properties":{"m_user_name":"Ada"}}),
            "settings-create",
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    let cache = cache();
    cache.refresh(&client).await.unwrap();
    let view = cache.view();
    assert!(view.complete);
    assert_eq!(view.objects[id].object["properties"]["m_user_name"], "Ada");
    assert!(mock.peer_write(id, json!({"m_user_name":"Grace"}), false));
    cache.invalidate(id);
    assert!(!cache.view().complete);
    assert!(!cache.view().objects[id].fresh);
    cache.refresh(&client).await.unwrap();
    assert_eq!(
        cache.view().objects[id].object["properties"]["m_user_name"],
        "Grace"
    );
    assert!(cache.view().complete);
}

#[tokio::test]
async fn failed_poll_and_missing_search_row_never_delete_cached_data() {
    let mock = Mock::start().await;
    let client = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let object = client
        .create_object(
            &json!({"type":"m_settings", "name":"settings"}),
            "settings-create",
        )
        .await
        .unwrap();
    let id = object["id"].as_str().unwrap();
    let cache = cache();
    cache.refresh(&client).await.unwrap();
    mock.set_status(Some(503));
    assert!(cache.refresh(&client).await.is_err());
    assert!(cache.view().objects.contains_key(id));
    mock.set_status(None);
    client.archive_object(id, "archive-settings").await.unwrap();
    cache.refresh(&client).await.unwrap();
    let view = cache.view();
    assert!(!view.complete);
    assert!(!view.objects[id].fresh);
    assert!(view.objects.contains_key(id));
}

#[test]
fn old_poll_and_old_confirmed_fetch_cannot_overwrite_later_events() {
    let cache = cache();
    let old = cache.token();
    cache.invalidate("obj-1");
    assert!(matches!(
        cache.install_snapshot(
            old,
            BTreeMap::from([("obj-1".into(), document("obj-1", "old"))]),
            Instant::now()
        ),
        Err(CacheError::Superseded)
    ));
    assert!(matches!(
        cache.install_confirmed(old, Instant::now(), document("obj-1", "old")),
        Err(CacheError::Superseded)
    ));
    let fresh = cache.token();
    cache
        .install_confirmed(fresh, Instant::now(), document("obj-1", "new"))
        .unwrap();
    assert_eq!(
        cache.view().objects["obj-1"].object["properties"]["m_user_name"],
        "new"
    );
    assert!(!cache.view().complete); // A single object is not a complete snapshot.
}

#[test]
fn unknown_invalidations_and_overlapping_polls_cannot_be_reported_complete() {
    let cache = cache();
    let first = cache.token();
    let second = cache.token();
    cache
        .install_snapshot(second, BTreeMap::new(), Instant::now())
        .unwrap();
    assert!(cache.view().complete);
    assert!(matches!(
        cache.install_snapshot(first, BTreeMap::new(), Instant::now()),
        Err(CacheError::Superseded)
    ));
    cache.invalidate("new-object");
    let after_event = cache.token();
    cache
        .install_snapshot(after_event, BTreeMap::new(), Instant::now())
        .unwrap();
    assert!(!cache.view().complete);
}

#[test]
fn wrong_type_or_partial_write_reply_is_never_installed() {
    let cache = cache();
    let token = cache.token();
    assert!(cache
        .install_confirmed(token, Instant::now(), json!({"id":"obj-1", "etag":"x"}))
        .is_err());
    assert!(cache
        .install_confirmed(
            token,
            Instant::now(),
            json!({"id":"obj-1", "type":"m_settings", "etag":"x"})
        )
        .is_err());
    assert!(cache
        .install_confirmed(token, Instant::now(), json!({"id":"obj-1", "type":"m_bot"}))
        .is_err());
    assert!(cache.view().objects.is_empty());
}

#[test]
fn slow_snapshot_uses_start_time_for_freshness() {
    let cache = TypedCache::new("m_settings", Duration::from_secs(1));
    let started = Instant::now() - Duration::from_secs(2);
    cache
        .install_snapshot(
            cache.token(),
            BTreeMap::from([("obj-1".into(), document("obj-1", "Ada"))]),
            started,
        )
        .unwrap();
    assert!(!cache.view().complete);
    assert!(!cache.view().objects["obj-1"].fresh);
}
