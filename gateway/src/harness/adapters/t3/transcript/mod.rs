mod preview;
mod project;
mod renders;
mod usage;

#[cfg(test)]
#[path = "renders_tests.rs"]
mod renders_tests;

#[cfg(test)]
pub(crate) use usage::turn_token_line;
pub(crate) use usage::{tokens_from_projection, turns_from_projection};

pub(crate) use preview::latest_preview;
#[cfg(test)]
pub(crate) use preview::preview_text;
#[cfg(test)]
pub(crate) use project::message_id;
#[cfg(test)]
pub(crate) use project::{child_send, provider_failure};
pub(crate) use project::{
    command_with, group_transcript, latest_run_id, runtime_mode, send_command, transcript,
    transcript_for, turn_items, with_tokens,
};

pub(crate) const SHELL: &str = "orchestration.subscribeShell";

use super::T3;
use crate::harness::{HarnessError, ShellSnapshot, ShellThread, ThreadSnapshot, Transcript};
use async_trait::async_trait;
use serde_json::{json, Value};

#[async_trait]
impl Transcript for T3 {
    async fn read(&self, thread_id: &str) -> Result<ThreadSnapshot, HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        let body = rpc
            .call(
                "orchestration.getThreadProjection",
                json!({ "threadId": thread_id }),
            )
            .await
            .map_err(HarnessError::failed)?;
        Ok(ThreadSnapshot::new(body))
    }

    async fn activity(&self) -> Result<ShellSnapshot, HarnessError> {
        read_shell(self, false).await
    }

    async fn busy(&self) -> Result<ShellSnapshot, HarnessError> {
        let client = self
            .rpc()
            .await
            .map_err(|err| HarnessError::failed(format!("couldn't reach T3: {err}")))?;
        read_shell_with(client.as_ref(), true).await.map_err(|err| {
            HarnessError::failed(format!("couldn't read the busy Bot threads: {err}"))
        })
    }
}

async fn read_shell(t3: &T3, busy: bool) -> Result<ShellSnapshot, HarnessError> {
    let client = t3.rpc().await.map_err(HarnessError::unavailable)?;
    read_shell_with(client.as_ref(), busy)
        .await
        .map_err(HarnessError::failed)
}

async fn read_shell_with(
    client: &crate::harness::t3::T3Client,
    busy: bool,
) -> anyhow::Result<ShellSnapshot> {
    let item = client
        .read_until(SHELL, json!({}), |item| {
            let kind = item.get("kind").and_then(Value::as_str);
            kind == Some("snapshot") || (busy && kind == Some("snapshot-required"))
        })
        .await?;
    if item.get("kind").and_then(Value::as_str) == Some("snapshot-required") {
        return Ok(ShellSnapshot {
            threads: Vec::new(),
            http_required: true,
            busy_count: 0,
        });
    }
    let snapshot = item.get("snapshot").cloned().unwrap_or(Value::Null);
    Ok(ShellSnapshot {
        threads: shell_threads(&snapshot),
        http_required: false,
        busy_count: crate::platform::release::busy_in_snapshot(&snapshot),
    })
}

pub(crate) fn facts_in(
    snapshot: &Value,
) -> std::collections::HashMap<String, crate::bots::roster::Fact> {
    snapshot
        .get("threads")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|thread| {
            let id = thread["id"].as_str()?.to_string();
            let working = crate::platform::release::thread_is_busy(thread)
                || thread["activeRunId"].is_string();
            Some((
                id,
                crate::bots::roster::Fact {
                    stamp: thread["updatedAt"].as_str().unwrap_or("").to_string(),
                    working,
                },
            ))
        })
        .collect()
}

fn shell_threads(snapshot: &Value) -> Vec<ShellThread> {
    let facts = facts_in(snapshot);
    ["threads", "archivedThreads"]
        .into_iter()
        .flat_map(|key| {
            let inbox = key == "threads";
            snapshot
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|thread| {
                    let id = thread["id"].as_str()?.to_string();
                    let fact = facts.get(&id);
                    let working = if inbox {
                        fact.is_some_and(|fact| fact.working)
                    } else {
                        crate::platform::release::thread_is_busy(thread)
                            || thread["activeRunId"].is_string()
                    };
                    let updated_at = if inbox {
                        fact.map(|fact| fact.stamp.clone()).unwrap_or_default()
                    } else {
                        thread["updatedAt"].as_str().unwrap_or("").to_string()
                    };
                    Some(ShellThread {
                        working,
                        updated_at,
                        inbox,
                        id,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
