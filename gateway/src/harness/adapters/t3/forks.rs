//! Side-chat commands. The JSON stays in this file.
use super::turns::runtime_mode_command;
use super::T3;
use crate::harness::{
    ForkPoint, ForkRequest, Forks, HarnessError, MergeRequest, SideChatRef, Transcript,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;

pub(crate) fn fork_command(request: &ForkRequest) -> Value {
    let mut command = json!({
        "type": "thread.fork",
        "createdBy": "user",
        "creationSource": "web",
        "sourceThreadId": request.source_thread_id,
        "targetThreadId": request.target_thread_id,
        "sourcePoint": source_point(&request.point),
        "title": request.title,
    });
    if request.side_chat {
        command["sideChat"] = json!(true);
    }
    command
}

pub(crate) fn merge_command(request: &MergeRequest) -> Value {
    json!({
        "type": "thread.merge_back",
        "createdBy": "user",
        "creationSource": "web",
        "sourceThreadId": request.source_thread_id,
        "targetThreadId": request.target_thread_id,
        "sourcePoint": { "type": "run", "runId": request.run_id },
    })
}

pub(crate) fn delete_command(thread_id: &str) -> Value {
    json!({
        "type": "thread.delete",
        "threadId": thread_id,
        "onlyIfUnused": true,
    })
}

fn source_point(point: &ForkPoint) -> Value {
    match point {
        ForkPoint::LatestStable => json!({ "type": "latest_stable" }),
        ForkPoint::Run(run_id) => json!({ "type": "run", "runId": run_id }),
    }
}

/// Live side chats of `source`, oldest first. A child projection that is missing was skipped.
pub(crate) fn side_chats_in(
    source: &str,
    parent: &Value,
    children: &HashMap<String, Value>,
) -> Vec<SideChatRef> {
    let mut found = Vec::new();
    for id in fork_target_ids(parent, source) {
        if found.iter().any(|chat: &Listed| chat.id == id) {
            continue;
        }
        let Some(child) = children.get(&id) else {
            continue;
        };
        if let Some(chat) = listed(&id, child) {
            found.push(chat);
        }
    }
    found.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then(left.id.cmp(&right.id))
    });
    found
        .into_iter()
        .map(|chat| SideChatRef {
            id: chat.id,
            title: chat.title,
        })
        .collect()
}

struct Listed {
    id: String,
    title: String,
    created_at: String,
}

fn fork_target_ids(parent: &Value, source: &str) -> Vec<String> {
    parent
        .get("contextTransfers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|transfer| transfer["type"] == "fork" && transfer["sourceThreadId"] == source)
        .filter_map(|transfer| transfer["targetThreadId"].as_str().map(str::to_string))
        .collect()
}

fn listed(id: &str, child: &Value) -> Option<Listed> {
    if !is_side_chat(child) || is_deleted(child) {
        return None;
    }
    Some(Listed {
        id: id.to_string(),
        title: title_of(child),
        created_at: created_of(child),
    })
}

fn is_side_chat(child: &Value) -> bool {
    child["lineage"]["sideChat"] == true || child["thread"]["lineage"]["sideChat"] == true
}

fn is_deleted(child: &Value) -> bool {
    stamp(&child["deletedAt"]) || stamp(&child["thread"]["deletedAt"])
}

fn stamp(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.is_empty(),
        Value::Number(_) => true,
        Value::Bool(set) => *set,
        Value::Array(_) | Value::Object(_) => false,
    }
}

fn title_of(child: &Value) -> String {
    text(&child["title"])
        .or_else(|| text(&child["thread"]["title"]))
        .unwrap_or("Side chat")
        .to_string()
}

fn created_of(child: &Value) -> String {
    text(&child["createdAt"])
        .or_else(|| text(&child["thread"]["createdAt"]))
        .unwrap_or("")
        .to_string()
}

fn text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

#[async_trait]
impl Forks for T3 {
    async fn fork(&self, request: &ForkRequest) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        rpc.dispatch(fork_command(request))
            .await
            .map_err(HarnessError::failed)?;
        if let Some(mode) = &request.runtime_mode {
            if let Err(err) = rpc
                .dispatch(runtime_mode_command(&request.target_thread_id, mode))
                .await
            {
                let _ = rpc
                    .dispatch(delete_command(&request.target_thread_id))
                    .await;
                return Err(HarnessError::failed(err));
            }
        }
        Ok(())
    }

    async fn merge_back(&self, request: &MergeRequest) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        rpc.dispatch(merge_command(request))
            .await
            .map_err(HarnessError::failed)?;
        Ok(())
    }

    async fn list_side_chats(
        &self,
        source_thread_id: &str,
    ) -> Result<Vec<SideChatRef>, HarnessError> {
        let parent = Transcript::read(self, source_thread_id).await?;
        let mut children = HashMap::new();
        for id in fork_target_ids(parent.body(), source_thread_id) {
            if let Ok(snapshot) = Transcript::read(self, &id).await {
                children.insert(id, snapshot.body().clone());
            }
        }
        Ok(side_chats_in(source_thread_id, parent.body(), &children))
    }

    async fn delete_if_unused(&self, thread_id: &str) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        rpc.dispatch(delete_command(thread_id))
            .await
            .map_err(HarnessError::failed)?;
        Ok(())
    }
}
