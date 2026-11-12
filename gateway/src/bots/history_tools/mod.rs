//! Pure pieces of a search over one Bot's own history.
//!
//! [`score`] grades a message. [`window`] builds the page around a match from a
//! projection the caller already holds. [`scope`] is the set of threads that
//! caller may search. Nothing here places a call. The tool that returns these
//! to a Bot is a later change.
//!
//! The allow on dead code stays until that tool calls these from the binary.

#![allow(
    dead_code,
    reason = "the history search tool that calls these is a later phase"
)]

mod scope;
mod score;
mod window;

#[cfg(test)]
mod scope_tests;
#[cfg(test)]
mod score_tests;
#[cfg(test)]
mod window_tests;
