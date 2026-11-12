//! Failures that stop a cycle. A remote rejection is a per-path outcome, not one of these.

use super::super::client::AnytypeError;
use super::manifest::ManifestError;
use super::outbox::OutboxError;
use std::io;

#[derive(Debug)]
pub(super) enum SyncError {
    Io(io::Error),
    Manifest(ManifestError),
    Outbox(OutboxError),
    Conflict(String),
}

impl From<io::Error> for SyncError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ManifestError> for SyncError {
    fn from(error: ManifestError) -> Self {
        Self::Manifest(error)
    }
}

impl From<OutboxError> for SyncError {
    fn from(error: OutboxError) -> Self {
        Self::Outbox(error)
    }
}

pub(super) fn remote_note(error: &AnytypeError) -> String {
    match error {
        AnytypeError::Unreachable => "unreachable".to_string(),
        AnytypeError::NotGranted => "not granted".to_string(),
        AnytypeError::EtagMismatch { .. } => "etag mismatch".to_string(),
        AnytypeError::RateLimited { .. } => "rate limited".to_string(),
        AnytypeError::ObjectArchived => "object archived".to_string(),
        AnytypeError::Gone => "gone".to_string(),
        AnytypeError::ApiChanged => "api changed".to_string(),
        AnytypeError::Bad { status, message } => format!("{status} {message}"),
    }
}
