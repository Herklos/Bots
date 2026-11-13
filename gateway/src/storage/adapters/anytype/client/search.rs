//! Space search. Pagination stays in the query string (`?limit=&offset=`).
//! A `limit` member in the body is refused by the app. The walker follows `has_more`.
//!
//! The body is the caller's: `query`, `type`, `filters` or `filter`, `fields`, `sorts`.

use super::error::AnytypeError;
use super::http::{send_json, JsonCall};
use super::AnytypeClient;
use serde_json::Value;

const PAGE: u32 = 25;
const MAX_ROWS: usize = 10_000;

pub(super) struct Page {
    pub rows: Vec<Value>,
    pub has_more: bool,
}

impl AnytypeClient {
    pub(super) async fn search_page(
        &self,
        body: &Value,
        offset: u32,
        limit: u32,
    ) -> Result<Page, AnytypeError> {
        let value = self.search_page_raw(body, offset, limit).await?;
        let rows = value["data"].as_array().cloned().unwrap_or_default();
        let has_more = value["has_more"].as_bool().unwrap_or(false);
        Ok(Page { rows, has_more })
    }

    async fn search_page_raw(
        &self,
        body: &Value,
        offset: u32,
        limit: u32,
    ) -> Result<Value, AnytypeError> {
        let path = format!(
            "/v2/spaces/{}/search?offset={offset}&limit={limit}",
            self.space_id
        );
        send_json(
            self,
            JsonCall {
                method: reqwest::Method::POST,
                path: &path,
                idem_key: None,
                etag: None,
                body: Some(body),
                limited: false,
            },
        )
        .await
    }

    /// Walks `has_more` until the space search is exhausted.
    pub(crate) async fn search(&self, body: &Value) -> Result<Vec<Value>, AnytypeError> {
        let mut offset = 0_u32;
        let mut all = Vec::new();
        loop {
            let page = self.search_page(body, offset, PAGE).await?;
            let count = page.rows.len();
            all.extend(page.rows);
            if !page.has_more || count == 0 || all.len() >= MAX_ROWS {
                break;
            }
            offset = offset.saturating_add(u32::try_from(count).unwrap_or(PAGE));
        }
        Ok(all)
    }

    /// Strict pagination for typed caches: missing pagination fields, stalled
    /// pages, and exceeding the bound are errors, never an empty snapshot.
    pub(crate) async fn search_strict(&self, body: &Value) -> Result<Vec<Value>, AnytypeError> {
        let mut offset = 0_u32;
        let mut all = Vec::new();
        loop {
            let value = self.search_page_raw(body, offset, PAGE).await?;
            let page = strict_page(&value, offset)?;
            if all.len().saturating_add(page.rows.len()) > MAX_ROWS {
                return Err(AnytypeError::ApiChanged);
            }
            let count = page.rows.len();
            all.extend(page.rows);
            if !page.has_more {
                return Ok(all);
            }
            if all.len() >= MAX_ROWS {
                return Err(AnytypeError::ApiChanged);
            }
            offset = offset
                .checked_add(u32::try_from(count).map_err(|_| AnytypeError::ApiChanged)?)
                .ok_or(AnytypeError::ApiChanged)?;
        }
    }
}

fn strict_page(value: &Value, offset: u32) -> Result<Page, AnytypeError> {
    let rows = value
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .ok_or(AnytypeError::ApiChanged)?;
    let has_more = value
        .get("has_more")
        .and_then(Value::as_bool)
        .ok_or(AnytypeError::ApiChanged)?;
    if rows.len() > PAGE as usize
        || (has_more && rows.is_empty())
        || value
            .get("offset")
            .is_some_and(|v| v.as_u64() != Some(u64::from(offset)))
        || value.get("total").is_some_and(|v| {
            v.as_u64().is_none_or(|total| {
                let end = u64::from(offset) + u64::try_from(rows.len()).unwrap_or(u64::MAX);
                end > total || has_more != (end < total)
            })
        })
    {
        return Err(AnytypeError::ApiChanged);
    }
    Ok(Page { rows, has_more })
}

#[cfg(test)]
mod strict_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_ambiguous_or_stalled_pages() {
        for body in [
            json!({}),
            json!({"data":[],"has_more":true}),
            json!({"data":[],"has_more":"false"}),
            json!({"data":[],"has_more":false,"offset":9}),
            json!({"data":[],"has_more":false,"total":1}),
            json!({"data":[],"has_more":false,"total":"zero"}),
            json!({"data":vec![json!({}); 26],"has_more":false}),
        ] {
            assert!(strict_page(&body, 0).is_err());
        }
        assert!(strict_page(&json!({"data":[],"has_more":false,"offset":0}), 0).is_ok());
    }
}
