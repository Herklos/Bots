//! A per-Bot repository fact, kept for [`SIGNAL_TTL`].
//!
//! Only `auto` asks the harness. The master switch, `on`, and `off` do not.
//! A failed ask is not cached, so a later try can succeed.

use super::signal::{signal, CodeMode, SignalInput, VcsStatus};
use crate::harness::Harness;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(crate) const SIGNAL_TTL: Duration = Duration::from_secs(60);

#[derive(Debug)]
struct Slot {
    at: Instant,
    is_repo: bool,
}

#[derive(Debug)]
pub(crate) struct SignalCache {
    ttl: Duration,
    slots: Mutex<HashMap<String, Slot>>,
}

impl SignalCache {
    pub(crate) fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            slots: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Slot>> {
        self.slots.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub(crate) fn get(&self, slug: &str, now: Instant) -> Option<bool> {
        let slots = self.lock();
        let slot = slots.get(slug)?;
        fresh(slot, now, self.ttl).then_some(slot.is_repo)
    }

    pub(crate) fn put(&self, slug: &str, now: Instant, is_repo: bool) {
        self.lock()
            .insert(slug.to_string(), Slot { at: now, is_repo });
    }
}

impl Default for SignalCache {
    fn default() -> Self {
        Self::new(SIGNAL_TTL)
    }
}

fn fresh(slot: &Slot, now: Instant, ttl: Duration) -> bool {
    match now.checked_duration_since(slot.at) {
        Some(age) => age < ttl,
        None => true,
    }
}

/// What `GET /api/bots/{slug}/code` is allowed to say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gate {
    pub enabled: bool,
    pub show: bool,
}

pub(crate) async fn evaluate(
    harness: &dyn Harness,
    cache: &SignalCache,
    now: Instant,
    slug: &str,
    cwd: &str,
    master_on: bool,
    mode: CodeMode,
) -> Gate {
    if !master_on {
        return Gate {
            enabled: false,
            show: false,
        };
    }
    let status = if matches!(mode, CodeMode::Auto) {
        load_status(harness, cache, now, slug, cwd).await
    } else {
        None
    };
    let show = signal(&SignalInput {
        mode,
        vcs: status.as_ref(),
        activity: None,
    })
    .show;
    Gate {
        enabled: true,
        show,
    }
}

async fn load_status(
    harness: &dyn Harness,
    cache: &SignalCache,
    now: Instant,
    slug: &str,
    cwd: &str,
) -> Option<VcsStatus> {
    if let Some(is_repo) = cache.get(slug, now) {
        return Some(VcsStatus { is_repo });
    }
    let Ok(probe) = harness.repos() else {
        return None;
    };
    let Ok(is_repo) = probe.is_repo(cwd).await else {
        return None;
    };
    cache.put(slug, now, is_repo);
    Some(VcsStatus { is_repo })
}
