//! Routines are scheduled tasks on the harness. T3 Code is the one wired today;
//! other backends may be added.
//!
//! The harness lists tasks and applies upsert, enable, delete, and run. Task objects
//! already carry `lastRunAt`, `lastRunStatus`, `lastRunError` and `runCount`. The
//! gateway keeps a short local history of the changes it made. Labels and clocks stay here.

use crate::bots::runtime_mode::{clamp_label, RuntimeMode};
use crate::harness::{HarnessError, Routines};
use crate::platform::error::{ApiError, ApiResult};
use serde_json::{json, Value};
use std::path::PathBuf;

#[cfg(test)]
mod tests;

const HISTORY_LIMIT: usize = 50;

#[derive(Debug)]
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
        let _ = crate::storage::write_json(&self.file, &Value::Array(items)).await;
    }
}

/// Keep tasks bound to one Bot's main thread. Other tasks stay out of that Bot's sheet.
pub(crate) fn tasks_for_thread(doc: &Value, thread_id: &str) -> Vec<Value> {
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
        if h.is_multiple_of(12) { 12 } else { h % 12 },
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
                .map(|d| WEEKDAYS[usize::try_from(*d).unwrap_or(0)])
                .collect::<Vec<_>>()
                .join(", "),
        ),
    }
}

/// What a routine's schedule says, as the panel shows it: "Every day at 8:00 AM", "Weekdays at
/// 8:00 AM", "Every 2 hours", "Once, Nov 10 at 8:00 AM", "When a webhook arrives".
pub(crate) fn schedule_label(task: &Value) -> String {
    let schedule = &task["schedule"];
    match schedule["type"].as_str() {
        Some("interval") => {
            let ms = schedule["everyMs"].as_u64().unwrap_or(0);
            if ms == 0 {
                "On a timer".into()
            } else if ms.is_multiple_of(3_600_000) {
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
pub(crate) fn new_routine(
    bot: &Value,
    title: &str,
    kind: &str,
    now: u64,
    cap: RuntimeMode,
) -> Result<Value, &'static str> {
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
        crate::bots::provisioner::default_model()
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
    let mut task = json!({
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
    });
    clamp_task_mode(&mut task, cap);
    Ok(task)
}

/// A known `runtimeMode` on a routine write is stored at or below the cap.
/// An unknown mode is left for the harness to reject.
pub(crate) fn clamp_task_mode(task: &mut Value, cap: RuntimeMode) {
    let Some(mode) = task.get("runtimeMode").and_then(Value::as_str) else {
        return;
    };
    let Some(clamped) = clamp_label(mode, cap) else {
        return;
    };
    task["runtimeMode"] = json!(clamped);
}

/// A wizard entry is either a preset or a local time, one weekday, and a prompt.
/// `/api/routines` forwards that same pair of shapes once the thread exists.
pub(crate) fn validate_pending_routine(entry: &Value) -> Result<(), &'static str> {
    let _ = pending_schedule(entry)?;
    Ok(())
}

fn pending_schedule(entry: &Value) -> Result<Value, &'static str> {
    let prompt = entry["prompt"]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or("A routine needs a prompt.")?;
    if prompt.chars().count() > 2000 {
        return Err("A routine prompt is too long.");
    }
    if let Some(preset) = entry.get("preset") {
        if entry.get("time").is_some() || entry.get("weekday").is_some() {
            return Err("A routine is a preset or a time and weekday.");
        }
        let schedule = match preset.as_str() {
            Some("hour") => json!({ "type": "interval", "everyMs": 3_600_000u64 }),
            Some("day") => json!({ "type": "fixed_time", "timeOfDay": "09:00" }),
            Some("weekdays") => {
                json!({ "type": "fixed_time", "timeOfDay": "09:00", "weekdays": [1, 2, 3, 4, 5] })
            }
            _ => return Err("Pick a routine preset."),
        };
        return Ok(schedule);
    }
    let time = entry["time"]
        .as_str()
        .map(str::trim)
        .filter(|time| clock_24(time))
        .ok_or("A routine needs a time.")?;
    let weekday = entry["weekday"]
        .as_u64()
        .filter(|day| *day <= 6)
        .ok_or("A routine needs a weekday.")?;
    Ok(json!({ "type": "fixed_time", "timeOfDay": time, "weekdays": [weekday] }))
}

fn clock_24(time: &str) -> bool {
    let Some((hour, minute)) = time.split_once(':') else {
        return false;
    };
    hour.len() == 2
        && minute.len() == 2
        && hour.parse::<u32>().is_ok_and(|hour| hour <= 23)
        && minute.parse::<u32>().is_ok_and(|minute| minute <= 59)
        && hour.chars().all(|c| c.is_ascii_digit())
        && minute.chars().all(|c| c.is_ascii_digit())
}

/// The task body for one stored entry, bound to this thread.
pub(crate) fn pending_routine_task(
    project_id: &str,
    thread_id: &str,
    model: &Value,
    entry: &Value,
    cap: RuntimeMode,
) -> Result<Value, &'static str> {
    if project_id.is_empty() || thread_id.is_empty() {
        return Err("This Bot is still getting set up.");
    }
    let prompt = entry["prompt"].as_str().unwrap_or("").trim();
    let schedule = pending_schedule(entry)?;
    let model = if model.is_object() {
        model.clone()
    } else {
        crate::bots::provisioner::default_model()
    };
    let mut task = json!({
        "title": prompt,
        "prompt": prompt,
        "enabled": true,
        "schedule": schedule,
        "projectId": project_id,
        "threadId": thread_id,
        "threadMode": "continue",
        "workspaceStrategy": { "type": "root" },
        "modelSelection": model,
        "runtimeMode": "approval-required",
        "interactionMode": "default",
        "createdBy": "user",
        "creationSource": "web",
    });
    clamp_task_mode(&mut task, cap);
    Ok(task)
}

pub(crate) fn task_row(task: &Value) -> Value {
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

fn api(err: HarnessError) -> ApiError {
    ApiError::new(500, err.to_string())
}

pub(crate) async fn list(routines: &dyn Routines) -> ApiResult<Value> {
    routines.list().await.map_err(api)
}

pub(crate) async fn change(
    routines: &dyn Routines,
    history: &RoutineHistory,
    action: &str,
    payload: Value,
) -> ApiResult<Value> {
    let value = match action {
        "upsert" => routines.upsert(&payload).await.map_err(api)?,
        "enabled" => routines.set_enabled(&payload).await.map_err(api)?,
        "delete" => routines.delete(&payload).await.map_err(api)?,
        "run" => routines.run_now(&payload).await.map_err(api)?,
        _ => return Err(ApiError::bad_request("Unknown routine action")),
    };
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
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
    let tod = secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = i64::try_from(yoe).unwrap_or(0) + era * 400;
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
