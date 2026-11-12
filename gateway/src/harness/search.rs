//! A search across conversation text.
//!
//! The runtime that has the command returns one page of matches. A runtime
//! that does not returns [`HarnessError::Unsupported`] and sends nothing.

use crate::harness::HarnessError;
use async_trait::async_trait;

/// One hit from a conversation search. Older servers omit `quality`, `hits`,
/// and `message_id`.
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadHit {
    pub thread_id: String,
    pub project_id: String,
    pub source: String,
    pub snippet: String,
    pub message_created_at: Option<String>,
    pub quality: Option<f64>,
    pub hits: Option<u64>,
    pub message_id: Option<String>,
}

#[async_trait]
pub trait ThreadSearch: Send + Sync {
    /// `query` is already trimmed. `limit` is between 1 and 50.
    async fn search_threads(&self, query: &str, limit: u32)
        -> Result<Vec<ThreadHit>, HarnessError>;
}
