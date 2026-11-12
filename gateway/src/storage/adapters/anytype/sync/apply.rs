//! One action for one path. Local bytes are re-read just before a pull replaces them.
//! Every overwrite or delete copies the old bytes into the history ring first.

use super::super::client::AnytypeClient;
use super::conflict::{self, ConflictState};
use super::decide::Action;
use super::echo::EchoBook;
use super::error::SyncError;
use super::history;
use super::hook::{self, Step};
use super::manifest::{self, Entry, Manifest};
use super::merge::{self, Merged};
use super::outbox::Outbox;
use super::policy::{self, MergePolicy, PullReject};
use super::push;
use super::sha256_hex;
use crate::storage::write_atomic;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug, Clone)]
pub(super) struct Job {
    pub rel: String,
    pub action: Action,
    pub local_bytes: Option<Vec<u8>>,
    pub local_sha: Option<String>,
    pub remote_bytes: Option<Vec<u8>>,
    pub remote_sha: Option<String>,
    pub object_id: Option<String>,
    pub etag: Option<String>,
    pub modified: Option<String>,
    pub mtime_ns: Option<i64>,
    pub base: Option<String>,
    pub policy: MergePolicy,
}

pub(super) struct Ctx<'a> {
    pub client: &'a AnytypeClient,
    pub root: &'a Path,
    pub sync_dir: &'a Path,
    pub device_id: &'a str,
    pub manifest: &'a mut Manifest,
    pub outbox: &'a mut Outbox,
    pub conflicts: &'a mut super::conflict::Conflicts,
    pub hook: &'a mut dyn hook::Hook,
    pub echo: &'a mut EchoBook,
}

#[derive(Debug)]
pub(super) enum Outcome {
    Applied,
    Idle,
    Stopped,
    Paused { retry_after: Duration },
    Offline,
    NotGranted,
    ReadOnly,
    Failed(String),
}

pub(super) async fn perform(ctx: &mut Ctx<'_>, job: &Job) -> Result<Outcome, SyncError> {
    match job.action {
        Action::Noop => ack_idle(ctx, &job.rel).await,
        Action::PushCreate => push::create(ctx, job).await,
        Action::PushUpdate | Action::EditWinsPush => push::update(ctx, job, false).await,
        Action::PushTombstone => push::update(ctx, job, true).await,
        Action::PullCreate | Action::PullUpdate | Action::EditWinsPull => pull(ctx, job).await,
        Action::DeleteLocal => delete_local(ctx, job).await,
        Action::Merge => merge_both(ctx, job).await,
        Action::Conflict => {
            let local = job.local_bytes.clone().unwrap_or_default();
            let remote = job.remote_bytes.clone().unwrap_or_default();
            let local_sha = sha256_hex(&local);
            let remote_sha = job
                .remote_sha
                .clone()
                .unwrap_or_else(|| sha256_hex(&remote));
            open_conflict(ctx, job, &local_sha, &remote_sha, &local, &remote).await
        }
        Action::Adopt => adopt(ctx, job).await,
        Action::Drop => drop_entry(ctx, &job.rel).await,
    }
}

pub(super) async fn open_conflict(
    ctx: &mut Ctx<'_>,
    job: &Job,
    local_sha: &str,
    remote_sha: &str,
    local: &[u8],
    remote: &[u8],
) -> Result<Outcome, SyncError> {
    let sidecar = conflict::sidecar_rel(&job.rel, ctx.device_id, conflict::now_secs());
    write_atomic(&ctx.root.join(&sidecar), remote, None).await?;
    if !hook::gate(ctx.hook, Step::BeforeRemoteCall, &job.rel) {
        return Ok(Outcome::Stopped);
    }
    let published = conflict::publish(
        ctx.client,
        ctx.device_id,
        &job.rel,
        local_sha,
        remote_sha,
        local,
        remote,
    )
    .await;
    if !hook::gate(ctx.hook, Step::AfterRemoteCall, &job.rel) {
        return Ok(Outcome::Stopped);
    }
    let object_id = match published {
        Ok(id) => Some(id),
        Err(error) => return Ok(push::note(&error)),
    };
    ctx.conflicts.insert(conflict::record_for(
        &job.rel, object_id, local_sha, remote_sha, &sidecar,
    ));
    if !hook::gate(ctx.hook, Step::BeforeManifestWrite, &job.rel) {
        return Ok(Outcome::Stopped);
    }
    ctx.conflicts
        .save(&conflict::conflicts_path(ctx.sync_dir))
        .await?;
    Ok(Outcome::Applied)
}

pub(super) async fn commit(
    ctx: &mut Ctx<'_>,
    rel: &str,
    entry: Entry,
    bytes: Option<&[u8]>,
    ack: bool,
) -> Result<Outcome, SyncError> {
    if let Some(bytes) = bytes {
        if !entry.tombstone {
            let path = ctx.sync_dir.join("bases").join(sha256_hex(rel.as_bytes()));
            write_atomic(&path, bytes, Some(0o600)).await?;
        }
    }
    if !hook::gate(ctx.hook, Step::BeforeManifestWrite, rel) {
        return Ok(Outcome::Stopped);
    }
    let mut updated = ctx.manifest.clone();
    updated.entries.insert(rel.to_string(), entry);
    manifest::save(&manifest_path(ctx.sync_dir), &updated).await?;
    *ctx.manifest = updated;
    if ack {
        ctx.outbox.ack_path(rel);
        ctx.outbox.save(&outbox_path(ctx.sync_dir)).await?;
    }
    Ok(Outcome::Applied)
}

pub(super) async fn resolve(ctx: &mut Ctx<'_>, id: &str, keep: Keep) -> Result<Outcome, SyncError> {
    let Some(record) = ctx.conflicts.by_id(id).cloned() else {
        return Ok(Outcome::Failed(format!("unknown conflict {id}")));
    };
    if record.state != ConflictState::Open {
        return Ok(Outcome::Idle);
    }
    let sidecar = ctx.root.join(&record.sidecar);
    match keep {
        Keep::Local => {
            let bytes = std::fs::read(ctx.root.join(&record.path))?;
            let mut job = job_from_record(ctx, &record);
            job.action = Action::PushUpdate;
            job.local_bytes = Some(bytes);
            refresh_etag(ctx, &mut job).await;
            ctx.conflicts.remove_id(id);
            let outcome = push::update(ctx, &job, false).await?;
            finish_resolve(ctx, &record, &sidecar, &outcome).await?;
            Ok(outcome)
        }
        Keep::Remote => {
            let bytes = std::fs::read(&sidecar)?;
            preserve(ctx, &record.path).await?;
            write_atomic(&ctx.root.join(&record.path), &bytes, None).await?;
            let mut entry = ctx
                .manifest
                .entries
                .get(&record.path)
                .cloned()
                .unwrap_or_else(Entry::empty);
            entry.base_sha = Some(record.remote_sha.clone());
            entry.tombstone = false;
            entry.remote_etag = None;
            ctx.conflicts.remove_id(id);
            let outcome = commit(ctx, &record.path, entry, Some(&bytes), true).await?;
            finish_resolve(ctx, &record, &sidecar, &outcome).await?;
            Ok(outcome)
        }
        Keep::Both => {
            let remote = std::fs::read(&sidecar)?;
            let sibling = conflict::sibling_rel(&record.path, conflict::now_secs());
            write_atomic(&ctx.root.join(&sibling), &remote, None).await?;
            let bytes = std::fs::read(ctx.root.join(&record.path))?;
            let mut job = job_from_record(ctx, &record);
            job.action = Action::PushUpdate;
            job.local_bytes = Some(bytes);
            refresh_etag(ctx, &mut job).await;
            ctx.conflicts.remove_id(id);
            let outcome = push::update(ctx, &job, false).await?;
            finish_resolve(ctx, &record, &sidecar, &outcome).await?;
            Ok(outcome)
        }
    }
}

async fn refresh_etag(ctx: &Ctx<'_>, job: &mut Job) {
    let Some(object_id) = job.object_id.clone() else {
        return;
    };
    if let Ok(loaded) = super::remote::load(ctx.client, &object_id).await {
        job.etag = loaded.etag;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keep {
    Local,
    Remote,
    Both,
}

async fn pull(ctx: &mut Ctx<'_>, job: &Job) -> Result<Outcome, SyncError> {
    let Some(bytes) = job.remote_bytes.as_deref() else {
        return Ok(Outcome::Failed(format!("{} has no remote bytes", job.rel)));
    };
    match policy::accept_pulled(ctx.root, &job.rel) {
        Ok(_) => {}
        Err(
            PullReject::Escapes
            | PullReject::Denied
            | PullReject::Symlink
            | PullReject::Unavailable,
        ) => {
            return Ok(Outcome::Failed(format!("{} is not a safe pull", job.rel)));
        }
    }
    let path = ctx.root.join(&job.rel);
    if path.exists() {
        let current = std::fs::read(&path)?;
        let sha = sha256_hex(&current);
        if job.local_sha.as_deref() != Some(sha.as_str()) {
            return Ok(Outcome::Failed(format!("{} changed before pull", job.rel)));
        }
        preserve(ctx, &job.rel).await?;
    }
    let mode = existing_mode(&path);
    write_atomic(&path, bytes, mode).await?;
    let (size, mtime) = match stat(&path) {
        Some((size, mtime)) => (Some(size), Some(mtime)),
        None => (None, None),
    };
    let sha = job.remote_sha.clone().unwrap_or_else(|| sha256_hex(bytes));
    let mut entry = ctx
        .manifest
        .entries
        .get(&job.rel)
        .cloned()
        .unwrap_or_else(Entry::empty);
    entry.base_sha = Some(sha);
    entry.object_id = job.object_id.clone();
    entry.remote_etag = job.etag.clone();
    entry.remote_modified = job.modified.clone();
    entry.local_size = size;
    entry.local_mtime_ns = mtime;
    entry.tombstone = false;
    commit(ctx, &job.rel, entry, Some(bytes), true).await
}

async fn merge_both(ctx: &mut Ctx<'_>, job: &Job) -> Result<Outcome, SyncError> {
    let Some(local) = job.local_bytes.as_deref() else {
        return Ok(Outcome::Failed(format!("{} has no local bytes", job.rel)));
    };
    let Some(remote) = job.remote_bytes.as_deref() else {
        return Ok(Outcome::Failed(format!("{} has no remote bytes", job.rel)));
    };
    let Some(base) = std::fs::read(base_file(ctx.sync_dir, &job.rel)).ok() else {
        return open_conflict(
            ctx,
            job,
            &sha256_hex(local),
            &sha256_hex(remote),
            local,
            remote,
        )
        .await;
    };
    let (Ok(base), Ok(local_text), Ok(remote_text)) = (
        std::str::from_utf8(&base),
        std::str::from_utf8(local),
        std::str::from_utf8(remote),
    ) else {
        return open_conflict(
            ctx,
            job,
            &sha256_hex(local),
            &sha256_hex(remote),
            local,
            remote,
        )
        .await;
    };
    match merge::merge_text(base, local_text, remote_text) {
        Merged::Conflict => {
            open_conflict(
                ctx,
                job,
                &sha256_hex(local),
                &sha256_hex(remote),
                local,
                remote,
            )
            .await
        }
        Merged::Clean(text) => {
            let merged = text.into_bytes();
            preserve(ctx, &job.rel).await?;
            let path = ctx.root.join(&job.rel);
            let mode = existing_mode(&path);
            write_atomic(&path, &merged, mode).await?;
            let mut next = job.clone();
            next.local_bytes = Some(merged);
            next.action = Action::PushUpdate;
            if let Some((size, mtime)) = stat(&path) {
                next.mtime_ns = Some(mtime);
                next.local_sha = Some(sha256_hex(next.local_bytes.as_deref().unwrap_or_default()));
                let _ = size;
            }
            push::update(ctx, &next, false).await
        }
    }
}

async fn adopt(ctx: &mut Ctx<'_>, job: &Job) -> Result<Outcome, SyncError> {
    let sha = job
        .local_sha
        .clone()
        .or_else(|| job.remote_sha.clone())
        .unwrap_or_default();
    let mut entry = ctx
        .manifest
        .entries
        .get(&job.rel)
        .cloned()
        .unwrap_or_else(Entry::empty);
    entry.base_sha = Some(sha);
    entry.object_id = job.object_id.clone().or(entry.object_id);
    entry.remote_etag = job.etag.clone().or(entry.remote_etag);
    entry.remote_modified = job.modified.clone().or(entry.remote_modified);
    entry.tombstone = false;
    if let Some((size, mtime)) = stat(&ctx.root.join(&job.rel)) {
        entry.local_size = Some(size);
        entry.local_mtime_ns = Some(mtime);
    }
    let bytes = job.local_bytes.as_deref().or(job.remote_bytes.as_deref());
    commit(ctx, &job.rel, entry, bytes, true).await
}

async fn delete_local(ctx: &mut Ctx<'_>, job: &Job) -> Result<Outcome, SyncError> {
    preserve(ctx, &job.rel).await?;
    let path = ctx.root.join(&job.rel);
    if path.is_file() {
        std::fs::remove_file(&path)?;
    }
    drop_entry(ctx, &job.rel).await
}

async fn drop_entry(ctx: &mut Ctx<'_>, rel: &str) -> Result<Outcome, SyncError> {
    if !hook::gate(ctx.hook, Step::BeforeManifestWrite, rel) {
        return Ok(Outcome::Stopped);
    }
    let mut updated = ctx.manifest.clone();
    updated.entries.remove(rel);
    manifest::save(&manifest_path(ctx.sync_dir), &updated).await?;
    *ctx.manifest = updated;
    ctx.outbox.ack_path(rel);
    ctx.outbox.save(&outbox_path(ctx.sync_dir)).await?;
    Ok(Outcome::Applied)
}

async fn ack_idle(ctx: &mut Ctx<'_>, rel: &str) -> Result<Outcome, SyncError> {
    if ctx.outbox.ack_path(rel) > 0 {
        ctx.outbox.save(&outbox_path(ctx.sync_dir)).await?;
    }
    Ok(Outcome::Idle)
}

async fn finish_resolve(
    ctx: &mut Ctx<'_>,
    record: &conflict::Record,
    sidecar: &Path,
    outcome: &Outcome,
) -> Result<(), SyncError> {
    if !matches!(outcome, Outcome::Applied | Outcome::Idle) {
        return Ok(());
    }
    if let Some(object_id) = &record.object_id {
        let _ = conflict::mark(
            ctx.client,
            ctx.device_id,
            &record.path,
            object_id,
            ConflictState::Resolved,
        )
        .await;
    }
    if sidecar.is_file() {
        let _ = std::fs::remove_file(sidecar);
    }
    ctx.conflicts
        .save(&conflict::conflicts_path(ctx.sync_dir))
        .await?;
    Ok(())
}

fn job_from_record(ctx: &Ctx<'_>, record: &conflict::Record) -> Job {
    let entry = ctx.manifest.entries.get(&record.path);
    Job {
        rel: record.path.clone(),
        action: Action::PushUpdate,
        local_bytes: None,
        local_sha: Some(record.local_sha.clone()),
        remote_bytes: None,
        remote_sha: Some(record.remote_sha.clone()),
        object_id: entry.and_then(|entry| entry.object_id.clone()),
        etag: entry.and_then(|entry| entry.remote_etag.clone()),
        modified: entry.and_then(|entry| entry.remote_modified.clone()),
        mtime_ns: entry.and_then(|entry| entry.local_mtime_ns),
        base: entry.and_then(|entry| entry.base_sha.clone()),
        policy: MergePolicy::Whole,
    }
}

async fn preserve(ctx: &Ctx<'_>, rel: &str) -> Result<(), SyncError> {
    let path = ctx.root.join(rel);
    if !path.is_file() {
        return Ok(());
    }
    let bytes = std::fs::read(&path)?;
    history::record(&ctx.sync_dir.join("history"), rel, &bytes).await?;
    Ok(())
}

fn stat(path: &Path) -> Option<(u64, i64)> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let nanos = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((meta.len(), i64::try_from(nanos).ok()?))
}

fn existing_mode(path: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        let mode = std::fs::symlink_metadata(path).ok()?.permissions().mode();
        if mode & 0o111 != 0 {
            Some(0o755)
        } else {
            Some(0o644)
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

fn manifest_path(dir: &Path) -> PathBuf {
    dir.join("manifest.json")
}

fn outbox_path(dir: &Path) -> PathBuf {
    dir.join("outbox.jsonl")
}

fn base_file(dir: &Path, rel: &str) -> PathBuf {
    dir.join("bases").join(sha256_hex(rel.as_bytes()))
}
