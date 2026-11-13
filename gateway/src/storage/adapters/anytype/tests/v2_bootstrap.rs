use super::mock::Mock;
use crate::storage::adapters::anytype::client::AnytypeClient;
use crate::storage::adapters::anytype::schema::table::{COLUMNS, TABLES};
use crate::storage::adapters::anytype::schema::{bootstrap, read_meta};
use serde_json::json;
use std::collections::BTreeMap;

#[tokio::test]
async fn v1_space_upgrades_without_changing_existing_file_objects() {
    let mock = Mock::start().await;
    let api = AnytypeClient::with_rate(&mock.url, "k", "space-1", 10_000.0, 10_000.0).unwrap();
    for column in COLUMNS {
        api.create_property(
            &json!({"name":column.name,"key":column.key,"format":column.format}),
            &format!("v1-property-{}", column.key),
        )
        .await
        .unwrap();
    }
    for table in TABLES {
        let definitions: Vec<_> = table
            .properties
            .iter()
            .map(|property| json!({"property":property}))
            .collect();
        api.create_type(&json!({"name":table.name,"api_key":table.key,"layout":table.layout,"property_definitions":definitions}), &format!("v1-type-{}", table.key)).await.unwrap();
    }
    api.create_object(&json!({"type":"m_meta","name":"m_meta","properties":{"m_schema_version":1,"m_min_gateway":"0.4.3","m_keys_json":"{}"}}), "v1-meta").await.unwrap();
    let file = api.create_object(&json!({"type":"m_file","name":"memory","properties":{"m_path":"ada/memory/MEMORY.md","m_content":"keep me","m_sha":"sha"}}), "v1-file").await.unwrap();
    let file_id = file["id"].as_str().unwrap();
    let before = api.get_object(file_id).await.unwrap();
    let preview = bootstrap(&api, true).await.unwrap();
    assert!(!preview.schema_ready);
    assert_eq!(preview.types.len(), 16);
    assert!(preview.types.iter().any(|kind| kind == "m_bot"));
    assert!(preview.meta);
    let report = bootstrap(&api, false).await.unwrap();
    assert!(report.schema_ready);
    assert_eq!(report.types.len(), 16);
    let bot_type = api.get_type("m_bot").await.unwrap();
    let properties = bot_type["blocks"][0]["properties"].as_array().unwrap();
    for key in [
        "m_bot_id",
        "m_active_device",
        "last_modified_date",
        "backlinks",
    ] {
        assert!(
            properties
                .iter()
                .any(|property| property["property"] == key),
            "missing {key}"
        );
    }
    assert_eq!(api.get_object(file_id).await.unwrap(), before);
    let rows = api
        .search(&json!({"type":"m_meta","query":"m_meta"}))
        .await
        .unwrap();
    let object = api
        .get_object(rows[0]["id"].as_str().unwrap())
        .await
        .unwrap();
    let meta = read_meta(&object, &BTreeMap::new());
    assert_eq!(meta.version, Some(2));
    assert_eq!(meta.min_gateway.as_deref(), Some("0.4.4"));
    let second = bootstrap(&api, false).await.unwrap();
    assert!(second.types.is_empty() && second.properties.is_empty() && !second.meta);
    assert!(bootstrap(&api, true).await.unwrap().schema_ready);
}

#[tokio::test]
async fn v2_metadata_requires_the_distinct_minimum_release() {
    let mock = Mock::start().await;
    let api = AnytypeClient::with_rate(&mock.url, "k", "space-1", 10_000.0, 10_000.0).unwrap();
    bootstrap(&api, false).await.unwrap();
    let rows = api
        .search(&json!({"type":"m_meta","query":"m_meta"}))
        .await
        .unwrap();
    let id = rows[0]["id"].as_str().unwrap();
    let object = api.get_object(id).await.unwrap();
    api.patch_object(
        id,
        &json!({"ops":[{"op":"set_properties","set":{"m_min_gateway":"0.4.3"}}]}),
        object["etag"].as_str(),
        "v2-too-low",
    )
    .await
    .unwrap();
    assert!(!bootstrap(&api, true).await.unwrap().schema_ready);
    assert!(bootstrap(&api, false).await.unwrap().schema_ready);
    let object = api.get_object(id).await.unwrap();
    assert_eq!(
        read_meta(&object, &BTreeMap::new()).min_gateway.as_deref(),
        Some("0.4.4")
    );
}
