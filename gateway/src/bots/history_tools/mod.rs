//! Pieces of a search over one Bot's own history.
//!
//! [`score`] grades a message. [`window`] builds the page around a match.
//! [`scope`] is the set of threads a caller may search. [`source`] asks the
//! harness, then scans those threads when that search is missing, fails, or
//! is crowded out. The tool that returns this to a Bot is a later change.
//!
//! The allow on dead code stays until that tool calls these from the binary.

#![allow(
    dead_code,
    reason = "the history search tool that calls these is a later phase"
)]

mod cache;
mod scope;
mod score;
mod source;
mod window;

#[cfg(test)]
mod scope_tests;
#[cfg(test)]
mod score_tests;
#[cfg(test)]
mod source_tests;
#[cfg(test)]
mod window_tests;
