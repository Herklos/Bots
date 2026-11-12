//! The Anytype adapter: a local HTTP client. Pairing, the space check, and schema
//! setup are what Settings uses. File sync is not wired into storage yet.

mod client;
mod schema;
#[allow(
    dead_code,
    reason = "M5 wires the engine into the gateway; until then only its tests call it"
)]
mod sync;
#[cfg(test)]
mod tests;

pub(crate) use client::{confirm_pairing, open_client, start_pairing, status, AnytypeError};
pub(crate) use schema::{bootstrap, schema_ready};
