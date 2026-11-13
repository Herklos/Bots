use super::mock::Mock;
use crate::storage::adapters::anytype::client::AnytypeClient;
use serde_json::json;

#[tokio::test]
async fn the_same_idempotency_key_does_not_create_a_second_object() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let body = json!({"type": "page", "name": "Once"});
    let first = api.create_object(&body, "same-key").await.unwrap();
    let second = api.create_object(&body, "same-key").await.unwrap();
    assert_eq!(first["id"], second["id"]);
    assert_eq!(mock.object_count(), 1);

    let other = api.create_object(&body, "other-key").await.unwrap();
    assert_ne!(other["id"], first["id"]);
    assert_eq!(mock.object_count(), 2);

    let conflict = api
        .create_object(&json!({"type": "page", "name": "Different"}), "same-key")
        .await
        .unwrap_err();
    assert!(matches!(
        conflict,
        crate::storage::adapters::anytype::client::AnytypeError::Bad { status: 409, .. }
    ));
    assert_eq!(mock.object_count(), 2);
}

#[tokio::test]
async fn a_patch_replay_does_not_apply_twice() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let created = api
        .create_object(
            &json!({"type": "page", "name": "Note", "properties": {"m_rev": 1}}),
            "create-1",
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    let etag = created["etag"].as_str().unwrap();
    let body = json!({"ops": [{"op": "set_properties", "set": {"m_rev": 2}}]});
    let first = api
        .patch_object(id, &body, Some(etag), "patch-key")
        .await
        .unwrap();
    let second = api
        .patch_object(id, &body, Some(etag), "patch-key")
        .await
        .unwrap();
    assert_eq!(first["etag"], second["etag"]);
    let fetched = api.get_object(id).await.unwrap();
    assert_eq!(fetched["etag"], first["etag"]);
    assert_eq!(fetched["properties"]["m_rev"], 2);
}

#[tokio::test]
async fn an_empty_idempotency_key_is_refused() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let error = api
        .create_object(&json!({"type": "page", "name": "Note"}), "")
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        crate::storage::adapters::anytype::client::AnytypeError::Bad { status: 400, .. }
    ));
    assert_eq!(mock.object_count(), 0);
}
