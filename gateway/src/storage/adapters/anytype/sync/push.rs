//! Remote writes. The outbox line is on disk before the call. The manifest is written after it.
//! A patch key includes the etag, because a new `If-Match` is a different request.

use super::super::client::AnytypeError;
use super::apply::{commit, Ctx, Job, Outcome};
use super::error::{remote_note, SyncError};
use super::hook::{self, Step};
use super::manifest::Entry;
use super::outbox::{self, Item, Op};
use super::payload::{self, wire_key};
use super::remote;
use super::sha256_hex;
use super::unix_ms;
use serde_json::Value;

const ATTEMPTS: u32 = 3;

pub(super) async fn create(ctx: &mut Ctx<'_>, job: &Job) -> Result<Outcome, SyncError> {
    let Some(bytes) = job.local_bytes.as_deref() else {
        return Ok(Outcome::Failed(format!("{} has no local bytes", job.rel)));
    };
    let sha = sha256_hex(bytes);
    let item = queue(ctx, &job.rel, Op::Create, &sha)?;
    ctx.outbox.save(&outbox_path(ctx)).await?;
    if !hook::gate(ctx.hook, Step::BeforeRemoteCall, &job.rel) {
        return Ok(Outcome::Stopped);
    }
    let posted = send_new(ctx, job, bytes, &item.idem_key, 1).await;
    if !hook::gate(ctx.hook, Step::AfterRemoteCall, &job.rel) {
        return Ok(Outcome::Stopped);
    }
    match posted {
        Ok(wrote) => finish(ctx, job, &sha, &wrote.id, wrote.etag, false, Some(bytes)).await,
        Err(error) => Ok(pause_or(ctx, &item.id, error)),
    }
}

pub(super) async fn update(
    ctx: &mut Ctx<'_>,
    job: &Job,
    deleted: bool,
) -> Result<Outcome, SyncError> {
    let bytes = job.local_bytes.clone().unwrap_or_default();
    let sha = if deleted {
        job.base.clone().unwrap_or_else(|| sha256_hex(&bytes))
    } else {
        sha256_hex(&bytes)
    };
    let op = if deleted { Op::Tombstone } else { Op::Update };
    let item = queue(ctx, &job.rel, op, &sha)?;
    ctx.outbox.save(&outbox_path(ctx)).await?;
    if !hook::gate(ctx.hook, Step::BeforeRemoteCall, &job.rel) {
        return Ok(Outcome::Stopped);
    }
    let result = retry_update(ctx, job, &bytes, &sha, &item, deleted).await;
    if !hook::gate(ctx.hook, Step::AfterRemoteCall, &job.rel) {
        return Ok(Outcome::Stopped);
    }
    match result {
        UpdateEnd::Wrote(wrote) => {
            finish(
                ctx,
                job,
                &sha,
                &wrote.id,
                wrote.etag,
                deleted,
                if deleted {
                    None
                } else {
                    Some(bytes.as_slice())
                },
            )
            .await
        }
        UpdateEnd::Same => {
            let id = job
                .object_id
                .clone()
                .or(wrote_id_from_manifest(ctx, &job.rel));
            let Some(id) = id else {
                return Ok(Outcome::Failed(format!("{} has no object", job.rel)));
            };
            finish(ctx, job, &sha, &id, job.etag.clone(), deleted, None).await
        }
        UpdateEnd::Conflict {
            bytes: remote,
            sha: remote_sha,
        } => super::apply::open_conflict(ctx, job, &sha, &remote_sha, &bytes, &remote).await,
        UpdateEnd::GiveUp(error) => Ok(pause_or(ctx, &item.id, error)),
    }
}

enum UpdateEnd {
    Wrote(Wrote),
    Same,
    Conflict { bytes: Vec<u8>, sha: String },
    GiveUp(AnytypeError),
}

struct Wrote {
    id: String,
    etag: Option<String>,
}

async fn retry_update(
    ctx: &mut Ctx<'_>,
    job: &Job,
    bytes: &[u8],
    sha: &str,
    item: &Item,
    deleted: bool,
) -> UpdateEnd {
    let Some(object_id) = job.object_id.clone() else {
        return match send_new(ctx, job, bytes, &item.idem_key, rev_of(ctx, &job.rel)).await {
            Ok(wrote) => UpdateEnd::Wrote(wrote),
            Err(error) => UpdateEnd::GiveUp(error),
        };
    };
    let mut etag = job.etag.clone();
    let mut restored = false;
    for _ in 0..ATTEMPTS {
        let mut attempt = job.clone();
        attempt.etag = etag.clone();
        let sent = send_existing(
            ctx,
            &attempt,
            bytes,
            sha,
            &item.idem_key,
            &object_id,
            deleted,
        )
        .await;
        match sent {
            Ok(wrote) => return UpdateEnd::Wrote(wrote),
            Err(AnytypeError::EtagMismatch { .. }) => {
                let loaded = match remote::load(ctx.client, &object_id).await {
                    Ok(loaded) => loaded,
                    Err(error) => return UpdateEnd::GiveUp(error),
                };
                if !loaded.deleted && loaded.sha == sha {
                    return UpdateEnd::Same;
                }
                if job.base.as_deref() == Some(loaded.sha.as_str()) && !loaded.deleted {
                    etag = loaded.etag;
                    continue;
                }
                if deleted || loaded.deleted {
                    etag = loaded.etag;
                    continue;
                }
                return UpdateEnd::Conflict {
                    bytes: loaded.bytes,
                    sha: loaded.sha,
                };
            }
            Err(AnytypeError::ObjectArchived) if !restored => {
                restored = true;
                let key = wire_key(
                    &outbox::idem_key(
                        ctx.device_id,
                        &format!("restore:{}", job.rel),
                        Op::Update,
                        sha,
                    ),
                    None,
                );
                if let Err(error) = ctx.client.restore_object(&object_id, &key).await {
                    return UpdateEnd::GiveUp(error);
                }
                etag = None;
            }
            Err(AnytypeError::Gone) => {
                return match send_new(ctx, job, bytes, &item.idem_key, rev_of(ctx, &job.rel)).await
                {
                    Ok(wrote) => UpdateEnd::Wrote(wrote),
                    Err(error) => UpdateEnd::GiveUp(error),
                };
            }
            Err(error) => return UpdateEnd::GiveUp(error),
        }
    }
    UpdateEnd::GiveUp(AnytypeError::EtagMismatch { current: etag })
}

async fn send_new(
    ctx: &Ctx<'_>,
    job: &Job,
    bytes: &[u8],
    logical: &str,
    rev: i64,
) -> Result<Wrote, AnytypeError> {
    let blob = ensure_blob(ctx, &job.rel, bytes).await?;
    let props = payload::file_properties(
        &job.rel,
        bytes,
        false,
        ctx.device_id,
        rev,
        job.mtime_ns.unwrap_or(0),
        blob.as_deref(),
    );
    let created = ctx
        .client
        .create_object(&payload::create_body(props), &wire_key(logical, None))
        .await?;
    Ok(Wrote {
        id: payload::id_of(&created).ok_or(AnytypeError::ApiChanged)?,
        etag: payload::etag_of(&created),
    })
}

async fn send_existing(
    ctx: &Ctx<'_>,
    job: &Job,
    bytes: &[u8],
    sha: &str,
    logical: &str,
    object_id: &str,
    deleted: bool,
) -> Result<Wrote, AnytypeError> {
    let blob = if deleted {
        None
    } else {
        ensure_blob(ctx, &job.rel, bytes).await?
    };
    let props = if deleted {
        payload::file_properties(
            &job.rel,
            &[],
            true,
            ctx.device_id,
            rev_of(ctx, &job.rel),
            0,
            None,
        )
        .tap_sha(sha)
    } else {
        payload::file_properties(
            &job.rel,
            bytes,
            false,
            ctx.device_id,
            rev_of(ctx, &job.rel),
            job.mtime_ns.unwrap_or(0),
            blob.as_deref(),
        )
    };
    let patched = ctx
        .client
        .patch_object(
            object_id,
            &payload::patch_body(props),
            job.etag.as_deref(),
            &wire_key(logical, job.etag.as_deref()),
        )
        .await?;
    Ok(Wrote {
        id: object_id.to_string(),
        etag: payload::etag_of(&patched).or_else(|| job.etag.clone()),
    })
}

async fn ensure_blob(
    ctx: &Ctx<'_>,
    rel: &str,
    bytes: &[u8],
) -> Result<Option<String>, AnytypeError> {
    if payload::is_text(bytes) {
        return Ok(None);
    }
    let sha = sha256_hex(bytes);
    let key = wire_key(&payload::blob_key(ctx.device_id, rel, &sha), None);
    let name = rel.rsplit('/').next().unwrap_or("blob");
    let uploaded = ctx
        .client
        .upload_bytes(name, "application/octet-stream", bytes, &key)
        .await?;
    payload::id_of(&uploaded)
        .map(Some)
        .ok_or(AnytypeError::ApiChanged)
}

async fn finish(
    ctx: &mut Ctx<'_>,
    job: &Job,
    sha: &str,
    object_id: &str,
    etag: Option<String>,
    tombstone: bool,
    bytes: Option<&[u8]>,
) -> Result<Outcome, SyncError> {
    let mut entry = ctx
        .manifest
        .entries
        .get(&job.rel)
        .cloned()
        .unwrap_or_else(Entry::empty);
    entry.base_sha = Some(if tombstone {
        job.base.clone().unwrap_or_else(|| sha.to_string())
    } else {
        sha.to_string()
    });
    entry.object_id = Some(object_id.to_string());
    entry.remote_etag = etag;
    entry.tombstone = tombstone;
    if !tombstone {
        entry.local_size = bytes.map(|bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        entry.local_mtime_ns = job.mtime_ns;
    }
    let rev = rev_of(ctx, &job.rel);
    entry.extra.insert("hb_rev".to_string(), Value::from(rev));
    let outcome = commit(ctx, &job.rel, entry, bytes, true).await?;
    if matches!(outcome, Outcome::Applied) && !tombstone {
        ctx.echo.remember(&job.rel, sha);
    }
    Ok(outcome)
}

fn queue(ctx: &mut Ctx<'_>, rel: &str, op: Op, sha: &str) -> Result<Item, SyncError> {
    let item = Item::new(
        uuid::Uuid::new_v4().to_string(),
        rel,
        op,
        sha,
        ctx.device_id,
    );
    let key = item.idem_key.clone();
    if let Err(error) = ctx.outbox.push(item) {
        return Err(error.into());
    }
    ctx.outbox
        .items()
        .iter()
        .find(|item| item.idem_key == key)
        .cloned()
        .ok_or_else(|| SyncError::Conflict("outbox lost the item just queued".into()))
}

fn pause_or(ctx: &mut Ctx<'_>, id: &str, error: AnytypeError) -> Outcome {
    match error {
        AnytypeError::Unreachable => Outcome::Offline,
        AnytypeError::NotGranted => Outcome::NotGranted,
        AnytypeError::ApiChanged => Outcome::ReadOnly,
        AnytypeError::RateLimited { retry_after } => {
            let millis = i64::try_from(retry_after.as_millis()).unwrap_or(0);
            ctx.outbox.defer(id, unix_ms().saturating_add(millis));
            Outcome::Paused { retry_after }
        }
        other => Outcome::Failed(remote_note(&other)),
    }
}

fn rev_of(ctx: &Ctx<'_>, rel: &str) -> i64 {
    ctx.manifest
        .entries
        .get(rel)
        .and_then(|entry| entry.extra.get("hb_rev"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .saturating_add(1)
}

fn wrote_id_from_manifest(ctx: &Ctx<'_>, rel: &str) -> Option<String> {
    ctx.manifest
        .entries
        .get(rel)
        .and_then(|entry| entry.object_id.clone())
}

fn outbox_path(ctx: &Ctx<'_>) -> std::path::PathBuf {
    ctx.sync_dir.join("outbox.jsonl")
}

pub(super) fn note(error: &AnytypeError) -> Outcome {
    match error {
        AnytypeError::Unreachable => Outcome::Offline,
        AnytypeError::NotGranted => Outcome::NotGranted,
        AnytypeError::ApiChanged => Outcome::ReadOnly,
        AnytypeError::RateLimited { retry_after } => Outcome::Paused {
            retry_after: *retry_after,
        },
        AnytypeError::EtagMismatch { .. } => Outcome::Failed("etag mismatch".into()),
        AnytypeError::ObjectArchived => Outcome::Failed("object archived".into()),
        AnytypeError::Gone => Outcome::Failed("gone".into()),
        AnytypeError::Bad { status, message } => Outcome::Failed(format!("{status} {message}")),
    }
}

trait TapSha {
    fn tap_sha(self, sha: &str) -> Value;
}

impl TapSha for Value {
    fn tap_sha(mut self, sha: &str) -> Value {
        if let Some(object) = self.as_object_mut() {
            object.insert("hb_sha".to_string(), Value::from(sha));
        }
        self
    }
}
