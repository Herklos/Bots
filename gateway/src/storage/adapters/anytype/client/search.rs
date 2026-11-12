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
        let path = format!(
            "/v2/spaces/{}/search?offset={offset}&limit={limit}",
            self.space_id
        );
        let value = send_json(
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
        .await?;
        let rows = value["data"].as_array().cloned().unwrap_or_default();
        let has_more = value["has_more"].as_bool().unwrap_or(false);
        Ok(Page { rows, has_more })
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
}
