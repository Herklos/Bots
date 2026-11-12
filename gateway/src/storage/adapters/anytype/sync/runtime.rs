//! The engine as the gateway runs it. One background task owns the engine, keeps a
//! snapshot the API reads without touching the engine, and answers through a channel.
//! `storage::sync::start` decides whether this runtime exists at all.

use super::super::client::{AnytypeClient, AnytypeError};
use super::apply::Keep;
use super::device::{self, Peer};
use super::engine::{Config, Cycle, Engine};
use super::status::{self, Preview, SyncState};
use super::unix_ms;
use crate::storage::sync::Nudges;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// What the runtime needs. The key never appears in a snapshot.
#[derive(Debug, Clone)]
pub(crate) struct Materials {
    pub host: String,
    pub key: String,
    pub space_id: String,
    pub bots_root: PathBuf,
    pub sync_dir: PathBuf,
    pub hostname: String,
    pub version: String,
}

/// What the gateway asks the running engine to do.
pub(crate) enum Request {
    /// One cycle now (Sync now, or a nudge).
    Run,
    /// The confirmed first import.
    Import,
    /// The import dry run. The reply carries the preview or the reason it failed.
    DryRun(oneshot::Sender<Result<Value, String>>),
    /// Keep mine, Keep Anytype, or Keep both, applied to one conflict.
    Resolve(String, Keep, oneshot::Sender<Result<(), String>>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreviewJson {
    pub push: usize,
    pub pull: usize,
    pub conflict: usize,
    pub estimate_secs: u64,
}

/// One `hb_device` row, this machine or another gateway.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Device {
    pub slug: String,
    pub hostname: String,
    pub version: String,
    pub status: String,
    pub last_seen: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Snapshot {
    pub phase: &'static str,
    pub state: &'static str,
    pub last_sync: i64,
    pub retry_in: u64,
    pub outbox: usize,
    pub duplicates: usize,
    pub device_id: String,
    pub conflicts: Vec<(String, String)>,
    pub devices: Vec<Device>,
    pub last_error: Option<String>,
    pub preview: Option<PreviewJson>,
}

impl Snapshot {
    fn starting() -> Self {
        Self {
            phase: "needs_import",
            state: "offline",
            last_sync: 0,
            retry_in: 0,
            outbox: 0,
            duplicates: 0,
            device_id: String::new(),
            conflicts: Vec::new(),
            devices: Vec::new(),
            last_error: None,
            preview: None,
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "available": true,
            "running": true,
            "state": self.state,
            "phase": self.phase,
            "lastSync": self.last_sync,
            "retryIn": self.retry_in,
            "outbox": self.outbox,
            "duplicates": self.duplicates,
            "deviceId": self.device_id,
            "conflicts": self
                .conflicts
                .iter()
                .map(|(id, path)| json!({ "id": id, "path": path }))
                .collect::<Vec<Value>>(),
            "devices": self
                .devices
                .iter()
                .map(|device| {
                    json!({
                        "slug": device.slug,
                        "hostname": device.hostname,
                        "version": device.version,
                        "status": device.status,
                        "lastSeen": device.last_seen,
                        "lagSecs": (unix_ms() - device.last_seen).max(0) / 1000,
                    })
                })
                .collect::<Vec<Value>>(),
            "error": self.last_error,
            "preview": match &self.preview {
                None => Value::Null,
                Some(PreviewJson { push, pull, conflict, estimate_secs }) => json!({
                    "push": push,
                    "pull": pull,
                    "conflict": conflict,
                    "estimateSecs": estimate_secs,
                }),
            },
        })
    }
}

struct Inner {
    tx: mpsc::UnboundedSender<Request>,
    snapshot: std::sync::RwLock<Snapshot>,
}

impl Inner {
    fn set_snapshot(&self, snapshot: Snapshot) {
        *self.snapshot.write().expect("sync snapshot lock") = snapshot;
    }
}

/// The handle the gateway keeps. Cheap to clone and safe across tasks.
#[derive(Clone)]
pub(crate) struct SyncRuntime {
    inner: std::sync::Arc<Inner>,
}

impl SyncRuntime {
    /// The latest snapshot. Never waits on the engine.
    pub(crate) fn snapshot(&self) -> Snapshot {
        self.inner
            .snapshot
            .read()
            .expect("sync snapshot lock")
            .clone()
    }

    /// A write or a remove under the Bots root asks for an early cycle. The queue is
    /// capped: dropped nudges only delay one scan, never lose a change.
    pub(crate) fn run_now(&self) {
        let _ = self.inner.tx.send(Request::Run);
    }

    pub(crate) fn start_import(&self) {
        let _ = self.inner.tx.send(Request::Import);
    }

    pub(crate) async fn dry_run(&self) -> Result<Value, String> {
        let (reply, wait) = oneshot::channel();
        self.inner
            .tx
            .send(Request::DryRun(reply))
            .map_err(|_dropped: _| "The sync engine is not running.".to_string())?;
        wait.await
            .map_err(|_gone: _| "The sync engine is not running.".to_string())?
    }

    pub(crate) async fn resolve(&self, id: &str, keep: Keep) -> Result<(), String> {
        let (reply, wait) = oneshot::channel();
        self.inner
            .tx
            .send(Request::Resolve(id.to_string(), keep, reply))
            .map_err(|_dropped: _| "The sync engine is not running.".to_string())?;
        wait.await
            .map_err(|_gone: _| "The sync engine is not running.".to_string())?
    }
}

/// Start the background task. Opening the engine never talks to Anytype, so this
/// cannot fail: a bad key or host shows up later as the engine's own offline state.
pub(crate) fn start(materials: Materials, nudges: Arc<Nudges>) -> SyncRuntime {
    let (tx, rx) = mpsc::unbounded_channel();
    let runtime = SyncRuntime {
        inner: Arc::new(Inner {
            tx,
            snapshot: std::sync::RwLock::new(Snapshot::starting()),
        }),
    };
    let inner = Arc::clone(&runtime.inner);
    tokio::spawn(run_loop(materials, inner, nudges, rx));
    runtime
}

async fn run_loop(
    materials: Materials,
    inner: Arc<Inner>,
    nudges: Arc<Nudges>,
    mut rx: mpsc::UnboundedReceiver<Request>,
) {
    let config = Config::new(
        materials.host.clone(),
        materials.key.clone(),
        materials.space_id.clone(),
        materials.bots_root.clone(),
        materials.sync_dir.clone(),
        "",
    );
    let mut engine = match Engine::open(config) {
        Ok(engine) => engine,
        Err(error) => {
            tracing::warn!("Anytype sync is off: the sync state could not be opened ({error}).");
            return;
        }
    };
    let device_id = engine.manifest.device_id.clone();
    let mut snapshot = Snapshot::starting();
    snapshot.device_id = device_id.clone();
    write_snapshot(&mut snapshot, &engine);
    inner.set_snapshot(snapshot);
    let devices_client = AnytypeClient::new(
        materials.host.clone(),
        materials.key.clone(),
        materials.space_id.clone(),
    )
    .ok();
    let mut last_heartbeat: i64 = 0;
    let mut backoff = Duration::from_secs(2);
    let mut want_import = false;
    loop {
        // Nudges and requests first: they are the reason this cycle runs early.
        for rel in nudges.drain() {
            engine.nudge(&rel);
        }
        while let Ok(request) = rx.try_recv() {
            if serve(&mut engine, request).await {
                want_import = true;
            }
        }
        // One cycle. An import first flips the phase, then runs the same passes.
        let outcome = if want_import {
            want_import = false;
            run_import(&mut engine).await
        } else {
            engine.run_once().await
        };
        let mut snapshot = {
            let mut snapshot = inner.snapshot.read().expect("sync snapshot lock").clone();
            write_snapshot(&mut snapshot, &engine);
            snapshot
        };
        if let Ok(cycle) = &outcome {
            let linked = matches!(
                cycle.status.state,
                SyncState::Live | SyncState::Importing | SyncState::NeedsImport
            ) && snapshot.last_error.is_none();
            if linked {
                snapshot.last_sync = unix_ms();
            }
        }
        if let Err(error) = &outcome {
            tracing::warn!("Anytype sync cycle failed: {error}");
            backoff = status::grow_backoff(backoff);
        } else {
            backoff = Duration::from_secs(2);
        }
        // The device heartbeat and the peer list, every 60 seconds. It only logs.
        if unix_ms().saturating_sub(last_heartbeat) >= 60_000 {
            last_heartbeat = unix_ms();
            if let Some(client) = devices_client.as_ref() {
                match heartbeat(client, &device_id, &materials).await {
                    Ok(devices) => snapshot.devices = devices,
                    Err(error) => tracing::debug!("Anytype device heartbeat failed: {error:?}"),
                }
            }
        }
        inner.set_snapshot(snapshot);
        let state = engine.status().state;
        let offline = matches!(
            state,
            SyncState::Offline
                | SyncState::NotGranted
                | SyncState::ApiChanged
                | SyncState::NotFound
        ) || outcome.is_err();
        let wait = status::next_wait(offline, backoff, false);
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            Some(request) = rx.recv() => {
                if serve(&mut engine, request).await {
                    want_import = true;
                }
            }
            _ = nudges.notified() => {}
        }
    }
}

/// Answers one request. True when it was the import order.
async fn serve(engine: &mut Engine, request: Request) -> bool {
    match request {
        Request::Run => {}
        Request::Import => return true,
        Request::DryRun(reply) => {
            let result = engine
                .dry_run()
                .await
                .map(|preview| {
                    let PreviewJson {
                        push,
                        pull,
                        conflict,
                        estimate_secs,
                    } = preview_json(&preview);
                    json!({
                        "push": push,
                        "pull": pull,
                        "conflict": conflict,
                        "estimateSecs": estimate_secs,
                    })
                })
                .map_err(|error| error.to_string());
            let _ = reply.send(result);
        }
        Request::Resolve(id, keep, reply) => {
            let result = engine
                .resolve(&id, keep)
                .await
                .map(|_| ())
                .map_err(|error| error.to_string());
            let _ = reply.send(result);
        }
    }
    false
}

/// The confirmed first import: flip the phase, then run passes until the tree is up.
async fn run_import(engine: &mut Engine) -> Result<Cycle, super::error::SyncError> {
    engine.begin_import().await?;
    let mut last = engine.run_once().await?;
    for _ in 1..engine.config.max_passes.max(1) {
        let blocked = matches!(
            last.status.state,
            SyncState::Offline | SyncState::NotGranted | SyncState::NotFound
        );
        if last.stopped || blocked || (last.applied == 0 && !last.deferred) {
            break;
        }
        last = engine.run_once().await?;
    }
    Ok(last)
}

async fn heartbeat(
    client: &AnytypeClient,
    device_id: &str,
    materials: &Materials,
) -> Result<Vec<Device>, AnytypeError> {
    device::register(client, device_id, &materials.hostname, &materials.version).await?;
    let peers = device::peers(client, device_id).await?;
    Ok(peers
        .into_iter()
        .map(
            |Peer {
                 slug,
                 hostname,
                 version,
                 status,
                 last_seen,
             }| Device {
                slug,
                hostname,
                version,
                status,
                last_seen,
            },
        )
        .collect())
}

/// Copy the engine's status into the snapshot, keeping the device list and last sync.
fn write_snapshot(snapshot: &mut Snapshot, engine: &Engine) {
    let status = engine.status();
    snapshot.phase = engine.phase().as_str();
    snapshot.state = status.state.as_str();
    snapshot.retry_in = status.retry_in.as_secs();
    snapshot.outbox = status.outbox;
    snapshot.duplicates = status.duplicates;
    snapshot.last_error = status.last_error.clone();
    snapshot.conflicts = engine
        .conflicts
        .open_items()
        .map(|record| (record.id.clone(), record.path.clone()))
        .collect();
    snapshot.preview = status.preview.map(|preview| preview_json(&preview));
}

fn preview_json(preview: &Preview) -> PreviewJson {
    PreviewJson {
        push: preview.push,
        pull: preview.pull,
        conflict: preview.conflict,
        estimate_secs: preview.estimate.as_secs(),
    }
}
