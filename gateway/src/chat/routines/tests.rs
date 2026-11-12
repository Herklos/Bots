use super::*;
use crate::bots::runtime_mode::RuntimeMode;

fn routine(bot: &Value, title: &str, kind: &str, now: u64) -> Result<Value, &'static str> {
    new_routine(bot, title, kind, now, RuntimeMode::FullAccess)
}

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
    let made: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/t3/routine-created.json"
    ))
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
fn a_pending_routine_is_a_preset_or_a_time_and_weekday() {
    assert!(
        validate_pending_routine(&json!({ "preset": "hour", "prompt": "Check mail." })).is_ok()
    );
    assert!(
        validate_pending_routine(&json!({ "preset": "hourly", "prompt": "Check mail." })).is_err()
    );
    assert!(
        validate_pending_routine(&json!({ "time": "08:30", "weekday": 1, "prompt": "Plan." }))
            .is_ok()
    );
    assert!(
        validate_pending_routine(&json!({ "time": "8:30", "weekday": 1, "prompt": "Plan." }))
            .is_err()
    );
    assert!(
        validate_pending_routine(&json!({ "time": "08:30", "weekday": 7, "prompt": "Plan." }))
            .is_err()
    );
    let task = pending_routine_task(
        "p1",
        "t1",
        &Value::Null,
        &json!({ "preset": "weekdays", "prompt": "Send the note." }),
        RuntimeMode::FullAccess,
    )
    .unwrap();
    assert_eq!(task["threadId"], "t1");
    assert_eq!(task["projectId"], "p1");
    assert_eq!(task["schedule"]["weekdays"], json!([1, 2, 3, 4, 5]));
    assert_eq!(task["enabled"], true);
    assert_eq!(task["runtimeMode"], "approval-required");
}

#[test]
fn a_new_routine_is_off_and_bound_to_the_bot() {
    let bot = json!({
        "projectId": "p1",
        "mainThreadId": "t1",
        "model": { "instanceId": "claudeAgent", "model": "claude-haiku-5-5" }
    });
    let task = routine(&bot, "Digest", "day", 1_700_000_000).unwrap();
    assert_eq!(task["enabled"], false);
    assert_eq!(task["threadId"], "t1");
    assert_eq!(task["projectId"], "p1");
    assert_eq!(task["threadMode"], "continue");
    assert_eq!(task["runtimeMode"], "approval-required");
    assert_eq!(task["schedule"]["timeOfDay"], "09:00");
    let once = routine(&bot, "Later", "once", 0).unwrap();
    assert_eq!(once["schedule"]["type"], "once");
    assert!(once["schedule"]["runAt"].as_str().unwrap().ends_with('Z'));
    assert!(routine(&json!({}), "Digest", "hour", 0).is_err());
    assert!(routine(&bot, "  ", "hour", 0).is_err());
    let mut fast = bot.clone();
    fast["everyMs"] = json!(1000);
    assert!(routine(&fast, "Digest", "interval", 0).is_err());
}

#[test]
fn a_known_unix_second_formats() {
    assert_eq!(format_unix(0), "1970-01-01T00:00:00Z");
    assert_eq!(format_unix(1_700_000_000), "2023-11-14T22:13:20Z");
}

#[test]
fn hourly_interval_and_a_missing_model_stay_inside_the_rules() {
    let bot = json!({ "projectId": "p1", "mainThreadId": "t1" });
    let hour = routine(&bot, "Digest", "hour", 0).unwrap();
    assert_eq!(hour["schedule"]["everyMs"], 3_600_000u64);
    assert_eq!(hour["modelSelection"]["model"], "claude-haiku-5-5");
    assert_eq!(hour["enabled"], false);
    assert_eq!(hour["workspaceStrategy"]["type"], "root");
    let mut slow = bot.clone();
    slow["everyMs"] = json!(60_000);
    let interval = routine(&slow, "Pulse", "interval", 0).unwrap();
    assert_eq!(interval["schedule"]["everyMs"], 60_000u64);
    assert_eq!(
        routine(&bot, "Nope", "weekly", 0).unwrap_err(),
        "Pick hourly, daily, or once."
    );
    assert_eq!(
        routine(&json!({ "projectId": "p1" }), "Digest", "hour", 0).unwrap_err(),
        "This Bot is still getting set up."
    );
}

#[test]
fn a_routine_write_cannot_exceed_the_cap() {
    let mut task = json!({ "runtimeMode": "full-access", "title": "Digest" });
    clamp_task_mode(&mut task, RuntimeMode::Auto);
    assert_eq!(task["runtimeMode"], "auto");
    assert_eq!(task["title"], "Digest");
    let mut unknown = json!({ "runtimeMode": "ask" });
    clamp_task_mode(&mut unknown, RuntimeMode::Auto);
    assert_eq!(unknown["runtimeMode"], "ask");
    let mut missing = json!({ "title": "Digest" });
    clamp_task_mode(&mut missing, RuntimeMode::ApprovalRequired);
    assert!(missing.get("runtimeMode").is_none());
    let bot = json!({ "projectId": "p1", "mainThreadId": "t1" });
    let made = new_routine(&bot, "Digest", "hour", 0, RuntimeMode::ApprovalRequired).unwrap();
    assert_eq!(made["runtimeMode"], "approval-required");
}
