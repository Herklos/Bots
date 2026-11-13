//! Connector persistence. Results contain local secrets for harness materialization.
//! Never send a returned record to Anytype: a remote adapter must project public
//! fields and resolve credentials through a device-local secret store.
use crate::platform::error::ApiResult;
use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub(crate) enum EmptyDocument {
    Reject,
    Allow,
}

#[async_trait]
pub(crate) trait ConnectorStore: std::fmt::Debug + Send + Sync {
    async fn list(&self, slug: &str, empty: EmptyDocument) -> ApiResult<Vec<Value>>;
    /// An absent secret preserves any locally stored credential.
    async fn put(
        &self,
        slug: &str,
        name: &str,
        url: &str,
        enabled: bool,
        secret: Option<&str>,
    ) -> ApiResult<Vec<Value>>;
    async fn delete(&self, slug: &str, name: &str) -> ApiResult<Vec<Value>>;
}
