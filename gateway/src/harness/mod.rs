//! The agent runtime the gateway talks to.
//!
//! Each capability is its own trait. [`Harness`] only groups them. A runtime that
//! lacks one returns [`HarnessError::Unsupported`] instead of a pretend success.
//! `adapters` holds the implementations. T3 Code is the one the gateway runs.

mod error;
mod forks;
mod snapshot;

pub(crate) mod adapters;
pub(crate) mod providers;

pub(crate) use forks::{ForkPoint, ForkRequest, Forks, MergeRequest, SideChatRef};

pub(crate) use adapters::t3;
pub(crate) use error::HarnessError;
pub(crate) use snapshot::{
    message_id, BrowserTab, ComputerCatalog, ComputerLink, Frame, ProvisionedSend, ShellSnapshot,
    ShellThread, ThreadSnapshot, UserMessage,
};

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

#[async_trait]
pub trait Transcript: Send + Sync {
    async fn read(&self, thread_id: &str) -> Result<ThreadSnapshot, HarnessError>;
    /// Inbox facts. Only a `snapshot` item counts; a gate that asks for HTTP is an error.
    async fn activity(&self) -> Result<ShellSnapshot, HarnessError>;
    /// Threads a restart would interrupt. An HTTP gate is reported on the snapshot.
    async fn busy(&self) -> Result<ShellSnapshot, HarnessError>;
}

#[async_trait]
pub trait Turns: Send + Sync {
    /// A person's message, including a room wake and a voice clip. Returns the message id.
    async fn send_user(&self, message: &UserMessage) -> Result<(), HarnessError>;
    async fn interrupt(&self, thread_id: &str, run_id: &str) -> Result<(), HarnessError>;
    /// Computer open or close. Returns the message id the command used.
    async fn steer(&self, thread_id: &str, text: &str) -> Result<String, HarnessError>;
    /// Set an existing thread's approval mode before the next turn.
    async fn set_runtime_mode(&self, thread_id: &str, mode: &str) -> Result<(), HarnessError>;
}

#[async_trait]
pub trait Previews: Send + Sync {
    async fn inbox(
        &self,
        thread_id: &str,
        names: &HashMap<String, String>,
    ) -> Result<Option<Value>, HarnessError>;
}

/// Snapshots from a staying-open routine list. The channel closes when the connection drops.
pub(crate) type RoutineFeed = tokio::sync::mpsc::Receiver<Result<Value, HarnessError>>;

#[async_trait]
pub trait Routines: Send + Sync {
    async fn list(&self) -> Result<Value, HarnessError>;
    async fn upsert(&self, task: &Value) -> Result<Value, HarnessError>;
    async fn set_enabled(&self, body: &Value) -> Result<Value, HarnessError>;
    async fn delete(&self, body: &Value) -> Result<Value, HarnessError>;
    async fn run_now(&self, body: &Value) -> Result<Value, HarnessError>;
    /// Stay on the task list until the connection drops. Each item is one full snapshot.
    async fn watch(&self) -> Result<RoutineFeed, HarnessError>;
}

/// An answer the person gave to a pending request. The secret, when there is one, stays here.
#[derive(Clone, Debug)]
pub enum Approval {
    Secret {
        thread_id: String,
        turn_item_id: String,
        /// `None` declines.
        secret: Option<String>,
    },
    Request {
        thread_id: String,
        request_id: String,
        decision: Option<String>,
        answers: Option<Value>,
        feedback: Option<String>,
    },
}

#[async_trait]
pub trait Approvals: Send + Sync {
    async fn answer(&self, approval: &Approval) -> Result<Value, HarnessError>;
}

#[derive(Clone, Debug)]
pub struct ProjectInfo {
    pub id: String,
    pub workspace_root: String,
    pub title: String,
    pub favicon_path: String,
    pub project_icon_null: bool,
}

#[derive(Clone, Debug)]
pub struct LaunchThread {
    pub thread_id: String,
    pub project_id: String,
    pub title: String,
    pub model: Value,
    pub runtime_mode: String,
}

#[async_trait]
pub trait Provisioning: Send + Sync {
    async fn projects(&self) -> Result<Vec<ProjectInfo>, HarnessError>;
    async fn create_project(
        &self,
        project_id: &str,
        title: &str,
        workspace_root: &str,
        model: &Value,
    ) -> Result<(), HarnessError>;
    async fn update_face(
        &self,
        project_id: &str,
        name: &str,
        favicon: &str,
    ) -> Result<(), HarnessError>;
    async fn launch(&self, spec: &LaunchThread) -> Result<(), HarnessError>;
    async fn send_provisioned(
        &self,
        thread_id: &str,
        text: &str,
        kind: ProvisionedSend,
    ) -> Result<(), HarnessError>;
    async fn set_model(
        &self,
        project_id: Option<&str>,
        thread_id: Option<&str>,
        selection: &Value,
    ) -> Result<(), HarnessError>;
    async fn provider_config(&self) -> Result<Value, HarnessError>;
    /// This computer and the links already on it. A down runtime is `ok: false`, not an error.
    async fn computers(&self) -> Result<ComputerCatalog, HarnessError>;
}

#[async_trait]
pub trait Browser: Send + Sync {
    async fn pages(&self, thread_id: &str) -> Result<Vec<Value>, HarnessError>;
    async fn frame(
        &self,
        thread_id: &str,
        tab_id: &str,
        large: bool,
    ) -> Result<Vec<u8>, HarnessError>;
    async fn take(
        &self,
        thread_id: &str,
        slug: &str,
        tab_id: &str,
    ) -> Result<Vec<u8>, HarnessError>;
    async fn control(
        &self,
        thread_id: &str,
        tab_id: &str,
        action: &str,
    ) -> Result<Vec<u8>, HarnessError>;
    async fn input(
        &self,
        thread_id: &str,
        tab_id: &str,
        messages: &[Value],
    ) -> Result<Frame, HarnessError>;
    /// Input on the socket this viewer already holds. `None` means there is no live one.
    async fn held_input(&self, slug: &str, tab_id: &str, messages: &[Value]) -> Option<Vec<u8>>;
    /// Newest JPEG on the socket this viewer holds. `None` when it holds none.
    async fn held_frame(&self, slug: &str) -> Option<Vec<u8>>;
    /// Close the held socket. False when this viewer was not holding one.
    async fn release(&self, slug: &str) -> bool;
    async fn edit(&self, thread_id: &str, tab_id: &str, kind: &str) -> Result<Frame, HarnessError>;
    async fn tabs(&self, thread_id: &str) -> Result<Vec<BrowserTab>, String>;
    async fn type_text(&self, thread_id: &str, tab_id: &str, text: &str) -> Result<(), String>;
}

#[async_trait]
pub trait Relay: Send + Sync {
    fn http_base(&self) -> &str;
    /// The websocket origin for this runtime's HTTP base.
    fn ws_base(&self) -> String;
    async fn proxy_http(
        &self,
        method: axum::http::Method,
        path_and_query: &str,
        headers: &axum::http::HeaderMap,
        body: axum::body::Bytes,
    ) -> axum::response::Response;
    async fn proxy_ws(
        &self,
        client: axum::extract::ws::WebSocket,
        upstream_url: String,
        close_sockets: std::sync::Arc<tokio::sync::Notify>,
    );
}

#[async_trait]
pub trait Harness: std::fmt::Debug + Send + Sync {
    async fn connected(&self) -> bool;
    fn transcript(&self) -> Result<&dyn Transcript, HarnessError>;
    fn turns(&self) -> Result<&dyn Turns, HarnessError>;
    fn forks(&self) -> Result<&dyn Forks, HarnessError>;
    fn previews(&self) -> Result<&dyn Previews, HarnessError>;
    fn routines(&self) -> Result<&dyn Routines, HarnessError>;
    fn approvals(&self) -> Result<&dyn Approvals, HarnessError>;
    fn provisioning(&self) -> Result<&dyn Provisioning, HarnessError>;
    fn browser(&self) -> Result<&dyn Browser, HarnessError>;
    fn relay(&self) -> Result<&dyn Relay, HarnessError>;
    /// Whether a folder is a repository. A runtime that cannot answer returns
    /// [`HarnessError::Unsupported`] and sends no request.
    fn repos(&self) -> Result<&dyn Repos, HarnessError>;
}

/// Whether a workspace folder is a repository. One question, no write.
#[async_trait]
pub trait Repos: Send + Sync {
    async fn is_repo(&self, cwd: &str) -> Result<bool, HarnessError>;
}

pub(crate) fn connect(base_url: String, token: String) -> std::sync::Arc<dyn Harness> {
    std::sync::Arc::new(t3::T3::new(base_url, token))
}
