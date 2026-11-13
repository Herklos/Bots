//! The Anytype adapter: a local HTTP client, the sync engine behind the gateway,
//! and the workspace wrapper that nudges the engine after a write. Pairing, the
//! space check, and schema setup are what Settings uses.

mod client;
mod schema;
pub(crate) mod sync;
#[cfg(test)]
mod tests;
mod workspace;

pub(crate) use client::{confirm_pairing, open_client, start_pairing, status, AnytypeError};
pub(crate) use schema::{bootstrap, schema_ready, setup_failure};
pub(crate) use workspace::SyncedWorkspace;
