//! Failures from a harness capability. A runtime that lacks one returns [`HarnessError::Unsupported`].

use crate::platform::error::ApiError;

#[derive(Debug)]
pub enum HarnessError {
    /// The connection could not be opened.
    Unavailable(String),
    /// The call was sent and the runtime rejected it or closed it.
    Failed(String),
    /// This runtime does not implement the capability.
    Unsupported(&'static str),
}

impl HarnessError {
    pub(crate) fn unavailable(err: impl ToString) -> Self {
        Self::Unavailable(err.to_string())
    }

    pub(crate) fn failed(err: impl ToString) -> Self {
        Self::Failed(err.to_string())
    }

    #[allow(
        dead_code,
        reason = "a runtime other than T3 returns this when it lacks a capability"
    )]
    pub(crate) fn unsupported(what: &'static str) -> Self {
        Self::Unsupported(what)
    }

    /// The 502 the routes already return. A missing capability is 501.
    pub(crate) fn api(self) -> ApiError {
        match self {
            Self::Unavailable(msg) => ApiError::new(502, format!("T3 unavailable: {msg}")),
            Self::Failed(msg) => ApiError::new(502, msg),
            Self::Unsupported(what) => {
                ApiError::new(501, format!("This harness does not support {what}."))
            }
        }
    }

    pub(crate) fn api_note(self, note: impl Fn(String) -> String) -> ApiError {
        match self {
            Self::Failed(msg) => ApiError::new(502, note(msg)),
            other => other.api(),
        }
    }
}

impl std::error::Error for HarnessError {}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(msg) | Self::Failed(msg) => f.write_str(msg),
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
        }
    }
}
