//! Where Bots are stored. `workspace` is the trait the rest of the gateway uses,
//! `settings` is the saved choice, `sync` is the engine wiring, and `adapters` holds
//! one folder per backend.

pub(crate) mod adapters;
mod json;
pub(crate) mod local_lock;
pub(crate) mod settings;
pub(crate) mod stores;
pub(crate) mod sync;
pub(crate) mod workspace;

pub(crate) use adapters::filesystem::{
    read_json, write_atomic, write_json, write_json_private, LocalDisk,
};
pub(crate) use json::merge;
