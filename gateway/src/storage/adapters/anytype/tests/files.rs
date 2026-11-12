use super::mock::Mock;
use crate::storage::adapters::anytype::client::AnytypeClient;
use sha2::{Digest, Sha256};

#[tokio::test]
async fn a_peer_can_tear_a_file_write() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let uploaded = api
        .upload_bytes("notes.txt", "text/plain", b"original", "file-1")
        .await
        .unwrap();
    let id = uploaded["id"].as_str().unwrap();
    let again = api
        .upload_bytes("notes.txt", "text/plain", b"original", "file-1")
        .await
        .unwrap();
    assert_eq!(again["id"], id);

    assert!(mock.tear_file(id));
    let meta = api.file_meta(id).await.unwrap();
    let bytes = api.download_file(id).await.unwrap();
    assert_eq!(bytes, b"torn-by-peer");
    let live = hex::encode(Sha256::digest(&bytes));
    assert_eq!(meta["sha"], uploaded["sha"]);
    assert_ne!(meta["sha"], live);
}

#[tokio::test]
async fn a_slow_response_still_returns_the_object() {
    let mock = Mock::start().await;
    mock.set_delay(std::time::Duration::from_millis(80));
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let started = std::time::Instant::now();
    let created = api
        .upload_url("remote.txt", "https://example.test/remote.txt", "url-1")
        .await
        .unwrap();
    assert!(started.elapsed() >= std::time::Duration::from_millis(50));
    assert!(created["id"].is_string());
}
