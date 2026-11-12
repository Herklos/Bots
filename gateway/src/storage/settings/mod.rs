//! Where Bots are stored. The running gateway still reads the file system.
//! A saved `anytype` choice is kept. Saving that choice requires a key and a
//! reachable space (`POST /api/storage/anytype/check`). Responses never include
//! an Anytype key.
//!
//! `~/.hbots/storage.json` (the hbots home, outside the Bots root) holds the choice.
//! Environment variables win and lock the field they set, the same way
//! `HBOTS_NETWORK_ACCESS` locks network access. A bad saved folder does not stop
//! startup: the process uses the file-system default and `GET /api/storage` says why.
//! Changing the folder does not move data. Copying checks hashes and leaves the
//! old folder in place.

use crate::network::exposure::Surface;
use crate::platform::error::{ApiError, ApiResult};
use crate::storage::adapters::filesystem::copy::*;
use crate::storage::adapters::filesystem::folder::*;
use crate::AppState;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap};
use axum::Extension;
use axum::Json;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const DEFAULT_HOST: &str = "http://127.0.0.1:31009";
const DEFAULT_SPACE: &str = "hbots";
const KEY_NAME: &str = "hbots";
const FILE_NAME: &str = "storage.json";
const KEY_FILE: &str = "anytype-key";

mod anytype;
mod env;
mod filesystem;
mod http;
mod save;
mod stored;
#[cfg(test)]
mod tests;
mod view;

use anytype::*;
use env::*;
use filesystem::*;
use http::*;
use save::*;
use stored::*;
use view::*;

pub(crate) use anytype::{
    anytype_check, delete_storage_key, pair_confirm, pair_start, put_storage_key,
};
pub(crate) use env::ProcessEnv;
pub(crate) use filesystem::resolve_root;
pub(crate) use http::{copy_storage, get_storage, put_storage, restart_storage, test_storage};
