//! Installed hbots releases, self-update and the automatic update.
//!
//! Releases are GitHub release assets: `hbots-<version>-<target>.tar.gz` holding one `hbots`
//! binary, checked against the release's `SHA256SUMS`. An installed copy lives in
//! `$HBOTS_HOME/runtime/versions/<version>/hbots`. `runtime/current` links to the version the
//! service runs, and `runtime/previous` to the one before it, kept for a rollback. An update only
//! swaps those links and restarts the service; if the new version doesn't answer `/api/health`,
//! the old one is switched back.

use crate::{error::ApiResult, AppState};
use anyhow::{anyhow, bail, Context, Result};
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::HashMap,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::fs::{symlink, PermissionsExt},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod github;
mod http;
mod runtime;
mod system;
#[cfg(test)]
mod tests;
mod updater;
mod version;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use github::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use http::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use runtime::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use system::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use updater::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use version::*;

/// GitHub releases for this repository. Override the list with HBOTS_RELEASES_URL.
pub(crate) const REPO: &str = "Herklos/Bots";

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

const USER_AGENT_NAME: &str = "hbots-updater";

/// How long a successful check is kept before the next one.
const CHECK_TTL: Duration = Duration::from_secs(15 * 60);

/// A failed check waits longer each time, up to this.
const MAX_BACKOFF: Duration = Duration::from_secs(2 * 60 * 60);

/// An update that hasn't restarted us by then failed (systemd can't tell us the updater exited).
pub(crate) const PENDING_MS: u64 = 30 * 60 * 1000;

/// How long a release waits for hbots to go quiet before the automatic update gives up on it.
pub(crate) const MAX_WAIT_MS: u64 = 24 * 60 * 60 * 1000;

// ---------- paths ----------

/// `HBOTS_HOME`, or `~/.hbots`.
#[allow(
    dead_code,
    reason = "the CLI commands are the callers; the helper stays allowed so the release module typechecks without them"
)]
pub(crate) fn home_dir() -> PathBuf {
    match std::env::var("HBOTS_HOME") {
        Ok(h) if !h.is_empty() => PathBuf::from(h),
        _ => PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".hbots"),
    }
}

/// The version folder's `hbots` binary, through the `current` link, so the service follows updates.
#[allow(
    dead_code,
    reason = "the service spec is the caller; the helper stays allowed so the release module typechecks without it"
)]
pub(crate) fn current_binary(home: &Path) -> PathBuf {
    RuntimePaths::new(home).current.join("hbots")
}
