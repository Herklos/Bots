//! File upload and download.
//!
//! Upload is `multipart/form-data` with one part named `file`. The part's filename
//! and content type are what the app reads; it drops the extension and sniffs the mime.
//! Download is `GET .../files/{id}/content`. A `Range` header returns 206.

use super::error::AnytypeError;
use super::http::{require_idempotency, send_bytes, send_raw};
use super::AnytypeClient;
use serde_json::Value;

const BOUNDARY: &str = "hbots-anytype-file";

impl AnytypeClient {
    pub(crate) async fn upload_bytes(
        &self,
        name: &str,
        mime: &str,
        bytes: &[u8],
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        require_idempotency(idem_key)?;
        let path = format!("/v2/spaces/{}/files", self.space_id);
        let body = multipart(name, mime, bytes);
        let content_type = format!("multipart/form-data; boundary={BOUNDARY}");
        send_raw(
            self,
            reqwest::Method::POST,
            &path,
            Some(idem_key),
            &content_type,
            body,
        )
        .await
    }

    pub(crate) async fn download_file(&self, file_id: &str) -> Result<Vec<u8>, AnytypeError> {
        let path = format!("/v2/spaces/{}/files/{file_id}/content", self.space_id);
        send_bytes(self, &path, None).await
    }

    /// `Range: bytes={start}-{end}`, end inclusive, as the live app answered it.
    pub(crate) async fn download_file_range(
        &self,
        file_id: &str,
        start: u64,
        end: u64,
    ) -> Result<Vec<u8>, AnytypeError> {
        let path = format!("/v2/spaces/{}/files/{file_id}/content", self.space_id);
        let range = format!("bytes={start}-{end}");
        send_bytes(self, &path, Some(&range)).await
    }
}

fn multipart(name: &str, mime: &str, bytes: &[u8]) -> Vec<u8> {
    let safe_name = name.replace('"', "");
    let mime = if mime.is_empty() {
        "application/octet-stream"
    } else {
        mime
    };
    let mut body = Vec::new();
    body.extend(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{safe_name}\"\r\nContent-Type: {mime}\r\n\r\n"
        )
        .into_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend(format!("\r\n--{BOUNDARY}--\r\n").into_bytes());
    body
}
