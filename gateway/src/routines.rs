//! Routine history (port of the store in `server/routine-history.mts`): the dispatches and
//! executions of each routine that Threebot has observed, as references only. T3 remains the
//! scheduler. Prompts, webhook tokens, message bodies and errors are never stored.
//!
//! Not wired yet. Feeding it needs T3's `scheduledTasks.subscribe` and `orchestration.subscribeShell`
//! streams, and `T3Client` (in `t3.rs`, outside this task's files) drops stream chunks. Until that
//! lands, nothing calls this module, so the lint is allowed here rather than per item.
#![allow(dead_code)]

use crate::{
    error::{ApiError, ApiResult},
    store,
    vault::now_iso,
};
use regex::Regex;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::LazyLock,
};
use tokio::sync::Mutex;

static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_:%.\-]{1,600}$").unwrap());
static STAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})$")
        .unwrap()
});

const TERMINAL: [&str; 5] = [
    "completed",
    "failed",
    "cancelled",
    "interrupted",
    "rolled_back",
];
const ACTIVE: [&str; 5] = ["queued", "preparing", "starting", "running", "waiting"];

fn id(value: Option<&str>) -> Option<&str> {
    value.filter(|v| ID.is_match(v))
}

fn stamp(value: Option<&str>) -> Option<&str> {
    value.filter(|v| STAMP.is_match(v))
}

fn dispatch_status(value: Option<&str>) -> &'static str {
    match value {
        Some("succeeded") => "dispatched",
        Some("failed") => "dispatch_failed",
        Some("running") => "dispatching",
        _ => "unknown",
    }
}

/// Reads the trigger from a scheduled task's message ID, `scheduled-task-message:<task>:<...>`.
fn trigger_from_message(message_id: Option<&str>, task_id: &str) -> &'static str {
    let Some(tail) =
        message_id.and_then(|m| m.strip_prefix(&format!("scheduled-task-message:{task_id}:")))
    else {
        return "unknown";
    };
    if tail.contains(":webhook:") || tail.starts_with("webhook:") {
        "webhook"
    } else if tail.ends_with(":manual") {
        "manual"
    } else if tail.ends_with(":scheduled") {
        "scheduled"
    } else {
        "unknown"
    }
}

/// Keeps the newest `limit` rows of each routine. `rows` is ordered newest first, and a row
/// replaces an earlier one with the same ID in place, as a JavaScript `Map` does.
fn merge(existing: &mut Vec<Value>, incoming: Vec<Value>, limit: usize) {
    for row in incoming {
        match existing.iter().position(|r| r["id"] == row["id"]) {
            Some(index) => existing[index] = row,
            None => existing.push(row),
        }
    }
    existing.sort_by(|a, b| b["at"].as_str().cmp(&a["at"].as_str()));
    let mut counts: HashMap<String, usize> = HashMap::new();
    existing.retain(|row| {
        let count = counts
            .entry(row["taskId"].as_str().unwrap_or("").to_string())
            .or_insert(0);
        *count += 1;
        *count <= limit
    });
}

#[derive(Clone)]
struct Previous {
    last_run_at: Option<String>,
    status: Option<String>,
}

struct State {
    observing_since: String,
    rows: Vec<Value>,
}

pub struct RoutineHistory {
    file: PathBuf,
    limit: usize,
    state: Mutex<Option<State>>,
    previous: Mutex<HashMap<String, Previous>>,
}

/// One observed dispatch or execution, references only.
struct Row<'a> {
    id: String,
    task_id: &'a str,
    at: &'a str,
    status: &'a str,
    trigger: &'a str,
    thread_id: Option<&'a str>,
    run_id: Option<&'a str>,
    source: &'static str,
}

impl Row<'_> {
    fn json(self) -> Value {
        json!({
            "id": self.id, "taskId": self.task_id, "at": self.at, "status": self.status,
            "trigger": self.trigger, "threadId": self.thread_id, "runId": self.run_id,
            "source": self.source,
        })
    }
}

async fn persist(file: &Path, state: &State) -> ApiResult<()> {
    let text = json!({ "version": 1, "observingSince": state.observing_since, "rows": state.rows });
    let bytes = serde_json::to_string(&text).map_err(|e| ApiError::new(500, e.to_string()))? + "\n";
    store::write_atomic(file, bytes.as_bytes(), Some(0o600)).await?;
    Ok(())
}

impl RoutineHistory {
    pub fn new(file: PathBuf, limit: usize) -> Self {
        Self {
            file,
            limit,
            state: Mutex::new(None),
            previous: Mutex::new(HashMap::new()),
        }
    }

    /// Loads the file once. A missing file starts an empty history observed from now.
    async fn loaded<'a>(&self, slot: &'a mut Option<State>) -> ApiResult<&'a mut State> {
        if slot.is_none() {
            let loaded = match tokio::fs::read_to_string(&self.file).await {
                Ok(text) => {
                    let value: Value = serde_json::from_str(&text)
                        .map_err(|_| ApiError::new(500, "Invalid routine history file"))?;
                    let observing = stamp(value["observingSince"].as_str());
                    match (
                        value["version"].as_i64(),
                        value["rows"].as_array(),
                        observing,
                    ) {
                        (Some(1), Some(rows), Some(since)) => State {
                            observing_since: since.to_string(),
                            rows: rows.clone(),
                        },
                        _ => return Err(ApiError::new(500, "Invalid routine history file")),
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => State {
                    observing_since: now_iso(),
                    rows: Vec::new(),
                },
                Err(e) => return Err(e.into()),
            };
            *slot = Some(loaded);
        }
        slot.as_mut()
            .ok_or_else(|| ApiError::new(500, "Routine history is not loaded"))
    }

    /// Observes the tasks' last-run stamps. A task's first summary is not a run: it is only recorded
    /// as a dispatch when a later summary moves its stamp or its status.
    pub async fn consume_tasks(&self, tasks: &[Value]) -> ApiResult<()> {
        let mut slot = self.state.lock().await;
        let state = self.loaded(&mut slot).await?;
        let mut previous = self.previous.lock().await;
        let mut rows = Vec::new();
        for task in tasks {
            let Some(task_id) = id(task["id"].as_str()) else {
                continue;
            };
            let last_run_at = stamp(task["lastRunAt"].as_str()).map(String::from);
            let status = task["lastRunStatus"].as_str().map(String::from);
            if let (Some(old), Some(at)) = (previous.get(task_id), last_run_at.as_deref()) {
                if old.last_run_at.as_deref() != Some(at) || old.status != status {
                    rows.push(
                        Row {
                            id: format!("dispatch:{task_id}:{at}"),
                            task_id,
                            at,
                            status: dispatch_status(status.as_deref()),
                            trigger: "unknown",
                            thread_id: id(task["threadId"].as_str()),
                            run_id: None,
                            source: "dispatch",
                        }
                        .json(),
                    );
                }
            }
            previous.insert(
                task_id.to_string(),
                Previous {
                    last_run_at,
                    status,
                },
            );
        }
        merge(&mut state.rows, rows, self.limit);
        persist(&self.file, state).await
    }

    /// Records the executions of the routines' runs in one thread projection. A run counts only when
    /// its message names the routine (`scheduledTaskId`) and the run's `userMessageId` is that message.
    pub async fn consume_projection(
        &self,
        projection: &Value,
        task_ids: &HashSet<String>,
    ) -> ApiResult<()> {
        let mut slot = self.state.lock().await;
        let state = self.loaded(&mut slot).await?;
        let messages: HashMap<&str, &Value> = projection["messages"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter(|m| {
                        id(m["id"].as_str()).is_some()
                            && m["scheduledTaskId"]
                                .as_str()
                                .is_some_and(|t| task_ids.contains(t))
                    })
                    .filter_map(|m| m["id"].as_str().map(|mid| (mid, m)))
                    .collect()
            })
            .unwrap_or_default();
        let mut rows = Vec::new();
        for run in projection["runs"].as_array().into_iter().flatten() {
            let Some(message) = run["userMessageId"].as_str().and_then(|m| messages.get(m)) else {
                continue;
            };
            let (Some(run_id), Some(thread_id), Some(requested_at), Some(run_status)) = (
                id(run["id"].as_str()),
                id(run["threadId"].as_str()),
                stamp(run["requestedAt"].as_str()),
                run["status"].as_str(),
            ) else {
                continue;
            };
            let Some(task_id) = message["scheduledTaskId"].as_str() else {
                continue;
            };
            let status = if TERMINAL.contains(&run_status) || ACTIVE.contains(&run_status) {
                run_status
            } else {
                "unknown"
            };
            let at = stamp(run["startedAt"].as_str()).unwrap_or(requested_at);
            rows.push(
                Row {
                    id: format!("execution:{thread_id}:{run_id}"),
                    task_id,
                    at,
                    status,
                    trigger: trigger_from_message(message["id"].as_str(), task_id),
                    thread_id: Some(thread_id),
                    run_id: Some(run_id),
                    source: "execution",
                }
                .json(),
            );
        }
        if rows.is_empty() {
            return Ok(());
        }
        merge(&mut state.rows, rows, self.limit);
        persist(&self.file, state).await
    }

    /// The recorded rows of one routine. `incomplete` is always true: history only starts when observed.
    pub async fn get(&self, task_id: &str) -> ApiResult<Value> {
        let mut slot = self.state.lock().await;
        let state = self.loaded(&mut slot).await?;
        let rows: Vec<&Value> = state
            .rows
            .iter()
            .filter(|r| r["taskId"].as_str() == Some(task_id))
            .collect();
        Ok(json!({ "observingSince": state.observing_since, "incomplete": true, "rows": rows }))
    }

    /// Forgets a deleted routine.
    pub async fn remove(&self, task_id: &str) -> ApiResult<()> {
        let mut slot = self.state.lock().await;
        let state = self.loaded(&mut slot).await?;
        state.rows.retain(|r| r["taskId"].as_str() != Some(task_id));
        self.previous.lock().await.remove(task_id);
        persist(&self.file, state).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TASK: &str = "task-1";

    fn task(last_run_at: &str, run_count: i64) -> Value {
        json!({
            "id": TASK, "projectId": "project", "threadId": "thread",
            "lastRunAt": last_run_at, "lastRunStatus": "succeeded", "runCount": run_count,
        })
    }

    fn projection(status: &str) -> Value {
        json!({
            "messages": [{
                "id": "scheduled-task-message:task-1:123:manual", "scheduledTaskId": TASK,
                "text": "PRIVATE PROMPT AND TOKEN",
            }],
            "runs": [{
                "id": "run-1", "threadId": "thread",
                "userMessageId": "scheduled-task-message:task-1:123:manual",
                "requestedAt": "2026-10-31T12:00:00Z", "startedAt": "2026-10-31T12:00:01Z",
                "status": status,
            }],
        })
    }

    fn ids() -> HashSet<String> {
        HashSet::from([TASK.to_string()])
    }

    fn scratch() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("history.json");
        (dir, file)
    }

    fn rows(value: &Value) -> Vec<Value> {
        value["rows"].as_array().cloned().unwrap_or_default()
    }

    #[tokio::test]
    async fn the_first_summary_is_not_history_and_a_later_dispatch_differs_from_an_execution() {
        let (_dir, file) = scratch();
        let history = RoutineHistory::new(file.clone(), 200);
        history
            .consume_tasks(&[task("2026-10-31T12:00:00Z", 12)])
            .await
            .unwrap();
        assert!(rows(&history.get(TASK).await.unwrap()).is_empty());

        history
            .consume_tasks(&[task("2026-10-31T13:00:00Z", 13)])
            .await
            .unwrap();
        let first = rows(&history.get(TASK).await.unwrap());
        assert_eq!(first[0]["status"], "dispatched");
        assert_eq!(first[0]["source"], "dispatch");
        assert_eq!(first[0]["runId"], Value::Null);

        history
            .consume_projection(&projection("running"), &ids())
            .await
            .unwrap();
        history
            .consume_projection(&projection("completed"), &ids())
            .await
            .unwrap();
        let all = rows(&history.get(TASK).await.unwrap());
        assert_eq!(all.len(), 2);
        assert_eq!(all[1]["status"], "completed");
        assert_eq!(all[1]["trigger"], "manual");
        assert_eq!(all[1]["runId"], "run-1");

        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("PRIVATE"));
        let reopened = RoutineHistory::new(file, 200);
        assert_eq!(
            reopened.get(TASK).await.unwrap(),
            history.get(TASK).await.unwrap()
        );
    }

    #[tokio::test]
    async fn a_run_needs_an_explicit_task_id_and_a_matching_message() {
        let (_dir, file) = scratch();
        let history = RoutineHistory::new(file, 200);
        let mut p = projection("completed");
        p["messages"][0]["scheduledTaskId"] = json!("other-task");
        history.consume_projection(&p, &ids()).await.unwrap();
        assert!(rows(&history.get(TASK).await.unwrap()).is_empty());

        p["messages"][0]["scheduledTaskId"] = json!(TASK);
        p["runs"][0]["userMessageId"] = json!("unrelated");
        history.consume_projection(&p, &ids()).await.unwrap();
        assert!(rows(&history.get(TASK).await.unwrap()).is_empty());
    }

    #[tokio::test]
    async fn retention_is_per_routine_and_skipped_failed_and_cancelled_stay_distinct() {
        let (_dir, file) = scratch();
        let history = RoutineHistory::new(file, 2);
        for (n, status) in ["completed", "failed", "cancelled"].into_iter().enumerate() {
            let mut p = projection(status);
            p["runs"][0]["id"] = json!(format!("run-1{n}"));
            p["runs"][0]["startedAt"] = json!(format!("2026-10-31T12:0{n}:01Z"));
            history.consume_projection(&p, &ids()).await.unwrap();
        }
        let result = history.get(TASK).await.unwrap();
        assert_eq!(result["incomplete"], true);
        let statuses: Vec<Value> = rows(&result).iter().map(|r| r["status"].clone()).collect();
        assert_eq!(statuses, vec![json!("cancelled"), json!("failed")]);

        // A counter alone never implies a skipped run or a completion.
        history
            .consume_tasks(&[task("2026-10-31T13:00:00Z", 1)])
            .await
            .unwrap();
        history
            .consume_tasks(&[task("2026-10-31T13:00:00Z", 99)])
            .await
            .unwrap();
        assert_eq!(rows(&history.get(TASK).await.unwrap()).len(), 2);

        history.remove(TASK).await.unwrap();
        assert!(rows(&history.get(TASK).await.unwrap()).is_empty());
    }

    #[test]
    fn triggers_come_only_from_the_message_id_of_that_routine() {
        assert_eq!(
            trigger_from_message(Some("scheduled-task-message:t:1:manual"), "t"),
            "manual"
        );
        assert_eq!(
            trigger_from_message(Some("scheduled-task-message:t:1:scheduled"), "t"),
            "scheduled"
        );
        assert_eq!(
            trigger_from_message(Some("scheduled-task-message:t:webhook:x"), "t"),
            "webhook"
        );
        assert_eq!(
            trigger_from_message(Some("scheduled-task-message:other:1:manual"), "t"),
            "unknown"
        );
        assert_eq!(trigger_from_message(None, "t"), "unknown");
    }

    #[test]
    fn an_invalid_history_file_is_an_error_not_an_empty_history() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (_dir, file) = scratch();
        std::fs::write(
            &file,
            r#"{"version":2,"observingSince":"2026-10-31T12:00:00Z","rows":[]}"#,
        )
        .unwrap();
        let history = RoutineHistory::new(file, 200);
        assert!(rt.block_on(history.get(TASK)).is_err());
    }
}
