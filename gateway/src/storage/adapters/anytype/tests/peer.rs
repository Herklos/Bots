use super::mock::Mock;
use crate::storage::adapters::anytype::client::AnytypeClient;
use serde_json::json;

#[tokio::test]
async fn a_peer_write_is_last_writer_wins_per_property() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let created = api
        .create_object(
            &json!({
                "type": "m_file",
                "name": "ada/memory/MEMORY.md",
                "properties": {"m_content": "old", "m_sha": "aaa"},
            }),
            "create-1",
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    assert!(mock.peer_write(id, json!({"m_content": "one"}), false));
    assert!(mock.peer_write(id, json!({"m_sha": "bbb"}), false));
    let doc = api.get_object(id).await.unwrap();
    assert_eq!(doc["properties"]["m_content"], "one");
    assert_eq!(doc["properties"]["m_sha"], "bbb");
    assert!(doc.get("last_modified_date").is_some());
}

#[tokio::test]
async fn a_torn_peer_write_keeps_the_old_sha() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let created = api
        .create_object(
            &json!({
                "type": "m_file",
                "name": "ada/memory/MEMORY.md",
                "properties": {"m_content": "old", "m_sha": "aaa"},
            }),
            "create-1",
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    assert!(mock.peer_write(id, json!({"m_content": "torn", "m_sha": "ccc"}), true));
    let doc = api.get_object(id).await.unwrap();
    assert_eq!(doc["properties"]["m_content"], "torn");
    assert_eq!(doc["properties"]["m_sha"], "aaa");
}
