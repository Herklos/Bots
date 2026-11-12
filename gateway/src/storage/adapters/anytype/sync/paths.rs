//! On-disk names for the sync directory. Kept beside the engine so that file stays short.

use std::path::{Path, PathBuf};

pub(super) fn manifest_path(dir: &Path) -> PathBuf {
    dir.join("manifest.json")
}

pub(super) fn outbox_path(dir: &Path) -> PathBuf {
    dir.join("outbox.jsonl")
}

pub(super) fn device_path(dir: &Path) -> PathBuf {
    dir.join("device-id")
}
