//! Snapshot a Settings screen can show later. Names here are the engine's, not the chrome.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    NeedsImport,
    Importing,
    Live,
}

impl Phase {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::NeedsImport => "needs_import",
            Self::Importing => "importing",
            Self::Live => "live",
        }
    }
}

/// What the last cycle found. `phase` is the stored machine. `state` overlays it when the
/// link to Anytype is the thing a person would notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SyncState {
    NeedsImport,
    Importing,
    Live,
    Offline,
    NotGranted,
    ApiChanged,
    NotFound,
}

impl SyncState {
    pub(super) fn from_phase(phase: Phase) -> Self {
        match phase {
            Phase::NeedsImport => Self::NeedsImport,
            Phase::Importing => Self::Importing,
            Phase::Live => Self::Live,
        }
    }

    /// The wire name. The gateway and the UI keep these words for the state.
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::NeedsImport => "needs_import",
            Self::Importing => "importing",
            Self::Live => "live",
            Self::Offline => "offline",
            Self::NotGranted => "not_granted",
            Self::ApiChanged => "api_changed",
            Self::NotFound => "not_found",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Preview {
    pub push: usize,
    pub pull: usize,
    pub conflict: usize,
    pub estimate: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SyncStatus {
    pub phase: Phase,
    pub state: SyncState,
    pub outbox: usize,
    pub conflicts: usize,
    pub duplicates: usize,
    pub retry_in: Duration,
    pub last_error: Option<String>,
    pub preview: Option<Preview>,
}

impl SyncStatus {
    pub(super) fn fresh(phase: Phase) -> Self {
        Self {
            phase,
            state: SyncState::from_phase(phase),
            outbox: 0,
            conflicts: 0,
            duplicates: 0,
            retry_in: Duration::ZERO,
            last_error: None,
            preview: None,
        }
    }
}

/// How long the background loop waits. Scan work is due every 3s. After a failure the
/// wait grows from 2s to 60s. A nudge or an explicit run does not wait.
pub(super) fn next_wait(offline: bool, backoff: Duration, nudged: bool) -> Duration {
    if nudged {
        return Duration::ZERO;
    }
    if offline {
        return backoff.clamp(Duration::from_secs(2), Duration::from_secs(60));
    }
    Duration::from_secs(3)
}

pub(super) fn grow_backoff(current: Duration) -> Duration {
    let next = current.saturating_mul(2).max(Duration::from_secs(2));
    next.min(Duration::from_secs(60))
}
