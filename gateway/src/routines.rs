//! Routines are scheduled tasks on the harness. T3 Code is the one wired today
//! (`scheduledTasks.*`); other backends may be added.
//!
//! Tags, from `packages/contracts/src/rpc.ts` (`WS_METHODS`, the `scheduledTasks*` entries):
//! `scheduledTasks.subscribe`, `scheduledTasks.upsert`, `scheduledTasks.setEnabled`,
//! `scheduledTasks.delete`, `scheduledTasks.runNow`. `subscribe` is a stream; its first
//! item is `{ tasks }` (`ScheduledTaskListResult` in `packages/contracts/src/scheduledTask.ts`).
//! The gateway takes that chunk and interrupts the subscription. Task objects already
//! carry `lastRunAt`, `lastRunStatus`, `lastRunError` and `runCount`. There is no
//! `scheduledTasks.listRuns`. The gateway keeps a short local history of the changes it made.
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

/// Keep tasks bound to one Bot's main thread. Other tasks stay out of that Bot's sheet.
pub fn tasks_for_thread(doc: &Value, thread_id: &str) -> Vec<Value> {
    doc["tasks"]
        .as_array()
        .or_else(|| doc.as_array())
        .into_iter()
        .flatten()
        .filter(|t| t["threadId"].as_str() == Some(thread_id))
        .cloned()
        .collect()
}

/// `08:30` as "8:30 AM".
fn clock_12(time: &str) -> String {
    let (h, m) = time.split_once(':').unwrap_or((time, "00"));
    let h: u32 = h.trim().parse().unwrap_or(0) % 24;
    format!(
        "{}:{} {}",
        if h % 12 == 0 { 12 } else { h % 12 },
        m.get(..2).unwrap_or("00"),
        if h < 12 { "AM" } else { "PM" }
    )
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Which days a daily routine runs: every day, weekdays, weekends, or the days named.
fn days_label(days: &[u64]) -> Option<String> {
    let mut set: Vec<u64> = days.iter().copied().filter(|d| *d <= 6).collect();
    set.sort_unstable();
    set.dedup();
    match set.as_slice() {
        [] | [0, 1, 2, 3, 4, 5, 6] => None,
        [1, 2, 3, 4, 5] => Some("Weekdays".into()),
        [0, 6] => Some("Weekends".into()),
        other => Some(
            other
                .iter()
                .map(|d| WEEKDAYS[*d as usize])
                .collect::<Vec<_>>()
                .join(", "),
        ),
    }
}

/// What a routine's schedule says, as the panel shows it: "Every day at 8:00 AM", "Weekdays at
/// 8:00 AM", "Every 2 hours", "Once, Nov 10 at 8:00 AM", "When a webhook arrives".
pub fn schedule_label(task: &Value) -> String {
    let schedule = &task["schedule"];
    match schedule["type"].as_str() {
        Some("interval") => {
            let ms = schedule["everyMs"].as_u64().unwrap_or(0);
            if ms == 0 {
                "On a timer".into()
            } else if ms % 3_600_000 == 0 {
                let hours = ms / 3_600_000;
                if hours == 1 {
                    "Every hour".into()
                } else {
                    format!("Every {hours} hours")
                }
            } else {
                format!("Every {} minutes", (ms / 60_000).max(1))
            }
        }
        Some("fixed_time") => {
            let at = schedule["timeOfDay"]
                .as_str()
                .filter(|t| !t.is_empty())
                .map(clock_12);
            let days: Vec<u64> = schedule["weekdays"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .collect();
            let lead = days_label(&days).unwrap_or_else(|| "Every day".into());
            match at {
                Some(at) => format!("{lead} at {at}"),
                None => lead,
            }
        }
        Some("once") => {
            let run = schedule["runAt"].as_str().unwrap_or("");
            let date = run
                .get(5..7)
                .and_then(|m| m.parse::<usize>().ok())
                .filter(|m| (1..=12).contains(m))
                .zip(run.get(8..10));
            match (date, run.get(11..16)) {
                (Some((m, d)), Some(t)) => format!(
                    "Once, {} {} at {}",
                    MONTHS[m - 1],
                    d.trim_start_matches('0'),
                    clock_12(t)
                ),
                _ => "Once".into(),
            }
        }
        Some("webhook") => "When a webhook arrives".into(),
        _ => "Scheduled".into(),
    }
}

/// A routine bound to this Bot. It starts off. Webhook schedules are not built here.
pub fn new_routine(bot: &Value, title: &str, kind: &str, now: u64) -> Result<Value, &'static str> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Give the routine a name.");
    }
    let project = bot["projectId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("This Bot is still getting set up.")?;
    let thread = bot["mainThreadId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("This Bot is still getting set up.")?;
    let model = if bot["model"].is_object() {
        bot["model"].clone()
    } else {
        crate::provisioner::default_model()
    };
    let schedule = match kind {
        "hour" => json!({ "type": "interval", "everyMs": 3_600_000u64 }),
        "day" => json!({ "type": "fixed_time", "timeOfDay": "09:00" }),
        "once" => json!({ "type": "once", "runAt": format_unix(now.saturating_add(86_400)) }),
        "interval" => {
            let ms = bot["everyMs"].as_u64().unwrap_or(3_600_000);
            if ms < 60_000 {
                return Err("A routine repeats no faster than once a minute.");
            }
            json!({ "type": "interval", "everyMs": ms })
        }
        _ => return Err("Pick hourly, daily, or once."),
    };
    Ok(json!({
        "title": title,
        "prompt": title,
        "enabled": false,
        "schedule": schedule,
        "projectId": project,
        "threadId": thread,
        "threadMode": "continue",
        "workspaceStrategy": { "type": "root" },
        "modelSelection": model,
        "runtimeMode": "approval-required",
        "interactionMode": "default",
        "createdBy": "user",
        "creationSource": "web",
    }))
}

pub fn task_row(task: &Value) -> Value {
    json!({
        "id": task["id"].as_str().unwrap_or(""),
        "title": task["title"].as_str().unwrap_or("Routine"),
        "enabled": task["enabled"] == true,
        "paused": task["enabled"] != true,
        "schedule": schedule_label(task),
        "nextRunAt": task["nextRunAt"].as_str().unwrap_or(""),
        "lastRunAt": task["lastRunAt"].as_str().unwrap_or(""),
        "lastStatus": task["lastRunStatus"].as_str().unwrap_or("never"),
        "lastError": task["lastRunError"].as_str().unwrap_or(""),
        "blocked": task["blockedByPolicy"].is_string(),
    })
}

pub async fn list(t3: &T3) -> ApiResult<Value> {
    let value = t3
        .rpc()
        .await?
        .read_until(LIST, json!({}), |_| true)
        .await?;
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
    fn a_bot_only_sees_its_own_routines() {
        let doc = json!({ "tasks": [
            { "id": "a", "title": "Morning", "threadId": "t1", "enabled": false, "schedule": { "type": "interval", "everyMs": 3_600_000 } },
            { "id": "b", "title": "Other", "threadId": "t2", "enabled": true, "schedule": { "type": "once", "runAt": "2026-01-01T00:00:00Z" } }
        ]});
        let mine = tasks_for_thread(&doc, "t1");
        assert_eq!(mine.len(), 1);
        assert_eq!(task_row(&mine[0])["schedule"], "Every hour");
        assert_eq!(schedule_label(&doc["tasks"][1]), "Once, Jan 1 at 12:00 AM");
        assert_eq!(
            schedule_label(&json!({ "schedule": { "type": "fixed_time", "timeOfDay": "09:00" } })),
            "Every day at 9:00 AM"
        );
    }

    #[test]
    fn schedules_read_the_way_a_person_says_them() {
        let at = |s: Value| schedule_label(&json!({ "schedule": s }));
        let daily = |days: Value, time: &str| {
            at(json!({ "type": "fixed_time", "timeOfDay": time, "weekdays": days }))
        };
        assert_eq!(
            daily(json!([1, 2, 3, 4, 5]), "08:00"),
            "Weekdays at 8:00 AM"
        );
        assert_eq!(daily(json!([0, 6]), "17:21"), "Weekends at 5:21 PM");
        assert_eq!(daily(json!([1, 3]), "00:05"), "Mon, Wed at 12:05 AM");
        assert_eq!(
            daily(json!([0, 1, 2, 3, 4, 5, 6]), "12:00"),
            "Every day at 12:00 PM"
        );
        assert_eq!(daily(json!([]), "13:30"), "Every day at 1:30 PM");
        assert_eq!(
            at(json!({ "type": "interval", "everyMs": 7_200_000 })),
            "Every 2 hours"
        );
        assert_eq!(
            at(json!({ "type": "interval", "everyMs": 1_800_000 })),
            "Every 30 minutes"
        );
        assert_eq!(
            at(json!({ "type": "once", "runAt": "2026-11-10T08:00:00+01:00" })),
            "Once, Nov 10 at 8:00 AM"
        );
        assert_eq!(at(json!({ "type": "webhook" })), "When a webhook arrives");
    }

    #[test]
    fn a_paused_routine_with_a_failed_run_says_so() {
        let made: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/t3/routine-created.json"))
                .unwrap();
        let row = task_row(&made["taskRow"]);
        assert_eq!(
            (row["paused"].as_bool(), row["lastStatus"].as_str()),
            (Some(true), Some("never"))
        );
        assert_eq!(row["schedule"], "Weekdays at 8:00 AM");
        let failed = task_row(
            &json!({ "id": "x", "enabled": true, "lastRunStatus": "failed", "lastRunError": "No model", "nextRunAt": "2026-11-11T08:00:00Z", "schedule": { "type": "interval", "everyMs": 3_600_000 } }),
        );
        assert_eq!(
            (
                failed["paused"].as_bool(),
                failed["lastStatus"].as_str(),
                failed["lastError"].as_str()
            ),
            (Some(false), Some("failed"), Some("No model"))
        );
        assert_eq!(failed["nextRunAt"], "2026-11-11T08:00:00Z");
    }

    #[test]
    fn a_new_routine_is_off_and_bound_to_the_bot() {
        let bot = json!({
            "projectId": "p1",
            "mainThreadId": "t1",
            "model": { "instanceId": "claudeAgent", "model": "claude-haiku-5-5" }
        });
        let task = new_routine(&bot, "Digest", "day", 1_700_000_000).unwrap();
        assert_eq!(task["enabled"], false);
        assert_eq!(task["threadId"], "t1");
        assert_eq!(task["projectId"], "p1");
        assert_eq!(task["threadMode"], "continue");
        assert_eq!(task["runtimeMode"], "approval-required");
        assert_eq!(task["schedule"]["timeOfDay"], "09:00");
        let once = new_routine(&bot, "Later", "once", 0).unwrap();
        assert_eq!(once["schedule"]["type"], "once");
        assert!(once["schedule"]["runAt"].as_str().unwrap().ends_with('Z'));
        assert!(new_routine(&json!({}), "Digest", "hour", 0).is_err());
        assert!(new_routine(&bot, "  ", "hour", 0).is_err());
        let mut fast = bot.clone();
        fast["everyMs"] = json!(1000);
        assert!(new_routine(&fast, "Digest", "interval", 0).is_err());
    }

    #[test]
    fn a_known_unix_second_formats() {
        assert_eq!(format_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn hourly_interval_and_a_missing_model_stay_inside_the_rules() {
        let bot = json!({ "projectId": "p1", "mainThreadId": "t1" });
        let hour = new_routine(&bot, "Digest", "hour", 0).unwrap();
        assert_eq!(hour["schedule"]["everyMs"], 3_600_000u64);
        assert_eq!(hour["modelSelection"]["model"], "claude-haiku-5-5");
        assert_eq!(hour["enabled"], false);
        assert_eq!(hour["workspaceStrategy"]["type"], "root");
        let mut slow = bot.clone();
        slow["everyMs"] = json!(60_000);
        let interval = new_routine(&slow, "Pulse", "interval", 0).unwrap();
        assert_eq!(interval["schedule"]["everyMs"], 60_000u64);
        assert_eq!(
            new_routine(&bot, "Nope", "weekly", 0).unwrap_err(),
            "Pick hourly, daily, or once."
        );
        assert_eq!(
            new_routine(&json!({ "projectId": "p1" }), "Digest", "hour", 0).unwrap_err(),
            "This Bot is still getting set up."
        );
    }
}
