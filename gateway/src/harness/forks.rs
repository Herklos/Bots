//! A hidden fork of a thread, and handing that fork back to the next message.
//! The runtime's command names stay in the adapter.

use super::HarnessError;
use async_trait::async_trait;

/// Where the fork copies from. A checkpoint can be added later without a new trait.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForkPoint {
    LatestStable,
    Run(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkRequest {
    pub source_thread_id: String,
    pub target_thread_id: String,
    pub title: String,
    pub side_chat: bool,
    pub point: ForkPoint,
    pub runtime_mode: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeRequest {
    pub source_thread_id: String,
    pub target_thread_id: String,
    pub run_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideChatRef {
    pub id: String,
    pub title: String,
}

#[async_trait]
pub trait Forks: Send + Sync {
    async fn fork(&self, request: &ForkRequest) -> Result<(), HarnessError>;
    async fn merge_back(&self, request: &MergeRequest) -> Result<(), HarnessError>;
    async fn list_side_chats(
        &self,
        source_thread_id: &str,
    ) -> Result<Vec<SideChatRef>, HarnessError>;
    async fn delete_if_unused(&self, thread_id: &str) -> Result<(), HarnessError>;
}
