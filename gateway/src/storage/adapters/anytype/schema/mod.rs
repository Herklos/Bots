//! Idempotent Anytype schema for the four hbots types.

mod bootstrap;
mod failure;
pub(crate) mod table;

#[cfg(test)]
pub(crate) use bootstrap::read_meta;
pub(crate) use bootstrap::{bootstrap, schema_ready};
pub(crate) use failure::setup_failure;
