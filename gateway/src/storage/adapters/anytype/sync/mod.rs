//! Anytype sync. The decision table is local. Listing, apply, and the engine talk to the client.
//! `runtime` is the long-lived task the gateway runs (M5); the rest is the engine itself.

// Engine seams the M4 tests drive directly (fault injection, hooks, confirm_import),
// or later milestones pick up (M6 stream, M7 pruning). The runtime keeps the rest live.
#![allow(
    dead_code,
    reason = "engine seams the sync tests and later milestones use"
)]

mod apply;
mod conflict;
mod decide;
mod device;
mod engine;
mod error;
mod history;
mod hook;
mod journal;
mod manifest;
mod merge;
mod outbox;
mod payload;
mod plan;
mod policy;
mod push;
mod remote;
mod runtime;
mod scan;
mod status;

#[cfg(test)]
mod tests;

pub(crate) use apply::Keep;
pub(crate) use runtime::{start as start_runtime, Materials, SyncRuntime};

use sha2::{Digest, Sha256};

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn unix_ms() -> i64 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}
