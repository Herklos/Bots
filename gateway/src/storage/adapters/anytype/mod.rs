//! The Anytype adapter: a thin client for the local Anytype HTTP API.
mod client;
#[cfg(test)]
mod tests;

pub(crate) use client::{confirm_pairing, start_pairing, status};
