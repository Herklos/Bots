//! Routines are scheduled tasks on the harness. T3 Code is the one wired today
//! (`scheduledTasks.*`); other backends may be added.
//!
//! Tags, from `packages/contracts/src/rpc.ts` (`WS_METHODS`, the `scheduledTasks*` entries):
//! `scheduledTasks.subscribe`, `scheduledTasks.upsert`, `scheduledTasks.setEnabled`,
//! `scheduledTasks.delete`, `scheduledTasks.runNow`. `subscribe` is a stream; its first
//! item is `{ tasks }` (`ScheduledTaskListResult` in `packages/contracts/src/scheduledTask.ts`).
//! The gateway keeps a short local history of the changes it made.
use crate::error::ApiResult;
use crate::t3::T3;
use serde_json::{json, Value};
use std::path::PathBuf;

pub const LIST: &str = "scheduledTasks.subscribe";
pub const UPSERT: &str = "scheduledTasks.upsert";
pub const SET_ENABLED: &str = "scheduledTasks.setEnabled";
pub const DELETE: &str = "scheduledTasks.delete";
pub const RUN_NOW: &str = "scheduledTasks.runNow";

const HISTORY_LIMIT: usize = 50;

pub struct RoutineHistory {
    file: PathBuf,
}

impl RoutineHistory {
    pub fn new(file: PathBuf) -> Self {
        Self { file }
    }

    pub async fn recent(&self) -> Value {
        match tokio::fs::read_to_string(&self.file).await {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| json!([])),
            Err(_) => json!([]),
        }
    }

    pub async fn record(&self, action: &str, task: &Value) {
        let mut items = self.recent().await.as_array().cloned().unwrap_or_default();
        items.insert(
            0,
            json!({
                "action": action,
                "id": task.get("id").cloned().unwrap_or(Value::Null),
                "title": task.get("title").cloned().unwrap_or(Value::Null),
                "at": now(),
            }),
        );
        items.truncate(HISTORY_LIMIT);
        let _ = crate::store::write_json(&self.file, &Value::Array(items)).await;
    }
}

pub async fn list(t3: &T3) -> ApiResult<Value> {
    let value = t3.rpc().await?.call(LIST, json!({})).await?;
    Ok(value)
}

pub async fn change(
    t3: &T3,
    history: &RoutineHistory,
    action: &str,
    tag: &str,
    payload: Value,
) -> ApiResult<Value> {
    let value = t3.rpc().await?.call(tag, payload).await?;
    let task = value.get("task").cloned().unwrap_or_else(|| value.clone());
    history.record(action, &task).await;
    Ok(value)
}

pub(crate) fn stamped() -> String {
    now()
}

fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix(secs)
}

fn format_unix(secs: u64) -> String {
    // Civil date from Unix days (Howard Hinnant), UTC.
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn history_keeps_the_newest_fifty() {
        let dir = tempfile::tempdir().unwrap();
        let history = RoutineHistory::new(dir.path().join("routine-history.json"));
        for n in 0..60 {
            history
                .record(
                    "upsert",
                    &json!({ "id": format!("t{n}"), "title": "Morning" }),
                )
                .await;
        }
        let recent = history.recent().await;
        let list = recent.as_array().unwrap();
        assert_eq!(list.len(), 50);
        assert_eq!(list[0]["id"], "t59");
        assert_eq!(list[0]["action"], "upsert");
        assert!(list[0]["at"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn a_known_unix_second_formats() {
        assert_eq!(format_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix(1_700_000_000), "2023-11-14T22:13:20Z");
    }
}
