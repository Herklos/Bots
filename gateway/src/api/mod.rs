//! `hbots`: the gateway as one Rust binary.
//!
//! It sits between the Exact app (`app/`) and a harness backend. T3 Code is the backend
//! wired today (`mod t3`); other backends may be added. The gateway keeps Bot folders,
//! settings, the Vault, the Projects board and the Bots' own MCP tool server. Home is
//! `~/.hbots`.

use crate::platform::error::{ApiError, ApiResult};
use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, Query, Request, State, WebSocketUpgrade},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, delete, get, post, put},
    Extension, Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};

use crate::*;

mod access_api;
mod bots_api;
mod chat_api;
mod code_api;
#[cfg(test)]
mod code_api_tests;
mod computer_api;
mod library_api;
#[cfg(test)]
mod library_file_tests;
mod memory_api;
#[cfg(test)]
mod memory_api_tests;
#[cfg(test)]
mod provider_limits_tests;
#[cfg(test)]
mod provider_switch_tests;
mod rooms_api;
mod router;
mod routines_api;
#[cfg(test)]
mod runtime_mode_tests;
mod server;
mod settings;
mod state;
#[cfg(test)]
mod tests;
mod threads;
mod usage_api;
mod vault_api;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use access_api::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use bots_api::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use chat_api::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use code_api::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use computer_api::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use library_api::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use memory_api::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use rooms_api::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use router::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use routines_api::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use server::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use settings::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub use state::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use threads::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use usage_api::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use vault_api::*;
