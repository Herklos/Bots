use super::mock::Mock;
use crate::storage::adapters::anytype::client::AnytypeClient;
use serde_json::json;

#[tokio::test]
async fn schema_creates_are_idempotent_and_lists_walk() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let body = json!({"key": "note", "name": "Note", "layout": "basic"});
    let first = api.create_type(&body, "type-1").await.unwrap();
    let second = api.create_type(&body, "type-1").await.unwrap();
    assert_eq!(first["key"], second["key"]);
    assert_eq!(mock.type_count(), 1);

    let property = json!({"key": "title", "name": "Title", "format": "text"});
    api.create_property(&property, "prop-1").await.unwrap();
    let properties = api.list_properties().await.unwrap();
    assert_eq!(properties.len(), 1);
    assert_eq!(properties[0]["format"], "text");
}
