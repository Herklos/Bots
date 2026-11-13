//! Turn one path's scan row and listing row into an action, fetching the body when the
//! cheap fields say the object changed.

use super::super::client::AnytypeError;
use super::apply::{Ctx, Job, Outcome};
use super::decide::{self, Action, Local, Remote};
use super::error::{remote_note, SyncError};
use super::hook::{self, Step};
use super::manifest::Entry;
use super::policy::{classify, Class};
use super::remote::{self, Listing, Loaded, RemoteFile};
use super::scan::Scan;
use super::sha256_hex;
use super::status::Preview;
use std::time::Duration;

pub(super) enum Planned {
    Ready(Box<Job>),
    Skip,
    Stop,
    Halt(Outcome),
}

pub(super) async fn one(
    ctx: &mut Ctx<'_>,
    rel: &str,
    scan: &Scan,
    listing: &Listing,
    truncated: bool,
) -> Result<Planned, SyncError> {
    let Class::Allow { merge } = classify(rel) else {
        return Ok(Planned::Skip);
    };
    let present = scan.files.get(rel);
    if truncated && present.is_none() {
        return Ok(Planned::Skip);
    }
    let local_sha = present.map(|file| file.sha.clone());
    let local = match &local_sha {
        Some(sha) => Local::Present(sha.clone()),
        None => Local::Absent,
    };
    let entry = ctx.manifest.entries.get(rel).cloned();
    let mut viewed = match view_remote(ctx, rel, &entry, listing, local_sha.as_deref()).await? {
        RemoteView::Stop => return Ok(Planned::Stop),
        RemoteView::Halt(outcome) => return Ok(Planned::Halt(outcome)),
        RemoteView::Ready(view) => view,
    };
    let mut remote_state = viewed.state;
    let mut remote_bytes = viewed.bytes;
    let mut remote_sha = viewed.sha;
    let action = decide::decide(
        entry.as_ref().and_then(|entry| entry.base_sha.as_deref()),
        &local,
        &remote_state,
        merge,
    );
    if wants_remote_bytes(action) && remote_bytes.is_none() && !viewed.deleted {
        if let Some(object_id) = viewed.object_id.clone() {
            match fetch(ctx, rel, &object_id).await? {
                RemoteView::Stop => return Ok(Planned::Stop),
                RemoteView::Halt(outcome) => return Ok(Planned::Halt(outcome)),
                RemoteView::Ready(again) => {
                    remote_state = again.state;
                    remote_bytes = again.bytes;
                    remote_sha = again.sha;
                    if again.etag.is_some() {
                        viewed.etag = again.etag;
                    }
                    if again.modified.is_some() {
                        viewed.modified = again.modified;
                    }
                }
            }
        }
    }
    let action = decide::decide(
        entry.as_ref().and_then(|entry| entry.base_sha.as_deref()),
        &local,
        &remote_state,
        merge,
    );
    super::log::decision(
        ctx.sync_dir,
        rel,
        action.name(),
        entry.as_ref().and_then(|row| row.base_sha.as_deref()),
        local_sha.as_deref(),
        remote_sha.as_deref(),
    );
    let local_bytes = if wants_local_bytes(action) {
        match std::fs::read(ctx.root.join(rel)) {
            Ok(bytes) => {
                let sha = sha256_hex(&bytes);
                if local_sha.as_deref() != Some(sha.as_str()) {
                    return Ok(Planned::Skip);
                }
                Some(bytes)
            }
            Err(error) => {
                return Ok(Planned::Halt(Outcome::Failed(format!("{rel}: {error}"))));
            }
        }
    } else {
        None
    };
    Ok(Planned::Ready(Box::new(Job {
        rel: rel.to_string(),
        action,
        local_bytes,
        local_sha,
        remote_bytes,
        remote_sha,
        object_id: viewed.object_id,
        etag: viewed.etag,
        modified: viewed.modified,
        mtime_ns: present.map(|file| file.mtime_ns),
        base: entry.and_then(|entry| entry.base_sha),
        policy: merge,
    })))
}

struct View {
    state: Remote,
    bytes: Option<Vec<u8>>,
    sha: Option<String>,
    object_id: Option<String>,
    etag: Option<String>,
    modified: Option<String>,
    deleted: bool,
}

enum RemoteView {
    Ready(View),
    Stop,
    Halt(Outcome),
}

async fn view_remote(
    ctx: &mut Ctx<'_>,
    rel: &str,
    entry: &Option<Entry>,
    listing: &Listing,
    local_sha: Option<&str>,
) -> Result<RemoteView, SyncError> {
    if let Some(file) = listing.files.get(rel) {
        return listed(ctx, rel, entry.as_ref(), file).await;
    }
    let Some(object_id) = entry.as_ref().and_then(|entry| entry.object_id.clone()) else {
        return Ok(RemoteView::Ready(absent()));
    };
    if listing.contains_id(&object_id) {
        return Ok(RemoteView::Ready(absent()));
    }
    match fetch(ctx, rel, &object_id).await? {
        RemoteView::Stop => Ok(RemoteView::Stop),
        RemoteView::Halt(outcome) => match outcome {
            Outcome::Failed(message) if message.contains("gone") => Ok(RemoteView::Ready(absent())),
            other => Ok(RemoteView::Halt(other)),
        },
        RemoteView::Ready(loaded) => hidden(ctx, rel, &object_id, loaded, local_sha, entry).await,
    }
}

async fn listed(
    ctx: &mut Ctx<'_>,
    rel: &str,
    entry: Option<&Entry>,
    file: &RemoteFile,
) -> Result<RemoteView, SyncError> {
    if file.deleted {
        return Ok(RemoteView::Ready(View {
            state: Remote::Tombstone,
            bytes: None,
            sha: None,
            object_id: Some(file.object_id.clone()),
            etag: entry.and_then(|entry| entry.remote_etag.clone()),
            modified: file.modified.clone(),
            deleted: true,
        }));
    }
    if !remote::needs_body(entry, file) {
        return Ok(RemoteView::Ready(View {
            state: Remote::Present(file.advisory_sha.clone()),
            bytes: None,
            sha: Some(file.advisory_sha.clone()),
            object_id: Some(file.object_id.clone()),
            etag: entry.and_then(|entry| entry.remote_etag.clone()),
            modified: file
                .modified
                .clone()
                .or_else(|| entry.and_then(|entry| entry.remote_modified.clone())),
            deleted: false,
        }));
    }
    let mut fetched = match fetch(ctx, rel, &file.object_id).await? {
        RemoteView::Ready(view) => view,
        other => return Ok(other),
    };
    fetched.object_id = Some(file.object_id.clone());
    if fetched.modified.is_none() {
        fetched.modified = file.modified.clone();
    }
    Ok(RemoteView::Ready(fetched))
}

async fn hidden(
    ctx: &mut Ctx<'_>,
    rel: &str,
    object_id: &str,
    loaded: View,
    local_sha: Option<&str>,
    entry: &Option<Entry>,
) -> Result<RemoteView, SyncError> {
    if loaded.deleted {
        return Ok(RemoteView::Ready(View {
            state: Remote::Tombstone,
            deleted: true,
            object_id: Some(object_id.to_string()),
            ..loaded
        }));
    }
    let same = match (local_sha, loaded.sha.as_deref()) {
        (Some(local), Some(remote)) => local == remote,
        (None, Some(remote)) => {
            entry.as_ref().and_then(|entry| entry.base_sha.as_deref()) == Some(remote)
        }
        _ => false,
    };
    if same {
        return Ok(RemoteView::Ready(View {
            state: Remote::Tombstone,
            deleted: true,
            object_id: Some(object_id.to_string()),
            ..loaded
        }));
    }
    let key = super::payload::wire_key(
        &super::outbox::idem_key(
            ctx.device_id,
            &format!("restore:{rel}"),
            super::outbox::Op::Update,
            object_id,
        ),
        None,
    );
    if !hook::gate(ctx.hook, Step::BeforeRemoteCall, rel) {
        return Ok(RemoteView::Stop);
    }
    let restored = ctx.client.restore_object(object_id, &key).await;
    if !hook::gate(ctx.hook, Step::AfterRemoteCall, rel) {
        return Ok(RemoteView::Stop);
    }
    if let Err(error) = restored {
        return Ok(RemoteView::Halt(halt(error)));
    }
    fetch(ctx, rel, object_id).await
}

async fn fetch(ctx: &mut Ctx<'_>, rel: &str, object_id: &str) -> Result<RemoteView, SyncError> {
    if !hook::gate(ctx.hook, Step::BeforeRemoteCall, rel) {
        return Ok(RemoteView::Stop);
    }
    let loaded = remote::load(ctx.client, object_id).await;
    if !hook::gate(ctx.hook, Step::AfterRemoteCall, rel) {
        return Ok(RemoteView::Stop);
    }
    match loaded {
        Ok(loaded) => Ok(RemoteView::Ready(from_loaded(object_id, loaded))),
        Err(AnytypeError::Gone) => Ok(RemoteView::Halt(Outcome::Failed("gone".into()))),
        Err(error) => Ok(RemoteView::Halt(halt(error))),
    }
}

fn from_loaded(object_id: &str, loaded: Loaded) -> View {
    if loaded.deleted {
        View {
            state: Remote::Tombstone,
            bytes: None,
            sha: None,
            object_id: Some(object_id.to_string()),
            etag: loaded.etag,
            modified: loaded.modified,
            deleted: true,
        }
    } else {
        View {
            state: Remote::Present(loaded.sha.clone()),
            sha: Some(loaded.sha),
            bytes: Some(loaded.bytes),
            object_id: Some(object_id.to_string()),
            etag: loaded.etag,
            modified: loaded.modified,
            deleted: false,
        }
    }
}

fn absent() -> View {
    View {
        state: Remote::Absent,
        bytes: None,
        sha: None,
        object_id: None,
        etag: None,
        modified: None,
        deleted: false,
    }
}

fn halt(error: AnytypeError) -> Outcome {
    match error {
        AnytypeError::Unreachable => Outcome::Offline,
        AnytypeError::NotGranted => Outcome::NotGranted,
        AnytypeError::ApiChanged => Outcome::ReadOnly,
        AnytypeError::RateLimited { retry_after } => Outcome::Paused { retry_after },
        AnytypeError::Gone => Outcome::Failed("gone".into()),
        other => Outcome::Failed(remote_note(&other)),
    }
}

fn wants_remote_bytes(action: Action) -> bool {
    match action {
        Action::PullCreate
        | Action::PullUpdate
        | Action::EditWinsPull
        | Action::Merge
        | Action::Conflict => true,
        Action::Noop
        | Action::PushCreate
        | Action::PushUpdate
        | Action::PushTombstone
        | Action::Adopt
        | Action::DeleteLocal
        | Action::Drop
        | Action::EditWinsPush => false,
    }
}

fn wants_local_bytes(action: Action) -> bool {
    match action {
        Action::PushCreate
        | Action::PushUpdate
        | Action::EditWinsPush
        | Action::Merge
        | Action::Conflict => true,
        Action::Noop
        | Action::PullCreate
        | Action::PullUpdate
        | Action::EditWinsPull
        | Action::PushTombstone
        | Action::DeleteLocal
        | Action::Adopt
        | Action::Drop => false,
    }
}

pub(super) fn rank(path: &str) -> (u8, &str) {
    if path == ".botapp/shared-memory.md" {
        return (0, path);
    }
    let area = path.split('/').nth(1).unwrap_or("");
    let group = match area {
        "memory" => 1,
        "skills" => 2,
        "library" => 3,
        ".bot" => 4,
        _ => 5,
    };
    (group, path)
}

pub(super) fn tally(jobs: &[Job], rate: f64) -> Preview {
    let mut push = 0_usize;
    let mut pull = 0_usize;
    let mut conflict = 0_usize;
    for job in jobs {
        match job.action {
            Action::PushCreate
            | Action::PushUpdate
            | Action::PushTombstone
            | Action::EditWinsPush
            | Action::Merge => push += 1,
            Action::PullCreate
            | Action::PullUpdate
            | Action::EditWinsPull
            | Action::DeleteLocal => pull += 1,
            Action::Conflict => conflict += 1,
            Action::Noop | Action::Adopt | Action::Drop => {}
        }
    }
    let pushes = u32::try_from(push).unwrap_or(u32::MAX);
    let seconds = f64::from(pushes) / rate.max(0.25);
    Preview {
        push,
        pull,
        conflict,
        estimate: Duration::from_secs_f64(seconds),
    }
}

pub(super) fn is_push(action: Action) -> bool {
    match action {
        Action::PushCreate
        | Action::PushUpdate
        | Action::PushTombstone
        | Action::EditWinsPush
        | Action::Merge
        | Action::Conflict => true,
        Action::Noop
        | Action::PullCreate
        | Action::PullUpdate
        | Action::EditWinsPull
        | Action::DeleteLocal
        | Action::Adopt
        | Action::Drop => false,
    }
}
