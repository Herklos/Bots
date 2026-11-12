//! The gateway's own plumbing: config, errors, the CLI, the service, and self-update.

pub(crate) mod cli;
pub(crate) mod config;
pub(crate) mod error;
pub(crate) mod mcp;
pub(crate) mod redact;
pub(crate) mod release; // T6b: self-update and rollback.
pub(crate) mod service; // T6b: install and remove the background service.
