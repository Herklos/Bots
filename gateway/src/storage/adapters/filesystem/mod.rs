//! The file system adapter: Bots live in a folder on this machine.
pub(crate) mod atomic;
pub(crate) mod copy;
pub(crate) mod folder;
mod local_disk;

pub(crate) use atomic::{read_json, write_atomic, write_json};
pub(crate) use local_disk::LocalDisk;
