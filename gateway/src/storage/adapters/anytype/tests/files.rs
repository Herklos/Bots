use super::mock::Mock;
use crate::storage::adapters::anytype::client::AnytypeClient;
use sha2::{Digest, Sha256};

#[tokio::test]
async fn upload_keeps_the_measured_name_and_mime_and_replays() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let uploaded = api
        .upload_bytes("notes.bin", "text/markdown", b"original", "file-1")
        .await
        .unwrap();
    let id = uploaded["id"].as_str().unwrap();
    assert_eq!(uploaded["name"], "notes");
    assert_eq!(uploaded["mime_type"], "text/plain; charset=utf-8");
    assert!(uploaded.get("sha").is_none());

    let again = api
        .upload_bytes("notes.bin", "text/markdown", b"original", "file-1")
        .await
        .unwrap();
    assert_eq!(again["id"], id);

    let conflict = api
        .upload_bytes("notes.bin", "text/markdown", b"other", "file-1")
        .await
        .unwrap_err();
    assert!(matches!(
        conflict,
        crate::storage::adapters::anytype::client::AnytypeError::Bad { status: 409, .. }
    ));

    let binary = api
        .upload_bytes("data.bin", "application/octet-stream", &[0, 1, 2], "file-2")
        .await
        .unwrap();
    assert_eq!(binary["name"], "data");
    assert_eq!(binary["mime_type"], "application/octet-stream");

    let bytes = api.download_file(id).await.unwrap();
    assert_eq!(bytes, b"original");

    let response = reqwest::Client::new()
        .get(format!("{}/v2/spaces/space-1/files/{id}/content", mock.url))
        .send()
        .await
        .unwrap();
    let etag = hex::encode(Sha256::digest(b"original"));
    assert_eq!(
        response.headers().get("etag").unwrap().to_str().unwrap(),
        etag
    );
    assert_eq!(response.headers().get("accept-ranges").unwrap(), "bytes");

    let head = reqwest::Client::new()
        .head(format!("{}/v2/spaces/space-1/files/{id}/content", mock.url))
        .send()
        .await
        .unwrap();
    assert_eq!(head.status(), 200);
    assert_eq!(head.headers().get("etag").unwrap().to_str().unwrap(), etag);
}

#[tokio::test]
async fn a_ranged_download_returns_the_slice() {
    let mock = Mock::start().await;
    let api = AnytypeClient::new(&mock.url, "k", "space-1").unwrap();
    let payload: Vec<u8> = (0..1024)
        .map(|index| u8::try_from(index % 251).unwrap_or(0))
        .collect();
    let uploaded = api
        .upload_bytes("blob.bin", "application/octet-stream", &payload, "range-1")
        .await
        .unwrap();
    let id = uploaded["id"].as_str().unwrap();
    let slice = api.download_file_range(id, 256, 511).await.unwrap();
    assert_eq!(slice, payload[256..=511]);
}
