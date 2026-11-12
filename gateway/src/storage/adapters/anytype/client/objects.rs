//! Object create, read, patch, archive, permanent delete, and restore.
//! Each write takes the caller's idempotency key.
//!
//! Create uses the shortcut body `{type, name, properties, markdown}`.
//! Patch sends `{ops: [...]}` only. `If-Match` is omitted when `etag` is `None`
//! (the live app allows that). GET's etag arrives on the `ETag` header.

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
                limited: false,
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
                limited: true,
            },
        )
        .await
    }

    pub(crate) async fn patch_object(
        &self,
        object_id: &str,
        body: &Value,
        etag: Option<&str>,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        require_idempotency(idem_key)?;
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::PATCH,
                path: &self.object_path(object_id),
                idem_key: Some(idem_key),
                etag,
                body: Some(body),
                limited: true,
            },
        )
        .await
    }

    pub(crate) async fn archive_object(
        &self,
        object_id: &str,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        self.delete(object_id, false, idem_key).await
    }

    pub(crate) async fn delete_object(
        &self,
        object_id: &str,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        self.delete(object_id, true, idem_key).await
    }

    pub(crate) async fn restore_object(
        &self,
        object_id: &str,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        require_idempotency(idem_key)?;
        let path = format!("{}/restore", self.object_path(object_id));
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::POST,
                path: &path,
                idem_key: Some(idem_key),
                etag: None,
                body: None,
                limited: true,
            },
        )
        .await
    }

    async fn delete(
        &self,
        object_id: &str,
        permanent: bool,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        require_idempotency(idem_key)?;
        let path = if permanent {
            format!("{}?permanent=true", self.object_path(object_id))
        } else {
            self.object_path(object_id)
        };
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::DELETE,
                path: &path,
                idem_key: Some(idem_key),
                etag: None,
                body: None,
                limited: true,
            },
        )
        .await
    }

    fn object_path(&self, object_id: &str) -> String {
        format!("/v2/spaces/{}/objects/{object_id}", self.space_id)
    }
}
