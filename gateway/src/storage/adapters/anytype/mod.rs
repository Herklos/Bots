//! The Anytype adapter: a local HTTP client, the sync engine behind the gateway,
//! and the workspace wrapper that nudges the engine after a write. Pairing, the
//! space check, and schema setup are what Settings uses.

mod client;
mod codec;
mod schema;
mod stores;
pub(crate) mod sync;
#[cfg(test)]
mod tests;
pub(crate) mod typed;
mod workspace;

pub(crate) use client::{confirm_pairing, open_client, start_pairing, status, AnytypeClient, AnytypeError};
pub(crate) use schema::{bootstrap, schema_ready, setup_failure};
pub(crate) use workspace::SyncedWorkspace;
