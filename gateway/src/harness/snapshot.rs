//! A thread read, already turned into the shapes the rest of the gateway uses.
//! The runtime's own projection never leaves the adapter.

use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserTab {
    pub id: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub bytes: Vec<u8>,
    pub clipboard: String,
}

#[derive(Debug)]
pub struct ThreadSnapshot {
    body: Value,
}

impl ThreadSnapshot {
    pub(crate) fn new(body: Value) -> Self {
        Self { body }
    }

    pub(crate) fn transcript(&self) -> Value {
        crate::harness::t3::transcript::transcript(&self.body)
    }

    pub(crate) fn transcript_for(&self, peers: &crate::chat::Peers) -> Value {
        crate::harness::t3::transcript::with_tokens(
            crate::harness::t3::transcript::transcript_for(&self.body, peers),
            &self.body,
        )
    }

    pub(crate) fn lineage(&self) -> Vec<Value> {
        crate::bots::lineage::rows(&self.body)
    }

    pub(crate) fn token_totals(&self) -> (crate::bots::usage::TokenTotals, bool) {
        crate::harness::t3::transcript::tokens_from_projection(&self.body)
    }

    pub(crate) fn turns(&self) -> Vec<crate::bots::usage::Turn> {
        crate::harness::t3::transcript::turns_from_projection(&self.body)
    }

    pub(crate) fn active_run_id(&self) -> Option<String> {
        self.transcript()["activeRunId"]
            .as_str()
            .map(str::to_string)
    }

    pub(crate) fn preview(&self, names: &HashMap<String, String>) -> Option<Value> {
        crate::harness::t3::transcript::latest_preview(&self.body, names)
    }

    /// Group-thread working flag and the messages the room shows.
    pub(crate) fn group(&self) -> (bool, Vec<Value>) {
        let transcript = crate::harness::t3::transcript::group_transcript(&self.body);
        let working = transcript["working"] == true;
        let messages = transcript["messages"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        (working, messages)
    }
}

/// One thread in a shell snapshot, reduced to what the inbox and the updater use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellThread {
    pub id: String,
    pub updated_at: String,
    pub working: bool,
    /// Present on the live thread list the inbox reads. Archived threads are only for the busy count.
    pub inbox: bool,
}

#[derive(Clone, Debug)]
pub struct ShellSnapshot {
    pub threads: Vec<ShellThread>,
    /// The runtime asked for an HTTP snapshot before the stream. Auto-update waits.
    pub http_required: bool,
    pub(crate) busy_count: usize,
}

impl ShellSnapshot {
    pub(crate) fn facts(&self) -> HashMap<String, (String, bool)> {
        self.threads
            .iter()
            .filter(|thread| thread.inbox)
            .map(|thread| {
                (
                    thread.id.clone(),
                    (thread.updated_at.clone(), thread.working),
                )
            })
            .collect()
    }

    pub(crate) fn busy_count(&self) -> usize {
        self.busy_count
    }
}

/// A user message the gateway already decided to send. The adapter builds the runtime command.
#[derive(Clone, Debug)]
pub struct UserMessage {
    pub thread_id: String,
    pub message_id: String,
    pub text: String,
    pub attachments: Vec<Value>,
}

impl UserMessage {
    pub(crate) fn new(
        thread_id: &str,
        message_id: &str,
        text: &str,
        attachments: &[Value],
    ) -> Self {
        Self {
            thread_id: thread_id.to_string(),
            message_id: message_id.to_string(),
            text: text.to_string(),
            attachments: attachments.to_vec(),
        }
    }
}

/// Who a provisioned message is from, and how it waits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProvisionedSend {
    /// The brief, queued behind an active run.
    Brief,
    /// The person's first message, started immediately, with no steer intent.
    First,
}
