use super::mock::Mock;
use crate::storage::adapters::anytype::client::AnytypeClient;
use serde_json::json;

#[tokio::test]
async fn schema_creates_are_idempotent_and_lists_walk() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let body = json!({"api_key": "note", "name": "Note", "layout": "basic"});
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

#[tokio::test]
async fn explicit_keys_come_back_unchanged_and_digits_are_split() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let kept = api
        .create_property(
            &json!({"name": "Explicit", "format": "text", "key": "hb_explicit_key"}),
            "explicit",
        )
        .await
        .unwrap();
    assert_eq!(kept["key"], "hb_explicit_key");

    let split = api
        .create_property(
            &json!({"name": "Probe", "format": "text", "key": "hb_probe2"}),
            "probe",
        )
        .await
        .unwrap();
    assert_eq!(split["key"], "hb_probe_2");

    let typed = api
        .create_type(
            &json!({
                "name": "Spike",
                "api_key": "hb_spike_file",
                "layout": "basic",
                "property_definitions": [{"name": "Spike Path", "format": "text"}],
            }),
            "spike-type",
        )
        .await
        .unwrap();
    assert_eq!(typed["key"], "hb_spike_file");
    assert_eq!(typed["created"]["properties"][0]["key"], "spike_path");
    let listed = api.list_properties().await.unwrap();
    assert!(listed.iter().any(|row| row["key"] == "spike_path"));
}
