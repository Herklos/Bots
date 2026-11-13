//! Typed-object cache groundwork for P4. This module is not connected to the
//! running sync engine or to writers yet; see README.md before using it.
#![allow(dead_code)]

use super::client::{AnytypeClient, AnytypeError};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(crate) enum CacheError {
    Remote(AnytypeError),
    Incomplete(&'static str),
    Superseded,
}

impl From<AnytypeError> for CacheError {
    fn from(error: AnytypeError) -> Self {
        Self::Remote(error)
    }
}

#[derive(Debug, Clone)]
struct Entry {
    object: Value,
    fetched_at: Instant,
    uncertain: bool,
}

#[derive(Debug, Default)]
struct State {
    epoch: u64,
    entries: BTreeMap<String, Entry>,
    snapshot_at: Option<Instant>,
    pending: BTreeSet<String>,
}

/// One cache per type. Stream rows are hints, never authoritative objects.
#[derive(Debug)]
pub(crate) struct TypedCache {
    type_key: String,
    ttl: Duration,
    state: Mutex<State>,
}

#[derive(Debug, Clone)]
pub(crate) struct CachedObject {
    pub object: Value,
    /// False after an invalidation, an incomplete snapshot, or TTL expiry.
    pub fresh: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct View {
    pub objects: BTreeMap<String, CachedObject>,
    /// A recent poll with no unresolved missing or invalidated rows.
    pub complete: bool,
    pub snapshot_age: Option<Duration>,
}

impl TypedCache {
    pub(crate) fn new(type_key: impl Into<String>, ttl: Duration) -> Self {
        Self {
            type_key: type_key.into(),
            ttl,
            state: Mutex::new(State::default()),
        }
    }

    pub(crate) fn token(&self) -> u64 {
        self.state.lock().expect("typed cache").epoch
    }

    /// Poll without projections: search results can omit `properties`. Fetch
    /// every returned id. Failed/partial searches never replace cached data.
    pub(crate) async fn refresh(&self, client: &AnytypeClient) -> Result<(), CacheError> {
        if self.type_key.is_empty() {
            return Err(CacheError::Incomplete("type key is empty"));
        }
        let token = self.token();
        let started_at = Instant::now();
        let rows = client.search_strict(&json!({"type":self.type_key})).await?;
        let mut ids = BTreeSet::new();
        for row in rows {
            let id = object_id(&row).ok_or(CacheError::Incomplete("search row has no id"))?;
            if !ids.insert(id.to_owned()) {
                return Err(CacheError::Incomplete("duplicate search id"));
            }
        }
        let mut objects = BTreeMap::new();
        for id in ids {
            let object = client.get_object(&id).await?;
            self.validate(&id, &object)?;
            objects.insert(id, object);
        }
        self.install_snapshot(token, objects, started_at)
    }

    fn validate(&self, id: &str, object: &Value) -> Result<(), CacheError> {
        if object_id(object) != Some(id)
            || object["type"].as_str() != Some(self.type_key.as_str())
            || !object["properties"].is_object()
        {
            return Err(CacheError::Incomplete(
                "object identity, type, or properties mismatch",
            ));
        }
        Ok(())
    }

    fn install_snapshot(
        &self,
        token: u64,
        objects: BTreeMap<String, Value>,
        started_at: Instant,
    ) -> Result<(), CacheError> {
        let mut state = self.state.lock().expect("typed cache");
        if state.epoch != token {
            return Err(CacheError::Superseded);
        }
        // A paginated search is not an atomic snapshot. Never infer deletion
        // from absence: retain the last value and force callers to revalidate.
        for (id, entry) in &mut state.entries {
            if !objects.contains_key(id) {
                entry.uncertain = true;
            }
        }
        for (id, object) in objects {
            state.pending.remove(&id);
            state.entries.insert(
                id,
                Entry {
                    object,
                    fetched_at: started_at,
                    uncertain: false,
                },
            );
        }
        state.epoch = state.epoch.wrapping_add(1);
        state.snapshot_at = Some(started_at);
        Ok(())
    }

    /// An add, update, or remove means fetch again. Even an unknown id advances
    /// the epoch so an in-flight poll cannot claim completeness.
    pub(crate) fn invalidate(&self, id: &str) {
        let mut state = self.state.lock().expect("typed cache");
        state.epoch = state.epoch.wrapping_add(1);
        state.pending.insert(id.to_string());
        if let Some(entry) = state.entries.get_mut(id) {
            entry.uncertain = true;
        }
        state.snapshot_at = None;
    }

    /// Cache a complete object after a confirmed remote write and subsequent
    /// GET. Patch/create responses alone are not full documents. Caller must
    /// pass the token and time captured before that GET to avoid overstating
    /// freshness or overwriting a newer
    /// invalidation or write-through.
    pub(crate) fn install_confirmed(
        &self,
        token: u64,
        started_at: Instant,
        object: Value,
    ) -> Result<(), CacheError> {
        let id = object_id(&object)
            .ok_or(CacheError::Incomplete("object has no id"))?
            .to_owned();
        self.validate(&id, &object)?;
        let mut state = self.state.lock().expect("typed cache");
        if state.epoch != token {
            return Err(CacheError::Superseded);
        }
        state.epoch = state.epoch.wrapping_add(1);
        state.pending.remove(&id);
        state.snapshot_at = None;
        state.entries.insert(
            id,
            Entry {
                object,
                fetched_at: started_at,
                uncertain: false,
            },
        );
        Ok(())
    }

    pub(crate) fn view(&self) -> View {
        let state = self.state.lock().expect("typed cache");
        let now = Instant::now();
        let snapshot_age = state
            .snapshot_at
            .map(|time| now.saturating_duration_since(time));
        let objects: BTreeMap<_, _> = state
            .entries
            .iter()
            .map(|(id, entry)| {
                (
                    id.clone(),
                    CachedObject {
                        object: entry.object.clone(),
                        fresh: !entry.uncertain
                            && now.saturating_duration_since(entry.fetched_at) <= self.ttl,
                    },
                )
            })
            .collect();
        let complete = state.pending.is_empty()
            && snapshot_age.is_some_and(|age| age <= self.ttl)
            && objects.values().all(|entry| entry.fresh);
        View {
            objects,
            complete,
            snapshot_age,
        }
    }
}

fn object_id(value: &Value) -> Option<&str> {
    value["id"].as_str().filter(|id| !id.is_empty())
}
