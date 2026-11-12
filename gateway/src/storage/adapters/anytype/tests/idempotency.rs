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
}
