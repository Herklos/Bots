//! Canned conversation search for domain tests.
//!
//! [`SearchScript::Absent`] is a runtime that has no search command.

use super::{missing, MockHarness};
use crate::harness::{HarnessError, ThreadHit, ThreadSearch};
use async_trait::async_trait;

#[derive(Clone, Debug)]
pub(crate) enum SearchScript {
    Absent,
    Failed,
    /// The socket is down. The search falls through to the local scan.
    Down,
    Hits(Vec<ThreadHit>),
}

impl MockHarness {
    pub(crate) fn set_search(&self, script: SearchScript) {
        *self
            .search_script
            .lock()
            .unwrap_or_else(|err| err.into_inner()) = script;
    }

    pub(crate) fn search_calls(&self) -> Vec<(String, u32)> {
        self.search_calls
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    pub(crate) fn read_count(&self, thread: &str) -> usize {
        self.thread_reads
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .get(thread)
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn note_read(&self, thread: &str) {
        let mut reads = self
            .thread_reads
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        *reads.entry(thread.to_string()).or_insert(0) += 1;
    }
}

#[async_trait]
impl ThreadSearch for MockHarness {
    async fn search_threads(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<ThreadHit>, HarnessError> {
        self.search_calls
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .push((query.to_string(), limit));
        let script = self
            .search_script
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone();
        match script {
            SearchScript::Absent => Err(missing("thread search")),
            SearchScript::Failed => Err(HarnessError::failed("search failed")),
            SearchScript::Down => Err(HarnessError::unavailable("closed")),
            SearchScript::Hits(hits) => Ok(hits),
        }
    }
}
