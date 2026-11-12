//! Projections kept while their `updatedAt` stamp holds.
//!
//! A search reads a thread again only when that stamp moves. An empty stamp
//! is not a cache key: the caller does not know whether the thread changed.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub(crate) struct ProjectionCache {
    inner: Mutex<HashMap<String, (String, Value)>>,
}

impl ProjectionCache {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn get(&self, thread: &str, stamp: &str) -> Option<Value> {
        if stamp.is_empty() {
            return None;
        }
        let guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard
            .get(thread)
            .filter(|(seen, _)| seen == stamp)
            .map(|(_, body)| body.clone())
    }

    pub(crate) fn store(&self, thread: &str, stamp: &str, body: &Value) {
        if stamp.is_empty() {
            return;
        }
        let mut guard = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        guard.insert(thread.to_string(), (stamp.to_string(), body.clone()));
    }
}
