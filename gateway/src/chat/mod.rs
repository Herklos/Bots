//! A small REST face on a Bot's main thread for clients that don't speak the harness RPC,
//! such as the Exact app's `app.ts`: read the conversation, send a message, stop a run, answer
//! an approval. T3 Code is the harness wired today; other backends may be added. The `/t3/*`
//! passthrough is that backend's door.

use serde_json::{json, Value};
use sha2::Digest;
use std::{
    collections::HashMap,
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::Notify;

mod hold;
mod prompts;
#[cfg(test)]
mod tests;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use hold::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use prompts::*;

/// Bots by main thread: `{slug, name, color, shape}`. It turns a thread id in a tool call or a
/// sender into the Bot the person knows.
pub(crate) type Peers = HashMap<String, Value>;

pub(crate) mod approvals; // T5: answering a pending request.
pub(crate) mod attachments; // Chat files stored on the Bot.
pub(crate) mod code; // Gate for code work. Off unless beta.code is on.
pub(crate) mod images; // Library pictures named in a Bot reply.
pub(crate) mod rooms; // Local rooms. A send is stored here, then dispatched to each member's main thread.
pub(crate) mod routines;
pub(crate) mod thread_meta; // T5: reactions, voice calls, roster previews. // T5: T3 scheduled tasks.
