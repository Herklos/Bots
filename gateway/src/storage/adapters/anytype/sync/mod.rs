//! Local half of Anytype sync. These pieces decide what would move.
//! They do not open a connection.

mod decide;
mod history;
mod manifest;
mod merge;
mod outbox;
mod policy;
mod scan;

#[cfg(test)]
mod tests;

use sha2::{Digest, Sha256};

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
