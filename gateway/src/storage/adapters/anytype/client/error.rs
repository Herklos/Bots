//! Failures from the Anytype HTTP API. Pairing still speaks `ApiError` so Settings
//! keeps the messages it already shows.

use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AnytypeError {
    Unreachable,
    NotGranted,
    EtagMismatch { current: Option<String> },
    RateLimited { retry_after: Duration },
    ObjectArchived,
    Gone,
    ApiChanged,
    Bad { status: u16, message: String },
}
