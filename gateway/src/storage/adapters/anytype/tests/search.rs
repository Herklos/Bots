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

    let pages = api
        .search(&json!({"type": "page", "query": "alp", "fields": ["name"]}))
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
