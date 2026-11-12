//! Anytype sync. The decision table is local. Listing, apply, and the engine talk to the client.

mod apply;
mod conflict;
mod decide;
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
mod scan;
mod status;

#[cfg(test)]
mod tests;

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
