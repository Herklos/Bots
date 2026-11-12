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
mod t3;
#[cfg(test)]
mod tests;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use hold::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use prompts::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use t3::*;

const LIVE: &[&str] = &[
    "running",
    "in_progress",
    "pending",
    "queued",
    "streaming",
    "starting",
];

/// Bots by main thread: `{slug, name, color, shape}`. It turns a thread id in a tool call or a
/// sender into the Bot the person knows.
pub(crate) type Peers = HashMap<String, Value>;
