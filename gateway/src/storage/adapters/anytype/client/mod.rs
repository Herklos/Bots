//! Local Anytype HTTP client. `host` is the API origin, `key` is the scoped API key,
//! and `space_id` is the space later calls read and write.
//!
//! Object, search, schema, and file calls are part of this milestone and are not
//! reached by the running gateway yet. The filesystem backend stays the one in use.

#![allow(
    dead_code,
    reason = "storage still uses the filesystem; these calls are the M1 client"
)]

mod error;
mod files;
mod http;
mod limiter;
mod objects;
mod pairing;
mod schema;
mod search;
mod status;

pub(crate) use error::AnytypeError;
pub(crate) use pairing::{confirm_pairing, start_pairing};
pub(crate) use status::{open_client, status};

use limiter::Limiter;

pub(crate) struct AnytypeClient {
    host: String,
    key: String,
    space_id: String,
    http: reqwest::Client,
    limiter: Limiter,
}

impl std::fmt::Debug for AnytypeClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AnytypeClient")
            .field("host", &self.host)
            .field("space_id", &self.space_id)
            .field("key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl AnytypeClient {
    pub(crate) fn new(
        host: impl Into<String>,
        key: impl Into<String>,
        space_id: impl Into<String>,
    ) -> Result<Self, AnytypeError> {
        Self::with_rate(host, key, space_id, 8.0, 8.0)
    }

    pub(crate) fn with_rate(
        host: impl Into<String>,
        key: impl Into<String>,
        space_id: impl Into<String>,
        rate: f64,
        burst: f64,
    ) -> Result<Self, AnytypeError> {
        Ok(Self {
            host: host.into().trim_end_matches('/').to_string(),
            key: key.into(),
            space_id: space_id.into(),
            http: http::shared_client()?,
            limiter: Limiter::new(rate, burst),
        })
    }

    pub(crate) fn rate(&self) -> f64 {
        self.limiter.rate()
    }
}
