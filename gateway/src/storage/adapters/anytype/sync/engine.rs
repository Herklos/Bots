//! One reconcile cycle: scan, list, decide, apply. Cycles do not overlap; the caller
//! holds the engine. A stop from the hook reloads disk, the same as a restarted process.

use super::super::client::AnytypeClient;
use super::apply::{self, Ctx, Job, Keep, Outcome};
use super::conflict;
use super::echo::EchoBook;
use super::error::{remote_note, SyncError};
use super::hook::{self, Hook};
use super::manifest::{self, Identity, Loaded, Manifest};
use super::outbox::Outbox;
use super::paths::{device_path, manifest_path, outbox_path};
use super::plan::{self, Planned};
use super::remote;
use super::scan::{self, Scan, Shortcut};
use super::status::{Phase, Preview, SyncState, SyncStatus};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone)]
pub(super) struct Config {
    pub host: String,
    pub key: String,
    pub space_id: String,
    pub bots_root: PathBuf,
    pub sync_dir: PathBuf,
    pub device_id: String,
    pub write_rate: f64,
    pub write_burst: f64,
    pub max_writes: usize,
    pub settle: Duration,
    pub hash_budget: Duration,
    pub backoff_floor: Duration,
    pub max_passes: usize,
}

impl Config {
    pub(super) fn new(
        host: impl Into<String>,
        key: impl Into<String>,
        space_id: impl Into<String>,
        bots_root: PathBuf,
        sync_dir: PathBuf,
        device_id: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            key: key.into(),
            space_id: space_id.into(),
            bots_root,
            sync_dir,
            device_id: device_id.into(),
            write_rate: 0.8,
            write_burst: 40.0,
            max_writes: 50,
            settle: Duration::from_secs(1),
            hash_budget: Duration::from_millis(200),
            backoff_floor: Duration::from_secs(1),
            max_passes: 8,
        }
    }
}

#[derive(Debug)]
pub(super) struct Cycle {
    pub applied: usize,
    pub stopped: bool,
    pub deferred: bool,
    pub status: SyncStatus,
}

pub(super) struct Engine {
    pub(super) config: Config,
    pub(super) client: AnytypeClient,
    pub(super) manifest: Manifest,
    pub(super) outbox: Outbox,
    pub(super) conflicts: conflict::Conflicts,
    pub(super) device_id: String,
    pub(super) phase: Phase,
    pub(super) hook: Box<dyn Hook>,
    pub(super) status: SyncStatus,
    pub(super) pause_until: Option<Instant>,
    pub(super) backoff: Duration,
    pub(super) nudged: BTreeSet<String>,
    pub(super) duplicates: usize,
    pub(super) echo: EchoBook,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("phase", &self.phase)
            .field("device_id", &self.manifest.device_id)
            .finish_non_exhaustive()
    }
}

impl Engine {
    pub(super) fn open(config: Config) -> Result<Self, SyncError> {
        std::fs::create_dir_all(&config.bots_root)?;
        std::fs::create_dir_all(&config.sync_dir)?;
        let device_id = load_device(&config);
        std::fs::write(device_path(&config.sync_dir), format!("{device_id}\n"))?;
        let identity = Identity {
            host: config.host.clone(),
            space_id: config.space_id.clone(),
            root: config.bots_root.display().to_string(),
        };
        let manifest = match manifest::load(&manifest_path(&config.sync_dir), &identity)? {
            Loaded::Missing => Manifest::fresh(&identity, &device_id),
            Loaded::Current(manifest) | Loaded::Reset(manifest) => manifest,
        };
        let phase = phase_of(&manifest);
        let client = AnytypeClient::with_rate(
            &config.host,
            &config.key,
            &config.space_id,
            config.write_rate,
            config.write_burst,
        )
        .map_err(|error| SyncError::Conflict(remote_note(&error)))?;
        let outbox = Outbox::load(&outbox_path(&config.sync_dir))?;
        let conflicts = conflict::Conflicts::load(&conflict::conflicts_path(&config.sync_dir))?;
        let status = SyncStatus::fresh(phase);
        Ok(Self {
            config,
            client,
            manifest,
            outbox,
            conflicts,
            device_id,
            phase,
            hook: Box::new(|_step: hook::Step, _path: &str| hook::Control::Continue),
            status,
            pause_until: None,
            backoff: Duration::from_secs(2),
            nudged: BTreeSet::new(),
            duplicates: 0,
            echo: EchoBook::default(),
        })
    }

    pub(super) fn status(&self) -> SyncStatus {
        let mut status = self.status.clone();
        status.phase = self.phase;
        status.outbox = self.outbox.items().len();
        status.outbox_warn = status.outbox >= super::outbox::WARN_AT;
        status.conflicts = self.conflicts.open_count();
        status.duplicates = self.duplicates;
        if status.state != SyncState::Offline
            && status.state != SyncState::NotGranted
            && status.state != SyncState::ApiChanged
            && status.state != SyncState::NotFound
        {
            status.state = SyncState::from_phase(self.phase);
        }
        status
    }

    pub(super) fn phase(&self) -> Phase {
        self.phase
    }

    pub(super) fn write_rate(&self) -> f64 {
        self.client.rate()
    }

    pub(super) fn set_hook(&mut self, hook: Box<dyn Hook>) {
        self.hook = hook;
    }

    pub(super) fn nudge(&mut self, rel: &str) {
        self.nudged.insert(rel.to_string());
    }

    pub(super) fn conflict_for(&self, path: &str) -> Option<String> {
        self.conflicts
            .open_items()
            .find(|item| item.path == path)
            .map(|item| item.id.clone())
    }

    pub(super) async fn dry_run(&mut self) -> Result<Preview, SyncError> {
        let planned = self.collect(true).await?;
        let preview = plan::tally(&planned.jobs, self.client.rate());
        self.status.preview = Some(preview.clone());
        Ok(preview)
    }

    /// Flip the first-sync phase to `importing` once, so plain cycles know to write.
    /// The runtime calls this before running the confirmed import's passes.
    pub(super) async fn begin_import(&mut self) -> Result<(), SyncError> {
        if self.phase == Phase::NeedsImport {
            self.phase = Phase::Importing;
            self.store_phase();
            manifest::save(&manifest_path(&self.config.sync_dir), &self.manifest).await?;
        }
        Ok(())
    }

    pub(super) async fn confirm_import(&mut self) -> Result<Cycle, SyncError> {
        self.begin_import().await?;
        let mut last = self.run_once().await?;
        for _ in 1..self.config.max_passes.max(1) {
            let blocked = matches!(
                last.status.state,
                SyncState::Offline | SyncState::NotGranted | SyncState::NotFound
            );
            if last.stopped || blocked || (last.applied == 0 && !last.deferred) {
                break;
            }
            last = self.run_once().await?;
        }
        Ok(last)
    }

    pub(super) async fn resolve(&mut self, id: &str, keep: Keep) -> Result<Cycle, SyncError> {
        let outcome = {
            let mut ctx = self.ctx();
            apply::resolve(&mut ctx, id, keep).await?
        };
        self.note(&outcome, false);
        if matches!(outcome, Outcome::Stopped) {
            self.reload()?;
        }
        Ok(self.cycle(0, matches!(outcome, Outcome::Stopped), false))
    }

    pub(super) async fn run_once(&mut self) -> Result<Cycle, SyncError> {
        if let Some(until) = self.pause_until {
            if Instant::now() < until {
                self.status.retry_in = until.saturating_duration_since(Instant::now());
                self.status.state = SyncState::from_phase(self.phase);
                return Ok(self.cycle(0, false, false));
            }
            self.pause_until = None;
        }
        let scan = scan::scan_budget(
            &self.config.bots_root,
            &cache_of(&self.manifest),
            SystemTime::now(),
            self.config.settle,
            Some(self.config.hash_budget),
        )?;
        let listing = match remote::list(&self.client).await {
            Ok(listing) => listing,
            Err(error) => return self.list_failed(&scan, error).await,
        };
        self.backoff = Duration::from_secs(2);
        self.duplicates = listing.duplicates.len();
        self.absorb_conflicts(&listing).await?;
        if self.phase == Phase::NeedsImport {
            let planned = self.collect_with(&scan, &listing).await?;
            if planned.stopped {
                self.reload()?;
                return Ok(self.cycle(0, true, false));
            }
            self.status.preview = Some(plan::tally(&planned.jobs, self.client.rate()));
            self.status.state = SyncState::NeedsImport;
            self.status.last_error = None;
            return Ok(self.cycle(0, false, false));
        }
        let planned = self.collect_with(&scan, &listing).await?;
        if planned.stopped {
            self.reload()?;
            return Ok(self.cycle(0, true, false));
        }
        self.apply_jobs(planned.jobs).await
    }

    fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            client: &self.client,
            root: &self.config.bots_root,
            sync_dir: &self.config.sync_dir,
            device_id: &self.device_id,
            manifest: &mut self.manifest,
            outbox: &mut self.outbox,
            conflicts: &mut self.conflicts,
            hook: &mut *self.hook,
            echo: &mut self.echo,
        }
    }

    async fn apply_jobs(&mut self, jobs: Vec<Job>) -> Result<Cycle, SyncError> {
        let mut applied = 0_usize;
        let mut writes = 0_usize;
        let mut deferred = false;
        let mut read_only = false;
        for job in jobs {
            if plan::is_push(job.action) && (writes >= self.config.max_writes || read_only) {
                deferred = !read_only;
                continue;
            }
            if plan::is_push(job.action) {
                writes += 1;
            }
            let outcome = {
                let mut ctx = self.ctx();
                apply::perform(&mut ctx, &job).await?
            };
            match &outcome {
                Outcome::Applied => applied += 1,
                Outcome::Idle | Outcome::Failed(_) => {}
                Outcome::Stopped => {
                    self.reload()?;
                    return Ok(self.cycle(applied, true, deferred));
                }
                Outcome::Paused { retry_after } => {
                    self.arm_pause(*retry_after);
                    self.outbox
                        .save(&outbox_path(&self.config.sync_dir))
                        .await?;
                    return Ok(self.cycle(applied, false, true));
                }
                Outcome::Offline | Outcome::NotGranted | Outcome::ReadOnly => {
                    self.note(&outcome, false);
                    if matches!(outcome, Outcome::ReadOnly) {
                        read_only = true;
                        continue;
                    }
                    return Ok(self.cycle(applied, false, deferred));
                }
            }
            self.note(&outcome, false);
        }
        self.nudged.clear();
        if self.phase == Phase::Importing
            && applied == 0
            && !deferred
            && self.outbox.items().is_empty()
        {
            self.phase = Phase::Live;
            self.store_phase();
            manifest::save(&manifest_path(&self.config.sync_dir), &self.manifest).await?;
        }
        self.status.state = SyncState::from_phase(self.phase);
        self.status.last_error = None;
        self.status.retry_in = Duration::ZERO;
        Ok(self.cycle(applied, false, deferred))
    }

    async fn collect(&mut self, _preview: bool) -> Result<Collected, SyncError> {
        let scan = scan::scan_budget(
            &self.config.bots_root,
            &cache_of(&self.manifest),
            SystemTime::now(),
            self.config.settle,
            Some(self.config.hash_budget),
        )?;
        let listing = remote::list(&self.client)
            .await
            .map_err(|error| SyncError::Conflict(remote_note(&error)))?;
        self.duplicates = listing.duplicates.len();
        self.collect_with(&scan, &listing).await
    }

    async fn collect_with(
        &mut self,
        scan: &Scan,
        listing: &remote::Listing,
    ) -> Result<Collected, SyncError> {
        let mut paths = BTreeSet::new();
        paths.extend(scan.files.keys().cloned());
        paths.extend(listing.files.keys().cloned());
        paths.extend(self.manifest.entries.keys().cloned());
        for unsettled in &scan.unsettled {
            paths.remove(unsettled);
        }
        paths.retain(|path| !self.conflicts.open_for(path));
        let nudged = self.nudged.clone();
        let mut paths: Vec<String> = paths.into_iter().collect();
        paths.sort_by(|left, right| {
            nudged
                .contains(right)
                .cmp(&nudged.contains(left))
                .then(plan::rank(left).cmp(&plan::rank(right)))
        });
        let mut jobs = Vec::new();
        for rel in paths {
            let planned = {
                let mut ctx = self.ctx();
                plan::one(&mut ctx, &rel, scan, listing, scan.truncated).await?
            };
            match planned {
                Planned::Ready(job) => jobs.push(*job),
                Planned::Skip => {}
                Planned::Stop => {
                    return Ok(Collected {
                        jobs,
                        stopped: true,
                    })
                }
                Planned::Halt(outcome) => {
                    self.note(&outcome, false);
                    if matches!(
                        outcome,
                        Outcome::Offline | Outcome::NotGranted | Outcome::Paused { .. }
                    ) {
                        return Ok(Collected {
                            jobs,
                            stopped: false,
                        });
                    }
                }
            }
        }
        Ok(Collected {
            jobs,
            stopped: false,
        })
    }

    fn store_phase(&mut self) {
        self.manifest.extra.insert(
            "phase".to_string(),
            serde_json::Value::from(self.phase.as_str()),
        );
    }

    fn reload(&mut self) -> Result<(), SyncError> {
        let identity = Identity {
            host: self.config.host.clone(),
            space_id: self.config.space_id.clone(),
            root: self.config.bots_root.display().to_string(),
        };
        self.manifest = match manifest::load(&manifest_path(&self.config.sync_dir), &identity)? {
            Loaded::Missing => Manifest::fresh(&identity, &self.manifest.device_id),
            Loaded::Current(manifest) | Loaded::Reset(manifest) => manifest,
        };
        self.device_id = self.manifest.device_id.clone();
        self.phase = phase_of(&self.manifest);
        self.outbox = Outbox::load(&outbox_path(&self.config.sync_dir))?;
        self.conflicts =
            conflict::Conflicts::load(&conflict::conflicts_path(&self.config.sync_dir))?;
        Ok(())
    }

    pub(super) fn cycle(&self, applied: usize, stopped: bool, deferred: bool) -> Cycle {
        Cycle {
            applied,
            stopped,
            deferred,
            status: self.status(),
        }
    }
}

struct Collected {
    jobs: Vec<Job>,
    stopped: bool,
}

fn phase_of(manifest: &Manifest) -> Phase {
    match manifest.extra.get("phase").and_then(|value| value.as_str()) {
        Some("importing") => Phase::Importing,
        Some("live") => Phase::Live,
        _ => Phase::NeedsImport,
    }
}

fn cache_of(manifest: &Manifest) -> BTreeMap<String, Shortcut> {
    manifest
        .entries
        .iter()
        .filter(|(_, entry)| !entry.tombstone)
        .filter_map(|(path, entry)| {
            Some((
                path.clone(),
                Shortcut {
                    size: entry.local_size?,
                    mtime_ns: entry.local_mtime_ns?,
                    sha: entry.base_sha.clone()?,
                },
            ))
        })
        .collect()
}

fn load_device(config: &Config) -> String {
    let path = device_path(&config.sync_dir);
    if let Ok(text) = std::fs::read_to_string(&path) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if !config.device_id.is_empty() {
        return config.device_id.clone();
    }
    uuid::Uuid::new_v4().to_string()
}
