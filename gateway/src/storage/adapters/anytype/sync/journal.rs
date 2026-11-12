//! Offline outbox capture, remote conflict absorption, and the pause that follows a 429.

use super::super::client::AnytypeError;
use super::apply::Outcome;
use super::conflict::{self, ConflictState};
use super::engine::Engine;
use super::error::{remote_note, SyncError};
use super::outbox::{Item, Op};
use super::paths::outbox_path;
use super::scan::Scan;
use super::status::{self, Phase, SyncState};
use super::unix_ms;
use std::time::Duration;

impl Engine {
    pub(super) async fn list_failed(
        &mut self,
        scan: &Scan,
        error: AnytypeError,
    ) -> Result<super::engine::Cycle, SyncError> {
        let outcome = match &error {
            AnytypeError::Unreachable => {
                if self.phase != Phase::NeedsImport {
                    self.queue_offline(scan);
                    self.outbox
                        .save(&outbox_path(&self.config.sync_dir))
                        .await?;
                }
                self.backoff = status::grow_backoff(self.backoff);
                Outcome::Offline
            }
            AnytypeError::NotGranted => Outcome::NotGranted,
            AnytypeError::Gone => Outcome::Failed("not found".into()),
            AnytypeError::ApiChanged => Outcome::ReadOnly,
            AnytypeError::RateLimited { retry_after } => Outcome::Paused {
                retry_after: *retry_after,
            },
            other => Outcome::Failed(remote_note(other)),
        };
        if matches!(outcome, Outcome::Failed(ref message) if message == "not found") {
            self.status.state = SyncState::NotFound;
        }
        self.note(&outcome, true);
        Ok(self.cycle(0, false, false))
    }

    pub(super) fn queue_offline(&mut self, scan: &Scan) {
        let device = self.manifest.device_id.clone();
        for (rel, present) in &scan.files {
            let base = self
                .manifest
                .entries
                .get(rel)
                .and_then(|entry| entry.base_sha.as_deref());
            if base == Some(present.sha.as_str()) {
                continue;
            }
            let op = if self
                .manifest
                .entries
                .get(rel)
                .and_then(|entry| entry.object_id.as_ref())
                .is_some()
            {
                Op::Update
            } else {
                Op::Create
            };
            let item = Item::new(
                uuid::Uuid::new_v4().to_string(),
                rel,
                op,
                &present.sha,
                &device,
            );
            let _ = self.outbox.push(item);
        }
        if scan.truncated {
            return;
        }
        let missing: Vec<String> = self
            .manifest
            .entries
            .iter()
            .filter(|(rel, entry)| {
                entry.object_id.is_some()
                    && !scan.files.contains_key(*rel)
                    && !scan.unsettled.contains(*rel)
            })
            .map(|(rel, _)| rel.clone())
            .collect();
        for rel in missing {
            let sha = self
                .manifest
                .entries
                .get(&rel)
                .and_then(|entry| entry.base_sha.clone())
                .unwrap_or_default();
            let item = Item::new(
                uuid::Uuid::new_v4().to_string(),
                &rel,
                Op::Tombstone,
                &sha,
                &device,
            );
            let _ = self.outbox.push(item);
        }
    }

    pub(super) async fn absorb_conflicts(
        &mut self,
        listing: &super::remote::Listing,
    ) -> Result<(), SyncError> {
        let mut changed = false;
        let known: Vec<(String, String)> = listing
            .conflicts
            .iter()
            .map(|(path, conflict)| (path.clone(), conflict.state.clone()))
            .collect();
        for (path, state) in known {
            match conflict::state_of(&state) {
                ConflictState::Open => {
                    if !self.conflicts.open_for(&path) {
                        let record = conflict::record_for(&path, None, "", "", "");
                        self.conflicts.insert(record);
                        changed = true;
                    }
                }
                ConflictState::Resolved | ConflictState::Dismissed => {
                    if let Some(record) = self.conflicts.remove_path(&path) {
                        if !record.sidecar.is_empty() {
                            let _ =
                                std::fs::remove_file(self.config.bots_root.join(&record.sidecar));
                        }
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.conflicts
                .save(&conflict::conflicts_path(&self.config.sync_dir))
                .await?;
        }
        Ok(())
    }

    pub(super) fn note(&mut self, outcome: &Outcome, from_list: bool) {
        match outcome {
            Outcome::Offline => {
                self.status.state = SyncState::Offline;
                self.status.last_error = Some("unreachable".into());
                self.status.retry_in = self.backoff;
            }
            Outcome::NotGranted => {
                self.status.state = SyncState::NotGranted;
                self.status.last_error = Some("not granted".into());
            }
            Outcome::ReadOnly => {
                self.status.state = SyncState::ApiChanged;
                self.status.last_error = Some("api changed".into());
            }
            Outcome::Paused { retry_after } => {
                if from_list {
                    self.arm_pause(*retry_after);
                }
                self.status.last_error = Some("rate limited".into());
            }
            Outcome::Failed(message) => {
                if message == "not found" {
                    self.status.state = SyncState::NotFound;
                }
                self.status.last_error = Some(message.clone());
            }
            Outcome::Applied | Outcome::Idle | Outcome::Stopped => {}
        }
    }

    pub(super) fn arm_pause(&mut self, retry_after: Duration) {
        let wait = retry_after
            .max(self.config.backoff_floor)
            .max(Duration::from_millis(1));
        self.pause_until = Some(std::time::Instant::now() + wait);
        self.status.retry_in = wait;
        let extra = i64::try_from(wait.as_millis()).unwrap_or(0);
        let when = unix_ms().saturating_add(extra);
        let ids: Vec<String> = self
            .outbox
            .items()
            .iter()
            .map(|item| item.id.clone())
            .collect();
        for id in ids {
            self.outbox.defer(&id, when);
        }
    }
}
