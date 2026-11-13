//! Idempotent Anytype schema for the four hbots types.

mod bootstrap;
mod failure;
mod p4;
#[cfg(test)]
mod p4_tests;
#[cfg(test)]
mod p5_tests;
mod p5;
pub(crate) mod table;

#[cfg(test)]
pub(crate) use bootstrap::read_meta;
pub(crate) use bootstrap::{bootstrap, schema_ready};
pub(crate) use failure::setup_failure;
