//! The Anytype adapter: a local HTTP client. Pairing, the space check, and schema
//! setup are what Settings uses. File sync is not wired into storage yet.

mod client;
mod schema;
#[cfg(test)]
mod tests;

pub(crate) use client::{confirm_pairing, open_client, start_pairing, status, AnytypeError};
pub(crate) use schema::{bootstrap, schema_ready};
