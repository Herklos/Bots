use super::mock::Mock;
use crate::storage::adapters::anytype::client::AnytypeClient;
use crate::storage::adapters::anytype::schema::table::{self, COLUMNS, TABLES};
use crate::storage::adapters::anytype::schema::{bootstrap, read_meta, schema_ready};
use serde_json::json;
use std::collections::BTreeMap;

fn client(mock: &Mock) -> AnytypeClient {
    AnytypeClient::new(&mock.url, "k", "space-1").unwrap()
}

#[test]
fn declared_keys_have_no_digits() {
    for column in COLUMNS {
        assert!(
            column.key.chars().all(|c| !c.is_ascii_digit()),
            "{}",
            column.key
        );
    }
    for table in TABLES {
        assert!(table.key.chars().all(|c| !c.is_ascii_digit()));
    }
    assert!(table::option_allowed("hb_conflict_state", "open"));
    assert!(!table::option_allowed("hb_conflict_state", "later"));
    assert!(!table::option_allowed("hb_sha", "open"));
}

#[tokio::test]
async fn bootstrap_twice_is_a_no_op() {
    let mock = Mock::start().await;
    let api = client(&mock);
    let preview = bootstrap(&api, true).await.unwrap();
    assert!(preview.dry_run);
    assert_eq!(preview.types.len(), TABLES.len());
    assert_eq!(preview.properties.len(), COLUMNS.len());
    assert!(preview.meta);
    assert_eq!(mock.type_count(), 0);
    assert_eq!(mock.object_count(), 0);

    let first = bootstrap(&api, false).await.unwrap();
    assert_eq!(first.types.len(), TABLES.len());
    assert!(first.meta);
    assert!(first.schema_ready);
    let types = mock.type_count();
    let objects = mock.object_count();
    assert_eq!(objects, 1);

    let second = bootstrap(&api, false).await.unwrap();
    assert!(second.types.is_empty());
    assert!(second.properties.is_empty());
    assert!(!second.meta);
    assert!(second.schema_ready);
    assert!(second.keys.is_empty());
    assert_eq!(mock.type_count(), types);
    assert_eq!(mock.object_count(), objects);
    assert!(schema_ready(&api).await.unwrap());

    let rows = api
        .search(&json!({ "type": "hb_meta", "query": "hb_meta" }))
        .await
        .unwrap();
    let id = rows[0]["id"].as_str().unwrap();
    let fetched = api.get_object(id).await.unwrap();
    api.patch_object(
        id,
        &json!({ "ops": [{ "op": "set_properties", "set": { "hb_note": "extra" } }] }),
        Some(fetched["etag"].as_str().unwrap()),
        "note-1",
    )
    .await
    .unwrap();
    let rows = api
        .search(&json!({ "type": "hb_meta", "query": "hb_meta" }))
        .await
        .unwrap();
    let meta = read_meta(&rows[0], &BTreeMap::new());
    assert_eq!(meta.version, Some(1));
    assert_eq!(meta.min_gateway.as_deref(), Some(env!("CARGO_PKG_VERSION")));
    assert!(meta.keys.is_empty());
    assert!(schema_ready(&api).await.unwrap());
    let state = api.list_properties().await.unwrap();
    let conflict = state
        .iter()
        .find(|row| row["key"] == "hb_conflict_state")
        .unwrap();
    assert_eq!(conflict["format"], "select");
    assert!(conflict.get("options").is_none());
}

#[tokio::test]
async fn a_missing_property_is_the_only_create() {
    let mock = Mock::start().await;
    let api = client(&mock);
    for table in TABLES {
        api.create_type(
            &json!({ "name": table.name, "api_key": table.key, "layout": table.layout }),
            &format!("seed-type-{}", table.key),
        )
        .await
        .unwrap();
    }
    for column in COLUMNS {
        if column.key == "hb_mime" {
            continue;
        }
        api.create_property(
            &json!({ "name": column.name, "key": column.key, "format": column.format }),
            &format!("seed-property-{}", column.key),
        )
        .await
        .unwrap();
    }
    let report = bootstrap(&api, false).await.unwrap();
    assert!(report.types.is_empty());
    assert_eq!(report.properties, vec!["hb_mime".to_string()]);
    let again = bootstrap(&api, false).await.unwrap();
    assert!(again.properties.is_empty());
}

#[tokio::test]
async fn a_type_without_its_property_definitions_is_repaired() {
    // A space prepared before types carried their field list: the types and
    // properties exist, but a `fields` search needs the type to name them.
    let mock = Mock::start().await;
    let api = client(&mock);
    for table in TABLES {
        api.create_type(
            &json!({ "name": table.name, "api_key": table.key, "layout": table.layout }),
            &format!("seed-type-{}", table.key),
        )
        .await
        .unwrap();
    }
    for column in COLUMNS {
        api.create_property(
            &json!({ "name": column.name, "key": column.key, "format": column.format }),
            &format!("seed-property-{}", column.key),
        )
        .await
        .unwrap();
    }
    let preview = bootstrap(&api, true).await.unwrap();
    assert!(preview.types.is_empty());
    assert!(!preview.schema_ready);
    // Every table column shows up as a property to attach, shared columns once per table.
    let expected: usize = TABLES.iter().map(|table| table.properties.len()).sum();
    assert_eq!(preview.properties.len(), expected);

    let report = bootstrap(&api, false).await.unwrap();
    assert!(report.schema_ready);
    let after = bootstrap(&api, true).await.unwrap();
    assert!(after.schema_ready);
    assert!(after.properties.is_empty());
    let file_type = api.get_type("hb_file").await.unwrap();
    let names: Vec<&str> = file_type["blocks"][0]["properties"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|property| property["property"].as_str())
        .collect();
    assert!(names.contains(&"hb_path"));
    assert!(names.contains(&"hb_sha"));
}

#[tokio::test]
async fn a_drifted_key_is_kept_in_hb_keys_json() {
    let mock = Mock::start().await;
    mock.rewrite_key("hb_sha", "hb_sha_kept");
    let api = client(&mock);
    let report = bootstrap(&api, false).await.unwrap();
    assert_eq!(
        report.keys.get("hb_sha").map(String::as_str),
        Some("hb_sha_kept")
    );
    let properties = api.list_properties().await.unwrap();
    assert!(properties.iter().any(|row| row["key"] == "hb_sha_kept"));
    assert!(!properties.iter().any(|row| row["key"] == "hb_sha"));
    let rows = api
        .search(&json!({ "type": "hb_meta", "query": "hb_meta" }))
        .await
        .unwrap();
    let stored: BTreeMap<String, String> =
        serde_json::from_str(rows[0]["properties"]["hb_keys_json"].as_str().unwrap()).unwrap();
    assert_eq!(
        stored.get("hb_sha").map(String::as_str),
        Some("hb_sha_kept")
    );
    let before = properties.len();
    let second = bootstrap(&api, false).await.unwrap();
    assert!(second.properties.is_empty());
    assert!(!second.meta);
    assert_eq!(api.list_properties().await.unwrap().len(), before);
}
