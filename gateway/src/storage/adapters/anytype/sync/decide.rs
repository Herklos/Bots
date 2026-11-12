//! Pure reconcile decision. Shas in, an action out. No disk and no network.
//!
//! The both-edited row asks for a line merge when the path is text. This
//! function returns [`Action::Merge`] for that case. Whether the merge is clean
//! is decided by `merge`, which has the bytes. `Whole` conflicts immediately.

use super::policy::MergePolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Local {
    Absent,
    Present(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Remote {
    Absent,
    Present(String),
    Tombstone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Noop,
    PushCreate,
    PullCreate,
    Adopt,
    Conflict,
    PushUpdate,
    PullUpdate,
    Merge,
    PushTombstone,
    DeleteLocal,
    Drop,
    /// Local bytes changed and the remote object is a tombstone. Push them back.
    EditWinsPush,
    /// The local file is gone and the remote bytes changed. Pull them back.
    EditWinsPull,
}

impl Action {
    /// Deletes a local file or tombstones the remote object.
    pub(super) fn destroys_content(self) -> bool {
        match self {
            Self::DeleteLocal | Self::PushTombstone => true,
            Self::Noop
            | Self::PushCreate
            | Self::PullCreate
            | Self::Adopt
            | Self::Conflict
            | Self::PushUpdate
            | Self::PullUpdate
            | Self::Merge
            | Self::Drop
            | Self::EditWinsPush
            | Self::EditWinsPull => false,
        }
    }
}

pub(super) fn decide(
    base: Option<&str>,
    local: &Local,
    remote: &Remote,
    policy: MergePolicy,
) -> Action {
    match base {
        None => decide_unbased(local, remote),
        Some(base) => decide_based(base, local, remote, policy),
    }
}

fn decide_unbased(local: &Local, remote: &Remote) -> Action {
    match (local, remote) {
        (Local::Absent, Remote::Absent | Remote::Tombstone) => Action::Noop,
        (Local::Present(_), Remote::Absent) => Action::PushCreate,
        // A tombstone we never based must not delete the local file.
        (Local::Present(_), Remote::Tombstone) => Action::Conflict,
        (Local::Absent, Remote::Present(_)) => Action::PullCreate,
        (Local::Present(local_sha), Remote::Present(remote_sha)) => {
            if local_sha == remote_sha {
                Action::Adopt
            } else {
                Action::Conflict
            }
        }
    }
}

fn decide_based(base: &str, local: &Local, remote: &Remote, policy: MergePolicy) -> Action {
    match (local, remote) {
        (Local::Absent, Remote::Absent | Remote::Tombstone) => Action::Drop,
        (Local::Absent, Remote::Present(sha)) => {
            if sha == base {
                Action::PushTombstone
            } else {
                Action::EditWinsPull
            }
        }
        // The object was purged. A later create is safe because the caller searches first.
        (Local::Present(_), Remote::Absent) => Action::PushCreate,
        (Local::Present(sha), Remote::Tombstone) => {
            if sha == base {
                Action::DeleteLocal
            } else {
                Action::EditWinsPush
            }
        }
        (Local::Present(local_sha), Remote::Present(remote_sha)) => {
            match (
                local_sha == base,
                remote_sha == base,
                local_sha == remote_sha,
            ) {
                (true, true, _) => Action::Noop,
                (false, true, _) => Action::PushUpdate,
                (true, false, _) => Action::PullUpdate,
                (false, false, true) => Action::Adopt,
                (false, false, false) => match policy {
                    MergePolicy::Text3Way => Action::Merge,
                    MergePolicy::Whole => Action::Conflict,
                },
            }
        }
    }
}
