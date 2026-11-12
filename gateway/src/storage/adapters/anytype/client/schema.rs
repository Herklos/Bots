//! Type and property lists, plus create. Create bodies are the short form
//! `{name, key, layout}` and `{name, key, format}`: the spike did not record the full schema.

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
        let path = format!("/v2/spaces/{}/{kind}", self.space_id);
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
