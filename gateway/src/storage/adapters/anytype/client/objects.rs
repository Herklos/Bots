//! Object create, read, patch, and archive. Each write takes the caller's idempotency key.
//!
//! Create uses the shortcut body `{type, name, properties, markdown}`. Patch is sent
//! through as given; the published edit shape is `{ops: [{op, ...}]}`.

use super::error::AnytypeError;
use super::http::{require_idempotency, send_json, JsonCall};
use super::AnytypeClient;
use serde_json::Value;

impl AnytypeClient {
    pub(crate) async fn get_object(&self, object_id: &str) -> Result<Value, AnytypeError> {
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::GET,
                path: &self.object_path(object_id),
                idem_key: None,
                etag: None,
                body: None,
            },
        )
        .await
    }

    pub(crate) async fn create_object(
        &self,
        body: &Value,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        require_idempotency(idem_key)?;
        let path = format!("/v2/spaces/{}/objects", self.space_id);
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::POST,
                path: &path,
                idem_key: Some(idem_key),
                etag: None,
                body: Some(body),
            },
        )
        .await
    }

    pub(crate) async fn patch_object(
        &self,
        object_id: &str,
        body: &Value,
        etag: &str,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        require_idempotency(idem_key)?;
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::PATCH,
                path: &self.object_path(object_id),
                idem_key: Some(idem_key),
                etag: Some(etag),
                body: Some(body),
            },
        )
        .await
    }

    pub(crate) async fn archive_object(
        &self,
        object_id: &str,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        require_idempotency(idem_key)?;
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::DELETE,
                path: &self.object_path(object_id),
                idem_key: Some(idem_key),
                etag: None,
                body: None,
            },
        )
        .await
    }

    fn object_path(&self, object_id: &str) -> String {
        format!("/v2/spaces/{}/objects/{object_id}", self.space_id)
    }
}
