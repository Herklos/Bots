use super::mock::Mock;
use crate::storage::adapters::anytype::client::{AnytypeClient, AnytypeError};
use serde_json::json;

fn client(mock: &Mock) -> AnytypeClient {
    AnytypeClient::new(&mock.url, "k", "space-1").unwrap()
}

#[tokio::test]
async fn archive_leaves_search_and_restore_brings_it_back() {
    let mock = Mock::start().await;
    let api = client(&mock);
    let created = api
        .create_object(
            &json!({"type": "hb_file", "name": "ada/memory/MEMORY.md", "markdown": "# H\n\nbody"}),
            "create-1",
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["type"], "hb_file");
    assert!(created["etag"].is_string());
    let before = api.get_object(id).await.unwrap();
    assert_eq!(before["blocks"][0]["text"], "# H\n\nbody");
    assert!(before.get("last_modified_date").is_none());

    let archived = api.archive_object(id, "archive-1").await.unwrap();
    assert_eq!(archived["warnings"][0]["message"], "moved to the Bin");
    assert!(api
        .search(&json!({"query": "MEMORY"}))
        .await
        .unwrap()
        .is_empty());

    let fetched = api.get_object(id).await.unwrap();

    let refused = api
        .patch_object(
            id,
            &json!({"ops": [{"op": "set_properties", "set": {"hb_rev": 2}}]}),
            Some(fetched["etag"].as_str().unwrap()),
            "patch-archived",
        )
        .await
        .unwrap_err();
    assert!(matches!(refused, AnytypeError::ObjectArchived));

    api.restore_object(id, "restore-1").await.unwrap();
    assert_eq!(api.search(&json!({})).await.unwrap().len(), 1);

    api.delete_object(id, "delete-1").await.unwrap();
    assert!(matches!(
        api.get_object(id).await.unwrap_err(),
        AnytypeError::Gone
    ));
}

#[tokio::test]
async fn patch_ops_round_trip_and_a_missing_find_is_gone() {
    let mock = Mock::start().await;
    let api = client(&mock);
    let created = api
        .create_object(
            &json!({
                "type": "hb_file",
                "name": "note",
                "properties": {"hb_content": "old", "hb_sha": "aaa"},
                "markdown": "hello",
            }),
            "create-1",
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    let etag = created["etag"].as_str().unwrap();

    let patched = api
        .patch_object(
            id,
            &json!({"ops": [
                {"op": "set_properties", "set": {"hb_content": "new"}, "unset": ["hb_sha"]},
                {"op": "insert_blocks", "blocks": [{"type": "paragraph", "text": "tail"}]},
                {"op": "replace_text", "find": "hello", "replace": "world"},
            ]}),
            None,
            "patch-1",
        )
        .await
        .unwrap();
    assert_ne!(patched["etag"], etag);
    assert!(patched["created_blocks"]["0"].is_string());

    let fetched = api.get_object(id).await.unwrap();
    assert_eq!(fetched["properties"]["hb_content"], "new");
    assert!(fetched["properties"].get("hb_sha").is_none());
    assert!(fetched["markdown"].is_null());
    assert!(fetched["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|block| block["text"] == "world"));
    assert!(fetched.get("last_modified_date").is_some());

    let missing = api
        .patch_object(
            id,
            &json!({"ops": [{"op": "replace_text", "find": "absent", "replace": "x"}]}),
            Some(fetched["etag"].as_str().unwrap()),
            "patch-missing",
        )
        .await
        .unwrap_err();
    assert!(matches!(missing, AnytypeError::Gone));
}

#[tokio::test]
async fn a_stale_if_match_returns_the_current_etag_from_the_message() {
    let mock = Mock::start().await;
    let api = client(&mock);
    let created = api
        .create_object(&json!({"type": "page", "name": "Note"}), "create-1")
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    let etag = created["etag"].as_str().unwrap();
    match api
        .patch_object(
            id,
            &json!({"ops": [{"op": "set_properties", "set": {"hb_rev": 1}}]}),
            Some("00000000"),
            "stale",
        )
        .await
        .unwrap_err()
    {
        AnytypeError::EtagMismatch { current } => assert_eq!(current.as_deref(), Some(etag)),
        other => panic!("expected etag mismatch, got {other:?}"),
    }
}

#[tokio::test]
async fn a_slow_response_still_returns_the_object() {
    let mock = Mock::start().await;
    mock.set_delay(std::time::Duration::from_millis(80));
    let api = client(&mock);
    let started = std::time::Instant::now();
    let created = api
        .create_object(&json!({"type": "page", "name": "Slow"}), "slow-1")
        .await
        .unwrap();
    assert!(started.elapsed() >= std::time::Duration::from_millis(50));
    assert!(created["id"].is_string());
}
