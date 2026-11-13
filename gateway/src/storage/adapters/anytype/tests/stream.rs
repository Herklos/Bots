//! Search-stream client against the mock. No live Anytype and no `HBOTS_ANYTYPE_*`.

use super::mock::Mock;
use crate::storage::adapters::anytype::client::{AnytypeClient, AnytypeError, StreamEvent};
use serde_json::json;
use std::time::Duration;

fn client(mock: &Mock) -> AnytypeClient {
    AnytypeClient::new(&mock.url, "k", "space").unwrap()
}

async fn take(stream: &mut crate::storage::adapters::anytype::client::FileStream) -> StreamEvent {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(3), stream.next_event())
            .await
            .expect("timed out reading the search stream")
            .expect("stream error")
            .expect("stream ended");
        if !matches!(event, StreamEvent::Heartbeat) {
            return event;
        }
    }
}

#[tokio::test]
async fn the_stream_sends_a_snapshot_then_live_changes_without_content() {
    let mock = Mock::start().await;
    let api = client(&mock);
    let created = api
        .create_object(
            &json!({
                "type": "m_file",
                "name": "ada/memory/note.md",
                "properties": {
                    "m_path": "ada/memory/note.md",
                    "m_sha": "sha-1",
                    "m_deleted": false,
                    "m_device": "other",
                    "m_rev": 1,
                    "m_size": 3,
                    "m_content": "secret-content",
                    "m_blob": ["blob-1"],
                }
            }),
            "create-1",
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    let mut stream = api.open_file_stream(Duration::from_secs(1)).await.unwrap();

    let added = take(&mut stream).await;
    let StreamEvent::Added(row) = added else {
        panic!("expected object_added, got {added:?}");
    };
    assert_eq!(row["id"], id);
    assert_eq!(row["properties"]["m_path"], "ada/memory/note.md");
    assert_eq!(row["properties"]["m_sha"], "sha-1");
    assert!(row["properties"].get("m_content").is_none());
    assert!(row["properties"].get("m_blob").is_none());
    assert!(row.get("m_content").is_none());
    assert!(matches!(
        take(&mut stream).await,
        StreamEvent::SnapshotComplete
    ));

    let body = mock.last_stream_body();
    assert!(body.get("query").is_none());
    let fields: Vec<_> = body["fields"]
        .as_array()
        .expect("fields")
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert_eq!(
        fields,
        [
            "m_path",
            "m_sha",
            "m_deleted",
            "m_device",
            "m_rev",
            "last_modified_date",
            "m_size",
        ]
    );

    let beat = tokio::time::timeout(Duration::from_secs(2), stream.next_event())
        .await
        .expect("heartbeat")
        .expect("stream error")
        .expect("stream ended");
    assert_eq!(beat, StreamEvent::Heartbeat);

    assert!(mock.peer_write(
        &id,
        json!({"m_sha": "sha-2", "m_content": "still-secret"}),
        false,
    ));
    let updated = take(&mut stream).await;
    let StreamEvent::Updated(row) = updated else {
        panic!("expected object_updated, got {updated:?}");
    };
    assert_eq!(row["properties"]["m_sha"], "sha-2");
    assert!(row["properties"].get("m_content").is_none());

    api.archive_object(&id, "archive-1").await.unwrap();
    assert_eq!(
        take(&mut stream).await,
        StreamEvent::Removed { id: id.clone() }
    );

    mock.drop_streams();
    let ended = tokio::time::timeout(Duration::from_secs(2), stream.next_event())
        .await
        .expect("drop")
        .expect("stream error");
    assert!(ended.is_none());
}

#[tokio::test]
async fn a_stream_cap_is_rate_limited_and_does_not_shrink_writes() {
    let mock = Mock::start().await;
    mock.set_stream_cap(0);
    let api = client(&mock);
    let before = api.rate();
    let opened = api.open_file_stream(Duration::from_secs(15)).await;
    let Err(error) = opened else {
        panic!("expected the stream cap to refuse the connection");
    };
    assert!(matches!(error, AnytypeError::RateLimited { .. }));
    assert_eq!(api.rate(), before);
}
