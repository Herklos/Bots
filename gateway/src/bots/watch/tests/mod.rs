//! Phase 4 tests: evaluators, the SSRF guard, the poller's backoff and cooldown, the
//! store, the self-delivery to this machine's own hook, and the tools' public-host rule.

mod checks;
mod delivery;
#[path = "guard_addr_tests.rs"]
mod guard_addr;
#[path = "scheduling_tests.rs"]
mod scheduling;
#[path = "store_tests.rs"]
mod store;
#[path = "watch_tools_tests.rs"]
mod watch_tools;

use super::*;
pub(crate) use crate::harness::adapters::mock::MockHarness;
use crate::AppState;
use serde_json::{json, Value};
use std::sync::Arc;

pub(crate) fn mk_check(kind: &str, extra: Value) -> Value {
    let mut out = json!({ "type": kind });
    if let Value::Object(fields) = extra {
        for (key, value) in fields {
            out[key] = value;
        }
    }
    out
}

pub(crate) fn fire(mode: &str, cooldown: u64) -> Value {
    json!({ "mode": mode, "cooldownSec": cooldown })
}

/// A state that has already seen one poll: comparisons are against a baseline.
pub(crate) fn fresh_state() -> Value {
    json!({ "last": "", "lastTrue": Value::Null, "failures": 0, "failingSince": 0, "lastAt": "1970-01-01T00:02:00Z" })
}

/// A state that has never completed a poll: the first observation is only a baseline.
pub(crate) fn blank_state() -> Value {
    json!({ "last": "", "lastTrue": Value::Null, "failures": 0, "failingSince": 0 })
}

pub(crate) fn watch_with(check: &Value, fire: Value, state: Value) -> Value {
    json!({
        "id": "w_test01", "name": "Test", "enabled": true,
        "target": { "type": "http", "url": "https://example.com/status", "method": "GET" },
        "check": check, "fire": fire, "everyMs": 60_000,
        "allowPrivate": false, "state": state,
    })
}

pub(crate) fn evaluate_ok(check: &Value, body: &[u8], status: u16) -> evaluate::Eval {
    evaluate::evaluate(check, status, body).unwrap()
}

pub(crate) fn assert_none(value: Option<Value>) {
    assert!(value.is_none(), "expected no fire, got {value:?}");
}

pub(crate) async fn watch_state(
    dir: &std::path::Path,
    mock: Arc<crate::harness::adapters::mock::MockHarness>,
) -> Arc<AppState> {
    let state = crate::api::settings_state_with(
        dir,
        Some(Arc::clone(&mock) as Arc<dyn crate::harness::Harness>),
    );
    state.bots.create(&json!({ "name": "Ada" })).await.unwrap();
    state
        .bots
        .save(
            "ada",
            &json!({ "mainThreadId": "t-main", "projectId": "p1" }),
            false,
        )
        .await
        .unwrap();
    state
}
