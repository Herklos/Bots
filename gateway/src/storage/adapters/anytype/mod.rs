//! The Anytype adapter: a local HTTP client. Pairing and the space check are what
//! Settings uses. Object, search, schema, and file calls are not wired into storage yet.

mod client;
#[cfg(test)]
mod tests;

pub(crate) use client::{confirm_pairing, start_pairing, status};
