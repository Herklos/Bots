//! Vault CSV import and export. The gateway accepts raw CSV, so a thin client need not parse it.
//!
//! Import and export use the Vault's public API (`list`, `get`, `create`, `update`). The Vault file
//! is rewritten once per saved item.

use crate::platform::error::{ApiError, ApiResult};
use crate::vault::{site_domain, Vault};
use crate::{parse, require_json, AppState};
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Map, Value};
use std::{collections::HashMap, sync::Arc};

mod export;
mod import;
mod parse;
#[cfg(test)]
mod tests;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use export::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use import::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use parse::*;
