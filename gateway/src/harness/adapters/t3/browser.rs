use super::T3;
use crate::harness::{Browser, HarnessError, Relay};
use async_trait::async_trait;
use serde_json::{json, Value};

fn failed(err: impl ToString) -> HarnessError {
    HarnessError::failed(err.to_string())
}

#[async_trait]
impl Browser for T3 {
    async fn pages(&self, thread_id: &str) -> Result<Vec<Value>, HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        let listed = rpc
            .call("preview.list", json!({ "threadId": thread_id }))
            .await
            .map_err(HarnessError::failed)?;
        Ok(super::stream::tabs_for_screen(&listed))
    }

    async fn frame(
        &self,
        thread_id: &str,
        tab_id: &str,
        large: bool,
    ) -> Result<Vec<u8>, HarnessError> {
        let size = if large {
            super::stream::WatchSize::Large
        } else {
            super::stream::WatchSize::Card
        };
        super::stream::one_frame(self, thread_id, tab_id, size)
            .await
            .map_err(failed)
    }

    async fn take(
        &self,
        thread_id: &str,
        slug: &str,
        tab_id: &str,
    ) -> Result<Vec<u8>, HarnessError> {
        super::hold::take(self, thread_id, slug, tab_id)
            .await
            .map_err(failed)
    }

    async fn control(
        &self,
        thread_id: &str,
        tab_id: &str,
        action: &str,
    ) -> Result<Vec<u8>, HarnessError> {
        super::stream::send_control(self, thread_id, tab_id, action)
            .await
            .map_err(failed)
    }

    async fn input(
        &self,
        thread_id: &str,
        tab_id: &str,
        messages: &[Value],
    ) -> Result<crate::harness::Frame, HarnessError> {
        let driven = super::stream::send_input(self, thread_id, tab_id, messages)
            .await
            .map_err(failed)?;
        Ok(crate::harness::Frame {
            bytes: driven.image,
            clipboard: driven.clipboard,
        })
    }

    async fn held_input(&self, slug: &str, tab_id: &str, messages: &[Value]) -> Option<Vec<u8>> {
        super::hold::input(slug, tab_id, messages).await
    }

    async fn release(&self, slug: &str) -> bool {
        super::hold::release(slug).await
    }

    async fn edit(
        &self,
        thread_id: &str,
        tab_id: &str,
        kind: &str,
    ) -> Result<crate::harness::Frame, HarnessError> {
        let driven = super::stream::send_edit(self, thread_id, tab_id, kind)
            .await
            .map_err(failed)?;
        Ok(crate::harness::Frame {
            bytes: driven.image,
            clipboard: driven.clipboard,
        })
    }

    async fn tabs(&self, thread_id: &str) -> Result<Vec<crate::harness::BrowserTab>, String> {
        Ok(super::fill::list_tabs(self, thread_id)
            .await?
            .into_iter()
            .map(|tab| crate::harness::BrowserTab {
                id: tab.tab_id,
                url: tab.url,
            })
            .collect())
    }

    async fn type_text(&self, thread_id: &str, tab_id: &str, text: &str) -> Result<(), String> {
        super::fill::type_into_tab(self, thread_id, tab_id, text).await
    }
}

#[async_trait]
impl Relay for T3 {
    fn http_base(&self) -> &str {
        &self.base_url
    }

    fn ws_base(&self) -> String {
        super::ws_base(&self.base_url)
    }

    async fn proxy_http(
        &self,
        method: axum::http::Method,
        path_and_query: &str,
        headers: &axum::http::HeaderMap,
        body: axum::body::Bytes,
    ) -> axum::response::Response {
        super::proxy_http(self, method, path_and_query, headers, body).await
    }

    async fn proxy_ws(
        &self,
        client: axum::extract::ws::WebSocket,
        upstream_url: String,
        close_sockets: std::sync::Arc<tokio::sync::Notify>,
    ) {
        super::proxy_ws(client, upstream_url, self.token.clone(), close_sockets).await;
    }
}
