//! Tombstone garbage collection, duplicate `hb_file` cleanup, and manifest compaction.
//! History revisions that aged out are pruned in the same pass.
//!
//! A tombstone older than 30 days is archived by whichever device sees it first.
//! `409 object_archived` means another device already did that, and is success.
//! Two objects with one `hb_path` keep the earliest. An empty earliest object takes
//! the other object's bytes, then the extra object is archived.

use super::super::client::AnytypeError;
use super::engine::Engine;
use super::error::{remote_note, SyncError};
use super::history;
use super::log;
use super::manifest;
use super::outbox::{self, Op};
use super::payload::{self, wire_key};
use super::remote::{self, Listing, RemoteFile};
use super::status::Phase;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) const TOMBSTONE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

impl Engine {
    pub(super) async fn housekeep(&mut self, listing: &Listing) -> Result<(), SyncError> {
        if self.phase == Phase::NeedsImport {
            return Ok(());
        }
        let now = SystemTime::now();
        history::prune_tree(&self.config.sync_dir.join("history"), now)?;
        let mut changed = self.retarget(listing);
        self.archive_tombstones(listing, now).await?;
        self.archive_duplicates(listing).await?;
        if self.compact(listing) {
            changed = true;
        }
        if changed {
            manifest::save(
                &super::paths::manifest_path(&self.config.sync_dir),
                &self.manifest,
            )
            .await?;
        }
        self.duplicates = listing
            .duplicates
            .iter()
            .filter(|dup| match listing.files.get(&dup.path) {
                Some(winner) if winner.object_id != dup.object_id => false,
                Some(_) | None => true,
            })
            .count();
        Ok(())
    }

    fn retarget(&mut self, listing: &Listing) -> bool {
        let mut changed = false;
        for dup in &listing.duplicates {
            let Some(winner) = listing.files.get(&dup.path) else {
                continue;
            };
            let Some(entry) = self.manifest.entries.get_mut(&dup.path) else {
                continue;
            };
            if entry.object_id.as_deref() != Some(dup.object_id.as_str()) {
                continue;
            }
            entry.object_id = Some(winner.object_id.clone());
            entry.remote_etag = None;
            changed = true;
        }
        changed
    }

    async fn archive_tombstones(
        &mut self,
        listing: &Listing,
        now: SystemTime,
    ) -> Result<(), SyncError> {
        let due: Vec<(String, RemoteFile)> = listing
            .files
            .iter()
            .filter(|(path, file)| {
                file.deleted
                    && older_than(file.modified.as_deref(), now, TOMBSTONE_TTL)
                    && !local_file(&self.config.bots_root, path)
                    && !self.conflicts.open_for(path)
                    && !self.outbox.items().iter().any(|item| item.path == **path)
            })
            .map(|(path, file)| (path.clone(), file.clone()))
            .collect();
        for (path, file) in due {
            let key = outbox::idem_key(&self.device_id, &path, Op::Tombstone, "archive");
            match self
                .client
                .archive_object(&file.object_id, &wire_key(&key, None))
                .await
            {
                Ok(_) | Err(AnytypeError::ObjectArchived | AnytypeError::Gone) => {
                    log::decision(
                        &self.config.sync_dir,
                        &path,
                        "archive-tombstone",
                        None,
                        None,
                        Some(file.advisory_sha.as_str()).filter(|sha| !sha.is_empty()),
                    );
                }
                Err(error) => {
                    self.status.last_error = Some(remote_note(&error));
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    async fn archive_duplicates(&mut self, listing: &Listing) -> Result<(), SyncError> {
        for dup in &listing.duplicates {
            let Some(winner) = listing.files.get(&dup.path) else {
                continue;
            };
            if winner.object_id == dup.object_id {
                continue;
            }
            if winner.advisory_sha.is_empty() && !winner.deleted {
                let winner_id = winner.object_id.clone();
                if !self
                    .merge_duplicate(&dup.path, &winner_id, &dup.object_id)
                    .await?
                {
                    continue;
                }
            }
            let key = outbox::idem_key(
                &self.device_id,
                &dup.path,
                Op::Tombstone,
                &format!("dup:{}", dup.object_id),
            );
            match self
                .client
                .archive_object(&dup.object_id, &wire_key(&key, None))
                .await
            {
                Ok(_) | Err(AnytypeError::ObjectArchived | AnytypeError::Gone) => {
                    log::decision(
                        &self.config.sync_dir,
                        &dup.path,
                        "archive-duplicate",
                        None,
                        None,
                        None,
                    );
                }
                Err(error) => {
                    self.status.last_error = Some(remote_note(&error));
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// `Ok(false)` keeps the duplicate: its bytes did not land on the earliest object.
    async fn merge_duplicate(
        &mut self,
        path: &str,
        winner: &str,
        duplicate: &str,
    ) -> Result<bool, SyncError> {
        let loaded = match remote::load(&self.client, duplicate).await {
            Ok(loaded) => loaded,
            Err(AnytypeError::Gone | AnytypeError::ObjectArchived) => return Ok(true),
            Err(error) => {
                self.status.last_error = Some(remote_note(&error));
                return Ok(false);
            }
        };
        if loaded.deleted || loaded.bytes.is_empty() {
            return Ok(true);
        }
        let props =
            payload::file_properties(path, &loaded.bytes, false, &self.device_id, 1, 0, None);
        let key = outbox::idem_key(&self.device_id, path, Op::Update, &loaded.sha);
        match self
            .client
            .patch_object(
                winner,
                &payload::patch_body(props),
                None,
                &wire_key(&key, None),
            )
            .await
        {
            Ok(_) | Err(AnytypeError::ObjectArchived | AnytypeError::Gone) => Ok(true),
            Err(error) => {
                self.status.last_error = Some(remote_note(&error));
                Ok(false)
            }
        }
    }

    fn compact(&mut self, listing: &Listing) -> bool {
        let root = self.config.bots_root.clone();
        let busy: Vec<String> = self
            .outbox
            .items()
            .iter()
            .map(|item| item.path.clone())
            .collect();
        let open: Vec<String> = self
            .conflicts
            .open_items()
            .map(|item| item.path.clone())
            .collect();
        let before = self.manifest.entries.len();
        self.manifest.entries.retain(|path, _entry| {
            if local_file(&root, path)
                || busy.iter().any(|item| item == path)
                || open.contains(path)
            {
                return true;
            }
            match listing.files.get(path) {
                Some(file) if !file.deleted => true,
                Some(_) | None => false,
            }
        });
        before != self.manifest.entries.len()
    }
}

pub(super) fn older_than(modified: Option<&str>, now: SystemTime, ttl: Duration) -> bool {
    let Some(stamp) = modified else {
        return false;
    };
    let Some(then) = parse_rfc3339(stamp) else {
        return false;
    };
    now.duration_since(then).is_ok_and(|age| age > ttl)
}

fn local_file(root: &Path, rel: &str) -> bool {
    std::fs::symlink_metadata(root.join(rel))
        .map(|meta| meta.is_file())
        .unwrap_or(false)
}

fn parse_rfc3339(text: &str) -> Option<SystemTime> {
    let (date, time) = text.split_once('T')?;
    let mut dates = date.split('-');
    let year: i64 = dates.next()?.parse().ok()?;
    let month: u32 = dates.next()?.parse().ok()?;
    let day: u32 = dates.next()?.parse().ok()?;
    if dates.next().is_some() {
        return None;
    }
    let time = time.trim_end_matches('Z');
    let mut parts = time.split(':');
    let hour: u64 = parts.next()?.parse().ok()?;
    let minute: u64 = parts.next()?.parse().ok()?;
    let second: u64 = parts.next()?.split('.').next()?.parse().ok()?;
    if parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    let secs = days
        .checked_mul(86_400)?
        .checked_add(i64::try_from(hour * 3600 + minute * 60 + second).ok()?)?;
    if secs < 0 {
        return None;
    }
    UNIX_EPOCH.checked_add(Duration::from_secs(u64::try_from(secs).ok()?))
}

/// Howard Hinnant's `days_from_civil`. Days since the Unix epoch.
fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || day == 0 || day > 31 {
        return None;
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = u64::try_from(year - era * 400).ok()?;
    let month_index: u64 = if month > 2 {
        u64::from(month - 3)
    } else {
        u64::from(month + 9)
    };
    let doy = (153 * month_index + 2) / 5 + u64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + i64::try_from(doe).ok()? - 719_468)
}
