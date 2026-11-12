use super::*;
use crate::bots::runtime_mode::RuntimeMode;
use serde_json::{json, Value};

pub(super) const NOW: &str = "2026-11-12T10:00:00Z";
pub(super) const LATER: &str = "2026-11-12T10:05:00Z";

pub(super) fn agent(id: &str, every_ms: u64) -> Value {
    json!({
        "id": id,
        "title": "Morning digest",
        "prompt": "secret prompt text",
        "enabled": true,
        "createdBy": "agent",
        "creationSource": "mcp",
        "schedule": { "type": "interval", "everyMs": every_ms },
        "runtimeMode": "approval-required",
        "lastRunStatus": "never",
    })
}

pub(super) fn hold() -> Action {
    Action::Hold {
        reason: Reason::AgentCreated,
        every_ms: None,
        runtime_mode: None,
    }
}

pub(super) fn sealed(ids: &[&str]) -> ReviewStore {
    let mut store = ReviewStore::empty();
    store.seal_baseline(ids.iter().copied(), NOW);
    store
}

pub(super) fn ask() -> ReviewContext {
    ReviewContext::ask(RuntimeMode::FullAccess)
}

mod persistence;
mod rules;
