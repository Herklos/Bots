//! Type and property lists, plus create. Every create takes an idempotency key.
//!
//! Measured bodies, which the caller supplies:
//! type `{name, api_key, layout, property_definitions: [{name, format}]}`,
//! property `{name, format, key}`.

use super::error::AnytypeError;
use super::http::{require_idempotency, send_json, JsonCall};
use super::AnytypeClient;
use serde_json::Value;

const PAGE: u32 = 25;
const MAX_ROWS: usize = 10_000;

impl AnytypeClient {
    pub(crate) async fn list_types(&self) -> Result<Vec<Value>, AnytypeError> {
        self.collect("types").await
    }

    pub(crate) async fn list_properties(&self) -> Result<Vec<Value>, AnytypeError> {
        self.collect("properties").await
    }

    pub(crate) async fn create_type(
        &self,
        body: &Value,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        self.create_schema("types", body, idem_key).await
    }

    /// One type with its `blocks`, so bootstrap can see the attached properties.
    pub(crate) async fn get_type(&self, key: &str) -> Result<Value, AnytypeError> {
        let path = format!("/v2/spaces/{}/types/{key}", self.space_id);
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::GET,
                path: &path,
                idem_key: None,
                etag: None,
                body: None,
                limited: false,
            },
        )
        .await
    }

    /// Replaces the type's property list. Keeps every property that is named.
    pub(crate) async fn patch_type(
        &self,
        key: &str,
        body: &Value,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        require_idempotency(idem_key)?;
        let path = format!(
            "/v2/spaces/{}/types/{key}?create_missing_options=true",
            self.space_id
        );
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::PATCH,
                path: &path,
                idem_key: Some(idem_key),
                etag: None,
                body: Some(body),
                limited: true,
            },
        )
        .await
    }

    pub(crate) async fn create_property(
        &self,
        body: &Value,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        self.create_schema("properties", body, idem_key).await
    }

    async fn create_schema(
        &self,
        kind: &str,
        body: &Value,
        idem_key: &str,
    ) -> Result<Value, AnytypeError> {
        require_idempotency(idem_key)?;
        // Select options in a type's property_definitions are refused without this.
        let path = format!(
            "/v2/spaces/{}/{kind}?create_missing_options=true",
            self.space_id
        );
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

    async fn collect(&self, kind: &str) -> Result<Vec<Value>, AnytypeError> {
        let mut offset = 0_u32;
        let mut all = Vec::new();
        loop {
            let path = format!(
                "/v2/spaces/{}/{kind}?offset={offset}&limit={PAGE}",
                self.space_id
            );
            let value = send_json(
                self,
                JsonCall {
                    method: reqwest::Method::GET,
                    path: &path,
                    idem_key: None,
                    etag: None,
                    body: None,
                    limited: false,
                },
            )
            .await?;
            let rows = value["data"].as_array().cloned().unwrap_or_default();
            let count = rows.len();
            let has_more = value["has_more"].as_bool().unwrap_or(false);
            all.extend(rows);
            if !has_more || count == 0 || all.len() >= MAX_ROWS {
                break;
            }
            offset = offset.saturating_add(u32::try_from(count).unwrap_or(PAGE));
        }
        Ok(all)
    }
}
