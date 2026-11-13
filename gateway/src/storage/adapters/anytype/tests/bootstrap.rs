use super::mock::Mock;
use crate::storage::adapters::anytype::client::{AnytypeClient, AnytypeError};
use crate::storage::adapters::anytype::schema::setup_failure;
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
    assert!(table::option_allowed("m_conflict_state", "open"));
    assert!(!table::option_allowed("m_conflict_state", "later"));
    assert!(!table::option_allowed("m_sha", "open"));
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
        .search(&json!({ "type": "m_meta", "query": "m_meta" }))
        .await
        .unwrap();
    let id = rows[0]["id"].as_str().unwrap();
    let fetched = api.get_object(id).await.unwrap();
    api.patch_object(
        id,
        &json!({ "ops": [{ "op": "set_properties", "set": { "m_note": "extra" } }] }),
        Some(fetched["etag"].as_str().unwrap()),
        "note-1",
    )
    .await
    .unwrap();
    let rows = api
        .search(&json!({ "type": "m_meta", "query": "m_meta" }))
        .await
        .unwrap();
    let fetched = api.get_object(rows[0]["id"].as_str().unwrap()).await.unwrap();
    let meta = read_meta(&fetched, &BTreeMap::new());
    assert_eq!(meta.version, Some(1));
    assert_eq!(meta.min_gateway.as_deref(), Some(env!("CARGO_PKG_VERSION")));
    assert!(meta.keys.is_empty());
    assert!(schema_ready(&api).await.unwrap());
    let state = api.list_properties().await.unwrap();
    let conflict = state
        .iter()
        .find(|row| row["key"] == "m_conflict_state")
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
        if column.key == "m_mime" {
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
    assert_eq!(report.properties, vec!["m_mime".to_string()]);
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
    let file_type = api.get_type("m_file").await.unwrap();
    let names: Vec<&str> = file_type["blocks"][0]["properties"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|property| property["property"].as_str())
        .collect();
    assert!(names.contains(&"m_path"));
    assert!(names.contains(&"m_sha"));
}

#[tokio::test]
async fn a_drifted_key_is_kept_in_m_keys_json() {
    let mock = Mock::start().await;
    mock.rewrite_key("m_sha", "m_sha_kept");
    let api = client(&mock);
    let report = bootstrap(&api, false).await.unwrap();
    assert_eq!(
        report.keys.get("m_sha").map(String::as_str),
        Some("m_sha_kept")
    );
    let properties = api.list_properties().await.unwrap();
    assert!(properties.iter().any(|row| row["key"] == "m_sha_kept"));
    assert!(!properties.iter().any(|row| row["key"] == "m_sha"));
    let rows = api
        .search(&json!({ "type": "m_meta", "query": "m_meta" }))
        .await
        .unwrap();
    let fetched = api.get_object(rows[0]["id"].as_str().unwrap()).await.unwrap();
    let stored: BTreeMap<String, String> =
        serde_json::from_str(fetched["properties"]["m_keys_json"].as_str().unwrap()).unwrap();
    assert_eq!(
        stored.get("m_sha").map(String::as_str),
        Some("m_sha_kept")
    );
    let before = properties.len();
    let second = bootstrap(&api, false).await.unwrap();
    assert!(second.properties.is_empty());
    assert!(!second.meta);
    assert_eq!(api.list_properties().await.unwrap().len(), before);
}

#[tokio::test]
async fn a_fresh_space_never_searches_by_unknown_keys() {
    // The live app answers 400 for a `type` or `fields` key it has not heard of.
    let mock = Mock::start().await;
    let api = client(&mock);
    let unknown_type = api
        .search(&json!({ "type": "m_meta", "query": "m_meta" }))
        .await
        .unwrap_err();
    assert!(matches!(
        unknown_type,
        AnytypeError::Bad { status: 400, .. }
    ));
    let unknown_field = api
        .search(&json!({ "query": "m_meta", "fields": ["m_schema_version"] }))
        .await
        .unwrap_err();
    assert!(matches!(
        unknown_field,
        AnytypeError::Bad { status: 400, .. }
    ));
    let report = bootstrap(&api, false).await.unwrap();
    assert!(report.schema_ready);
}

#[tokio::test]
async fn the_type_body_uses_api_key_and_the_live_field_set() {
    let mock = Mock::start().await;
    let api = client(&mock);
    let bad = api
        .create_type(&json!({ "name": "x", "key": "m_x" }), "t-1")
        .await
        .unwrap_err();
    assert!(matches!(bad, AnytypeError::Bad { status: 400, .. }));
    let bad = api
        .create_property(&json!({ "name": "x" }), "p-1")
        .await
        .unwrap_err();
    assert!(matches!(bad, AnytypeError::Bad { status: 400, .. }));
}

#[tokio::test]
async fn properties_exist_before_the_types_that_link_them() {
    let mock = Mock::start().await;
    let api = client(&mock);
    bootstrap(&api, false).await.unwrap();
    // Linked, not minted twice: one property row per declared column.
    assert_eq!(api.list_properties().await.unwrap().len(), COLUMNS.len());
}

#[test]
fn setup_failure_keeps_the_reason_and_drops_the_space_id() {
    let id = "bafyreig54easpaerpi3ztgragrlxh2utaisv5cdboakfqhlmndt3tzk2va.39bilhfrc4c6s";
    let bad = AnytypeError::Bad {
        status: 400,
        message: format!("type \"m_meta\" not found in space {id}"),
    };
    let (status, text) = setup_failure(&bad);
    assert_eq!(status, 502);
    assert!(text.starts_with("Anytype could not prepare the space."));
    assert!(text.contains("m_meta"));
    assert!(!text.contains("bafy"));
    assert_eq!(
        setup_failure(&AnytypeError::Unreachable).1,
        "Anytype is not reachable. Open the Anytype app and check the host."
    );
    assert_eq!(setup_failure(&AnytypeError::NotGranted).0, 403);
    let busy = AnytypeError::RateLimited {
        retry_after: std::time::Duration::ZERO,
    };
    assert_eq!(
        setup_failure(&busy),
        (429, "Anytype is busy. Try again.".into())
    );
}
