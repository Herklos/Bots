//! The gate for code work.
//!
//! `beta.code` is the master switch and defaults to off. A Bot's `code` field
//! is `auto`, `on`, or `off`, and a missing field is `auto`. While the switch
//! is off, the code route returns `{ "enabled": false }` and does not ask the
//! harness anything.

mod cache;
mod signal;

#[cfg(test)]
mod tests;

pub(crate) use cache::{evaluate, SignalCache};
pub(crate) use signal::{mode_of, require_mode};
