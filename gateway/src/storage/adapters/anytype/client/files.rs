//! File upload and download. URL upload is JSON `{name, url}`. Byte upload is
//! multipart field `file`, built here so reqwest's stream feature stays off.

use super::error::AnytypeError;
use super::http::{require_idempotency, send_bytes, send_json, send_raw, JsonCall};
use super::AnytypeClient;
use serde_json::{json, Value};

const BOUNDARY: &str = "hbots-anytype-file";

impl AnytypeClient {
    pub(crate) async fn upload_url(
        &self,
        name: &str,
        url: &str,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        require_idempotency(idem_key)?;
        let path = format!("/v2/spaces/{}/files", self.space_id);
        let body = json!({ "name": name, "url": url });
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::POST,
                path: &path,
                idem_key: Some(idem_key),
                etag: None,
                body: Some(&body),
            },
        )
        .await
    }

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

    pub(crate) async fn file_meta(&self, file_id: &str) -> Result<Value, AnytypeError> {
        let path = format!("/v2/spaces/{}/files/{file_id}", self.space_id);
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::GET,
                path: &path,
                idem_key: None,
                etag: None,
                body: None,
            },
        )
        .await
    }

    pub(crate) async fn download_file(&self, file_id: &str) -> Result<Vec<u8>, AnytypeError> {
        let path = format!("/v2/spaces/{}/files/{file_id}/content", self.space_id);
        send_bytes(self, &path).await
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
    body.extend(format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{safe_name}\"\r\nContent-Type: {mime}\r\n\r\n"
    ).into_bytes());
    body.extend_from_slice(bytes);
    body.extend(format!("\r\n--{BOUNDARY}--\r\n").into_bytes());
    body
}
