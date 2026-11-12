use super::mock::Mock;
use crate::storage::adapters::anytype::client::{AnytypeClient, AnytypeError};
use serde_json::json;
use std::time::Duration;

fn client(mock: &Mock) -> AnytypeClient {
    AnytypeClient::new(&mock.url, "test-key", "space-1").unwrap()
}

#[tokio::test]
async fn each_failure_maps_to_its_error() {
    let mock = Mock::start().await;
    let api = client(&mock);

    let down = AnytypeClient::new("http://127.0.0.1:9", "k", "s").unwrap();
    assert!(matches!(
        down.get_object("missing").await.unwrap_err(),
        AnytypeError::Unreachable
    ));

    mock.set_not_granted(true);
    assert!(matches!(
        api.get_object("obj").await.unwrap_err(),
        AnytypeError::NotGranted
    ));
    mock.set_not_granted(false);

    mock.set_status(Some(404));
    assert!(matches!(
        api.get_object("obj").await.unwrap_err(),
        AnytypeError::Gone
    ));
    mock.set_status(Some(410));
    assert!(matches!(
        api.get_object("obj").await.unwrap_err(),
        AnytypeError::Gone
    ));
    mock.set_status(None);

    mock.set_malformed(true);
    assert!(matches!(
        api.get_object("obj").await.unwrap_err(),
        AnytypeError::ApiChanged
    ));
    mock.set_malformed(false);

    mock.set_rate_limit(1, 2);
    match api.get_object("obj").await.unwrap_err() {
        AnytypeError::RateLimited { retry_after } => {
            assert_eq!(retry_after, Duration::from_secs(2))
        }
        other => panic!("expected rate limit, got {other:?}"),
    }

    let created = api
        .create_object(&json!({"type": "page", "name": "Note"}), "create-1")
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    let etag = created["etag"].as_str().unwrap();
    mock.set_etag_mismatch(true);
    match api
        .patch_object(
            id,
            &json!({"ops": [{"op": "set_properties", "set": {"name": "Next"}}]}),
            Some(etag),
            "patch-1",
        )
        .await
        .unwrap_err()
    {
        AnytypeError::EtagMismatch { current } => assert_eq!(current.as_deref(), Some(etag)),
        other => panic!("expected etag mismatch, got {other:?}"),
    }
    mock.set_etag_mismatch(false);

    mock.set_archived_writes(true);
    assert!(matches!(
        api.archive_object(id, "archive-1").await.unwrap_err(),
        AnytypeError::ObjectArchived
    ));
    mock.set_archived_writes(false);

    mock.set_status(Some(500));
    match api.get_object(id).await.unwrap_err() {
        AnytypeError::Bad { status, .. } => assert_eq!(status, 500),
        other => panic!("expected a bad response, got {other:?}"),
    }
}
