//! Fork book for the in-memory harness. Kept beside the mock so the adapter file stays short.
use super::{missing, MockHarness};
use crate::harness::{ForkRequest, Forks, HarnessError, MergeRequest, SideChatRef};
use async_trait::async_trait;
use serde_json::Value;

impl MockHarness {
    pub(crate) fn arm_forks(&self) {
        self.book().armed = true;
    }

    pub(crate) fn set_projection(&self, thread: &str, body: Value) {
        self.book().projections.insert(thread.to_string(), body);
    }

    pub(crate) fn set_side_chats(&self, source: &str, chats: Vec<SideChatRef>) {
        self.book().chats.insert(source.to_string(), chats);
    }

    pub(crate) fn fork_requests(&self) -> Vec<ForkRequest> {
        self.book().forks.clone()
    }

    pub(crate) fn merge_requests(&self) -> Vec<MergeRequest> {
        self.book().merges.clone()
    }

    pub(crate) fn deleted_threads(&self) -> Vec<String> {
        self.book().deletes.clone()
    }

    pub(crate) fn sent_threads(&self) -> Vec<(String, String)> {
        self.book().sends.clone()
    }

    pub(crate) fn launches(&self) -> usize {
        self.book().launches
    }

    pub(crate) fn fail_next_send(&self) {
        self.book().fail_send = true;
    }
}

#[async_trait]
impl Forks for MockHarness {
    async fn fork(&self, request: &ForkRequest) -> Result<(), HarnessError> {
        let mut book = self.book();
        if !book.armed {
            return Err(missing("side chats"));
        }
        book.chats
            .entry(request.source_thread_id.clone())
            .or_default()
            .push(SideChatRef {
                id: request.target_thread_id.clone(),
                title: request.title.clone(),
            });
        book.forks.push(request.clone());
        Ok(())
    }

    async fn merge_back(&self, request: &MergeRequest) -> Result<(), HarnessError> {
        let mut book = self.book();
        if !book.armed {
            return Err(missing("side chats"));
        }
        book.merges.push(request.clone());
        Ok(())
    }

    async fn list_side_chats(
        &self,
        source_thread_id: &str,
    ) -> Result<Vec<SideChatRef>, HarnessError> {
        let book = self.book();
        if !book.armed {
            return Err(missing("side chats"));
        }
        Ok(book
            .chats
            .get(source_thread_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn delete_if_unused(&self, thread_id: &str) -> Result<(), HarnessError> {
        let mut book = self.book();
        if !book.armed {
            return Err(missing("side chats"));
        }
        book.deletes.push(thread_id.to_string());
        Ok(())
    }
}
