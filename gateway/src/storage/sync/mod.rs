//! The sync engine as the gateway runs it: `start` decides whether it exists, and the
//! `SyncHandle` is what Settings and the API read. Everything Anytype-specific lives
//! in the adapter; this module wires it to the saved storage choice.
//!
//! `start` never fails the gateway: a bad host, a missing key, or a missing space
//! logs a warning and leaves the engine off, with the reason on the handle.

use crate::api::AppState;
use crate::storage::adapters::anytype::sync::{start_runtime, Materials, SyncRuntime};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Notify;

pub(crate) use crate::storage::adapters::anytype::sync::Keep;

/// The queue the [`crate::storage::adapters::anytype::SyncedWorkspace`] feeds and the
/// engine drains. Bounded: a dropped nudge only delays one scan.
#[derive(Default)]
pub(crate) struct Nudges {
    queue: std::sync::Mutex<Vec<String>>,
    notify: Notify,
}

impl std::fmt::Debug for Nudges {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Nudges").finish_non_exhaustive()
    }
}

impl Nudges {
    /// One entry per write or remove. Duplicates collapse; the cap keeps this bounded.
    pub(crate) fn push(&self, rel: &str) {
        if let Ok(mut queue) = self.queue.try_lock() {
            if queue.len() < 1024 && !queue.iter().any(|path| path == rel) {
                queue.push(rel.to_string());
            }
        }
        self.notify.notify_waiters();
    }

    pub(crate) fn drain(&self) -> Vec<String> {
        let mut queue = self.queue.lock().expect("sync nudge lock");
        std::mem::take(&mut *queue)
    }

    pub(crate) async fn notified(&self) {
        self.notify.notified().await;
    }
}

/// What `AppState` holds. The engine starts after `AppState` is built; the slot is set
/// right after, and nothing else mutates it.
#[derive(Default)]
pub(crate) struct SyncSlot(std::sync::Mutex<Option<Arc<SyncHandle>>>);

impl std::fmt::Debug for SyncSlot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let running = self.get().is_some_and(|handle| handle.running());
        formatter
            .debug_struct("SyncSlot")
            .field("running", &running)
            .finish()
    }
}

impl SyncSlot {
    pub(crate) fn set(&self, handle: Option<Arc<SyncHandle>>) {
        *self.0.lock().expect("sync slot lock") = handle;
    }

    pub(crate) fn get(&self) -> Option<Arc<SyncHandle>> {
        self.0.lock().expect("sync slot lock").clone()
    }
}

/// Some only when the saved backend is Anytype. `running` is false when the engine
/// could not start; `reason` says why, for Settings and the log.
pub(crate) struct SyncHandle {
    runtime: Option<SyncRuntime>,
    running: bool,
    reason: Option<String>,
}

impl SyncHandle {
    pub(crate) fn running(&self) -> bool {
        self.running
    }

    pub(crate) fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub(crate) fn runtime(&self) -> Option<&SyncRuntime> {
        self.runtime.as_ref()
    }

    /// The shared status answer: the live snapshot when running, the reason otherwise.
    pub(crate) fn status(&self) -> Value {
        match &self.runtime {
            Some(runtime) => runtime.snapshot().to_json(),
            None => json!({
                "available": false,
                "running": false,
                "state": "off",
                "phase": "off",
                "reason": self.reason,
                "error": self.reason,
            }),
        }
    }

    /// The concrete reasons a start did not happen, as one line for the log.
    fn off(reason: String) -> Arc<Self> {
        tracing::warn!("Anytype sync is off: {reason}");
        Arc::new(Self {
            runtime: None,
            running: false,
            reason: Some(reason),
        })
    }
}

/// Whether the saved choice is Anytype. Cheap: reads the saved settings, no network.
/// `main` uses it to build the nudging workspace before the engine starts.
pub(crate) fn chooses_anytype(cfg: &crate::platform::config::Config) -> bool {
    chosen_of(cfg) == "anytype"
}

fn chosen_of(cfg: &crate::platform::config::Config) -> &'static str {
    if let Some(env_choice) = cfg.storage.storage.as_deref() {
        if env_choice.eq_ignore_ascii_case("anytype") {
            return "anytype";
        }
        if env_choice.eq_ignore_ascii_case("filesystem") {
            return "filesystem";
        }
    }
    let stored = crate::storage::settings::read_load(&cfg.data_root).stored;
    if stored.backend.as_str() == "anytype" {
        "anytype"
    } else {
        "filesystem"
    }
}

/// Start the engine when the saved backend is Anytype, else `None`. Never fails the
/// gateway: a bad host, a missing key, or a missing space logs a warning, records the
/// reason on the handle, and leaves the engine off.
pub(crate) async fn start(state: &AppState, nudges: Arc<Nudges>) -> Option<Arc<SyncHandle>> {
    if chosen_of(&state.cfg) != "anytype" {
        return None;
    }
    let cfg = &state.cfg;
    let stored = crate::storage::settings::read_load(&cfg.data_root).stored;
    let host = crate::storage::settings::effective_host(&cfg.storage, &stored);
    let space = crate::storage::settings::effective_space(&cfg.storage, &stored);
    if crate::storage::settings::normalize_host(&host).is_err() {
        let reason = format!(
            "The Anytype host {host} is not a usable URL. hbots is keeping the sync engine off."
        );
        return Some(SyncHandle::off(reason));
    }
    let Some(key) = crate::storage::settings::read_key(&cfg.data_root, &cfg.storage) else {
        let reason =
            "The Anytype key is missing. hbots is keeping the sync engine off.".to_string();
        return Some(SyncHandle::off(reason));
    };
    // The named space must be visible to the key, and its schema must be prepared.
    let client = match crate::storage::adapters::anytype::open_client(&host, &key, &space).await {
        Ok(client) => client,
        Err(error) => {
            let reason = format!(
                "The space check failed: {}. hbots is keeping the sync engine off.",
                error.message
            );
            return Some(SyncHandle::off(reason));
        }
    };
    let ready = crate::storage::adapters::anytype::schema_ready(&client)
        .await
        .unwrap_or(false);
    if !ready {
        let reason =
            "The Anytype space is not prepared for hbots. hbots is keeping the sync engine off."
                .to_string();
        return Some(SyncHandle::off(reason));
    }
    let hostname = hostname::get()
        .ok()
        .and_then(|name| name.to_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    let materials = Materials {
        host: host.clone(),
        key,
        space_id: client.space_id().to_string(),
        bots_root: cfg.bots_root.clone(),
        sync_dir: cfg.data_root.join("sync"),
        hostname,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let runtime = start_runtime(materials, nudges);
    Some(Arc::new(SyncHandle {
        runtime: Some(runtime),
        running: true,
        reason: None,
    }))
}
