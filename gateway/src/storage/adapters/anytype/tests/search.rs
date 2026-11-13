use super::mock::Mock;
use crate::storage::adapters::anytype::client::AnytypeClient;
use serde_json::json;

#[tokio::test]
async fn search_filters_fields_and_walks_pages() {
    let mock = Mock::start().await;
    mock.seed_named("alpha", "page");
    mock.seed_named("beta", "task");
    mock.seed_named("alpine", "page");
    mock.seed_objects(30);
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();

    // `name` comes back on every row on its own; as a `fields` entry the live
    // app rejects it, so the query projects a real field instead.
    let pages = api
        .search(&json!({"type": "page", "query": "alp", "fields": ["created_date"]}))
        .await
        .unwrap();
    let names: Vec<_> = pages
        .iter()
        .filter_map(|row| row["name"].as_str())
        .collect();
    assert_eq!(names, vec!["alpha", "alpine"]);
    assert!(pages.iter().all(|row| row.get("markdown").is_none()));
    assert!(pages.iter().all(|row| row.get("id").is_some()));

    let all = api.search(&json!({})).await.unwrap();
    assert_eq!(all.len(), 33);

    mock.seed_types(30);
    let types = api.list_types().await.unwrap();
    assert_eq!(types.len(), 30);
    assert_eq!(mock.type_count(), 30);
}

#[tokio::test]
async fn filters_sorts_and_a_limit_in_the_body_is_refused() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let small = api
        .create_object(
            &json!({"type": "m_file", "name": "small", "properties": {"m_size": 1}}),
            "small",
        )
        .await
        .unwrap();
    api.create_object(
        &json!({"type": "m_file", "name": "large", "properties": {"m_size": 5}}),
        "large",
    )
    .await
    .unwrap();

    let greater = api
        .search(&json!({
            "type": "m_file",
            "filters": [{"property": "m_size", "condition": "greater", "value": 2}],
        }))
        .await
        .unwrap();
    assert_eq!(greater.len(), 1);
    assert_eq!(greater[0]["name"], "large");

    let sorted = api
        .search(&json!({
            "sorts": [{"property": "m_size", "direction": "desc"}],
        }))
        .await
        .unwrap();
    assert_eq!(sorted[0]["name"], "large");

    api.patch_object(
        small["id"].as_str().unwrap(),
        &json!({"ops": [{"op": "set_properties", "set": {"m_rev": 1}}]}),
        None,
        "touch",
    )
    .await
    .unwrap();
    let day = api
        .search(&json!({"filter": "last_modified_date >= \"2026-11-12\""}))
        .await
        .unwrap();
    assert_eq!(day.len(), 1);
    assert_eq!(day[0]["name"], "small");

    let stamped = api
        .search(&json!({
            "filters": [{"property": "last_modified_date", "condition": "greater", "value": 1794502324}],
        }))
        .await
        .unwrap();
    assert!(stamped.is_empty());

    let refused = api.search(&json!({"limit": 25})).await.unwrap_err();
    match refused {
        crate::storage::adapters::anytype::client::AnytypeError::Bad { status, message } => {
            assert_eq!(status, 400);
            assert!(message.contains("?limit=25"));
        }
        other => panic!("expected a bad response, got {other:?}"),
    }
}
