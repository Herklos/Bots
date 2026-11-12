use super::super::*;
use super::{agent, ask, hold, sealed, LATER, NOW};
use crate::bots::runtime_mode::RuntimeMode;
use serde_json::{json, Value};

#[test]
fn the_baseline_grandfathers_tasks_that_already_exist() {
    let mut store = ReviewStore::empty();
    let old = agent("old", 60_000);
    assert_eq!(decide(&old, &store, Some(&ask())), Action::Leave);
    store.seal_baseline(["old", ""], NOW);
    assert_eq!(store.baseline.as_deref(), Some(NOW));
    assert!(store.grandfathered.contains("old"));
    assert!(!store.grandfathered.contains(""));
    assert_eq!(decide(&old, &store, Some(&ask())), Action::Leave);
    note_run(&mut store, "old", RunStatus::Failed);
    note_run(&mut store, "old", RunStatus::Failed);
    note_run(&mut store, "old", RunStatus::Failed);
    assert_eq!(decide(&old, &store, Some(&ask())), Action::Leave);

    let fresh = agent("new", 3_600_000);
    assert_eq!(decide(&fresh, &store, Some(&ask())), hold());
    assert_eq!(decide(&fresh, &store, None), Action::Leave);
    assert_eq!(
        decide(&json!({ "title": "Morning digest" }), &store, Some(&ask())),
        Action::Leave
    );
    store.seal_baseline(["new"], LATER);
    assert_eq!(store.baseline.as_deref(), Some(NOW));
    assert!(!store.grandfathered.contains("new"));
}

#[test]
fn a_routine_a_person_created_is_left_alone() {
    let store = sealed(&[]);
    let mut ctx = ask();
    ctx.enabled = DEFAULT_MAX_ROUTINES;
    ctx.pending = DEFAULT_MAX_PENDING;
    ctx.runtime_cap = RuntimeMode::ApprovalRequired;
    let cases = [
        ("user", "web"),
        ("user", "mobile"),
        ("system", "server"),
        ("agent", "server"),
        ("agent", "web"),
    ];
    for (by, source) in cases {
        let mut task = agent(by, 60_000);
        task["createdBy"] = json!(by);
        task["creationSource"] = json!(source);
        task["runtimeMode"] = json!("full-access");
        assert!(!agent_authored(&task), "{by}/{source}");
        assert_eq!(decide(&task, &store, Some(&ctx)), Action::Leave);
    }
    let mut missing = agent("bare", 60_000);
    missing["creationSource"] = Value::Null;
    assert!(!agent_authored(&missing));
    assert_eq!(decide(&missing, &store, Some(&ctx)), Action::Leave);
}

#[test]
fn trust_mode_leaves_an_agent_routine_alone() {
    let mut store = sealed(&[]);
    let mut ctx = ask();
    ctx.mode = ReviewMode::Trust;
    ctx.runtime_cap = RuntimeMode::ApprovalRequired;
    ctx.enabled = DEFAULT_MAX_ROUTINES;
    ctx.pending = DEFAULT_MAX_PENDING;
    let mut task = agent("t", 60_000);
    task["runtimeMode"] = json!("full-access");
    task["schedule"] = json!({ "type": "webhook" });
    assert_eq!(decide(&task, &store, Some(&ctx)), Action::Leave);

    record(&mut store, "t", "ada", &hold(), NOW);
    assert_eq!(
        resolve(&mut store, "t", Choice::Approve, LATER),
        Resolution::Enable
    );
    note_run(&mut store, "t", RunStatus::Failed);
    note_run(&mut store, "t", RunStatus::Failed);
    note_run(&mut store, "t", RunStatus::Failed);
    task["id"] = json!("t");
    assert_eq!(decide(&task, &store, Some(&ctx)), Action::Leave);
}

#[test]
fn ask_mode_holds_an_agent_routine() {
    let mut store = sealed(&[]);
    let task = agent("t", 3_600_000);
    assert!(agent_authored(&task));
    assert_eq!(decide(&task, &store, Some(&ask())), hold());
    let mut provider = task.clone();
    provider["id"] = json!("p");
    provider["creationSource"] = json!("provider");
    assert_eq!(decide(&provider, &store, Some(&ask())), hold());

    record(&mut store, "t", "ada", &hold(), NOW);
    record(&mut store, "t", "ada", &hold(), LATER);
    assert_eq!(store.items["t"].flagged_at, NOW);
    assert_eq!(store.items["t"].state, ReviewState::Pending);
    assert_eq!(decide(&task, &store, Some(&ask())), Action::Leave);
    let prompt = serde_json::to_string(&store.items["t"]).unwrap();
    assert!(!prompt.contains("secret prompt text"));
}

#[test]
fn an_extra_routine_past_either_cap_is_deleted() {
    assert!(routine_cap_exceeded(
        DEFAULT_MAX_ROUTINES,
        DEFAULT_MAX_ROUTINES
    ));
    assert!(!routine_cap_exceeded(
        DEFAULT_MAX_ROUTINES - 1,
        DEFAULT_MAX_ROUTINES
    ));
    assert!(pending_cap_exceeded(
        DEFAULT_MAX_PENDING,
        DEFAULT_MAX_PENDING
    ));
    assert!(!pending_cap_exceeded(
        DEFAULT_MAX_PENDING - 1,
        DEFAULT_MAX_PENDING
    ));

    let mut store = sealed(&[]);
    let task = agent("n", 3_600_000);
    let mut ctx = ask();
    ctx.enabled = DEFAULT_MAX_ROUTINES - 1;
    assert_eq!(decide(&task, &store, Some(&ctx)), hold());
    ctx.enabled = DEFAULT_MAX_ROUTINES;
    ctx.pending = 0;
    assert_eq!(
        decide(&task, &store, Some(&ctx)),
        Action::Delete {
            reason: Reason::RoutineCap,
        }
    );
    ctx.enabled = 0;
    ctx.pending = DEFAULT_MAX_PENDING;
    assert_eq!(
        decide(&task, &store, Some(&ctx)),
        Action::Delete {
            reason: Reason::PendingCap,
        }
    );
    ctx.enabled = 4;
    ctx.pending = DEFAULT_MAX_PENDING;
    assert_eq!(
        decide(&task, &store, Some(&ctx)),
        Action::Delete {
            reason: Reason::PendingCap,
        }
    );
    ctx.enabled = 6;
    assert_eq!(
        decide(&task, &store, Some(&ctx)),
        Action::Delete {
            reason: Reason::RoutineCap,
        }
    );

    let mut webhook = task.clone();
    webhook["schedule"] = json!({ "type": "webhook" });
    ctx.enabled = 0;
    ctx.pending = DEFAULT_MAX_PENDING;
    assert_eq!(
        decide(&webhook, &store, Some(&ctx)),
        Action::Delete {
            reason: Reason::PendingCap,
        }
    );

    record(
        &mut store,
        "n",
        "ada",
        &Action::Delete {
            reason: Reason::RoutineCap,
        },
        LATER,
    );
    assert_eq!(store.items["n"].state, ReviewState::Rejected);
    assert_eq!(store.items["n"].reason, "routine-cap");
    assert_eq!(decide(&task, &store, Some(&ctx)), Action::Leave);

    record(&mut store, "kept", "ada", &hold(), NOW);
    record(
        &mut store,
        "kept",
        "other",
        &Action::Delete {
            reason: Reason::PendingCap,
        },
        LATER,
    );
    assert_eq!(store.items["kept"].slug, "ada");
    assert_eq!(store.items["kept"].flagged_at, NOW);
    assert_eq!(store.items["kept"].decided_at.as_deref(), Some(LATER));
    store.items.get_mut("kept").unwrap().slug.clear();
    record(
        &mut store,
        "kept",
        "other",
        &Action::Delete {
            reason: Reason::PendingCap,
        },
        LATER,
    );
    assert_eq!(store.items["kept"].slug, "other");
}

#[test]
fn an_interval_faster_than_fifteen_minutes_is_raised() {
    let fast = agent("t", 60_000);
    let floor = agent("t", INTERVAL_FLOOR_MS);
    let slower = agent("t", INTERVAL_FLOOR_MS + 60_000);
    assert_eq!(
        raised_interval(&fast, INTERVAL_FLOOR_MS),
        Some(INTERVAL_FLOOR_MS)
    );
    assert_eq!(
        raised_interval(&agent("t", INTERVAL_FLOOR_MS - 1), INTERVAL_FLOOR_MS),
        Some(INTERVAL_FLOOR_MS)
    );
    assert_eq!(raised_interval(&floor, INTERVAL_FLOOR_MS), None);
    assert_eq!(raised_interval(&slower, INTERVAL_FLOOR_MS), None);
    assert_eq!(
        raised_interval(
            &json!({ "schedule": { "type": "fixed_time", "timeOfDay": "09:00" } }),
            INTERVAL_FLOOR_MS
        ),
        None
    );
    assert_eq!(
        raised_interval(
            &json!({ "schedule": { "type": "once", "runAt": NOW } }),
            INTERVAL_FLOOR_MS
        ),
        None
    );
    assert_eq!(
        raised_interval(
            &json!({ "schedule": { "type": "interval" } }),
            INTERVAL_FLOOR_MS
        ),
        Some(INTERVAL_FLOOR_MS)
    );

    let store = sealed(&[]);
    assert_eq!(
        decide(&fast, &store, Some(&ask())),
        Action::Hold {
            reason: Reason::AgentCreated,
            every_ms: Some(INTERVAL_FLOOR_MS),
            runtime_mode: None,
        }
    );
    assert_eq!(decide(&floor, &store, Some(&ask())), hold());

    let mut pending = sealed(&[]);
    record(&mut pending, "t", "ada", &hold(), NOW);
    assert_eq!(
        decide(&fast, &pending, Some(&ask())),
        Action::Revise {
            every_ms: Some(INTERVAL_FLOOR_MS),
            runtime_mode: None,
        }
    );
    let before = pending.clone();
    record(
        &mut pending,
        "t",
        "ada",
        &Action::Revise {
            every_ms: Some(INTERVAL_FLOOR_MS),
            runtime_mode: None,
        },
        LATER,
    );
    assert_eq!(pending, before);

    assert_eq!(
        resolve(&mut pending, "t", Choice::Approve, LATER),
        Resolution::Enable
    );
    assert_eq!(
        decide(&fast, &pending, Some(&ask())),
        Action::Revise {
            every_ms: Some(INTERVAL_FLOOR_MS),
            runtime_mode: None,
        }
    );
}

#[test]
fn a_broader_runtime_mode_is_clamped_to_the_bot() {
    let mut broad = agent("t", 3_600_000);
    broad["runtimeMode"] = json!("full-access");
    assert_eq!(clamped_runtime(&broad, RuntimeMode::Auto), Some("auto"));
    assert_eq!(
        clamped_runtime(&broad, RuntimeMode::ApprovalRequired),
        Some("approval-required")
    );
    assert_eq!(clamped_runtime(&broad, RuntimeMode::FullAccess), None);
    let mut within = broad.clone();
    within["runtimeMode"] = json!("approval-required");
    assert_eq!(clamped_runtime(&within, RuntimeMode::Auto), None);
    let mut unknown = broad.clone();
    unknown["runtimeMode"] = json!("ask");
    assert_eq!(clamped_runtime(&unknown, RuntimeMode::Auto), None);
    let mut missing = broad.clone();
    missing.as_object_mut().unwrap().remove("runtimeMode");
    assert_eq!(
        clamped_runtime(&missing, RuntimeMode::ApprovalRequired),
        None
    );

    let mut store = sealed(&[]);
    let mut ctx = ask();
    ctx.runtime_cap = RuntimeMode::Auto;
    assert_eq!(
        decide(&broad, &store, Some(&ctx)),
        Action::Hold {
            reason: Reason::AgentCreated,
            every_ms: None,
            runtime_mode: Some("auto"),
        }
    );
    let mut both = agent("t", 60_000);
    both["runtimeMode"] = json!("full-access");
    ctx.runtime_cap = RuntimeMode::ApprovalRequired;
    assert_eq!(
        decide(&both, &store, Some(&ctx)),
        Action::Hold {
            reason: Reason::AgentCreated,
            every_ms: Some(INTERVAL_FLOOR_MS),
            runtime_mode: Some("approval-required"),
        }
    );

    record(&mut store, "t", "ada", &hold(), NOW);
    assert_eq!(
        resolve(&mut store, "t", Choice::Approve, LATER),
        Resolution::Enable
    );
    assert_eq!(
        decide(&broad, &store, Some(&ctx)),
        Action::Revise {
            every_ms: None,
            runtime_mode: Some("approval-required"),
        }
    );
}

#[test]
fn three_failed_runs_pause_the_routine() {
    let mut store = sealed(&[]);
    record(&mut store, "t", "ada", &hold(), NOW);
    assert_eq!(
        resolve(&mut store, "t", Choice::Approve, LATER),
        Resolution::Enable
    );
    let task = agent("t", INTERVAL_FLOOR_MS);
    note_run(&mut store, "t", RunStatus::Failed);
    note_run(&mut store, "t", RunStatus::Failed);
    note_run(&mut store, "", RunStatus::Failed);
    assert_eq!(store.failure_count("t"), 2);
    assert_eq!(decide(&task, &store, Some(&ask())), Action::Leave);
    note_run(&mut store, "t", RunStatus::Running);
    note_run(&mut store, "t", run_status(Some("never")));
    assert_eq!(store.failure_count("t"), 2);
    note_run(&mut store, "t", RunStatus::Failed);
    assert_eq!(decide(&task, &store, Some(&ask())), Action::Pause);
    let before = store.clone();
    record(&mut store, "t", "ada", &Action::Pause, LATER);
    assert_eq!(store.items["t"].state, before.items["t"].state);

    let mut paused = task.clone();
    paused["enabled"] = json!(false);
    assert_eq!(decide(&paused, &store, Some(&ask())), Action::Leave);

    note_run(&mut store, "t", RunStatus::Succeeded);
    assert_eq!(store.failure_count("t"), 0);
    paused["enabled"] = json!(true);
    assert_eq!(decide(&paused, &store, Some(&ask())), Action::Leave);

    note_run(&mut store, "t", RunStatus::Failed);
    note_run(&mut store, "t", RunStatus::Failed);
    note_run(&mut store, "t", RunStatus::Failed);
    clear_failures(&mut store, "t");
    assert_eq!(decide(&paused, &store, Some(&ask())), Action::Leave);
}

#[test]
fn rejecting_a_routine_deletes_it_and_approving_clears_the_streak() {
    let mut store = sealed(&[]);
    record(&mut store, "t", "ada", &hold(), NOW);
    note_run(&mut store, "t", RunStatus::Failed);
    assert_eq!(
        resolve(&mut store, "missing", Choice::Reject, LATER),
        Resolution::Absent
    );
    assert_eq!(
        resolve(&mut store, "t", Choice::Reject, LATER),
        Resolution::Delete
    );
    assert_eq!(store.items["t"].state, ReviewState::Rejected);
    assert_eq!(store.items["t"].decided_at.as_deref(), Some(LATER));
    assert_eq!(store.items["t"].flagged_at, NOW);
    assert_eq!(store.failure_count("t"), 0);
    assert_eq!(
        resolve(&mut store, "t", Choice::Approve, LATER),
        Resolution::Absent
    );
    assert_eq!(
        decide(&agent("t", 60_000), &store, Some(&ask())),
        Action::Leave
    );

    let mut store = sealed(&[]);
    record(&mut store, "u", "bea", &hold(), NOW);
    note_run(&mut store, "u", RunStatus::Failed);
    note_run(&mut store, "u", RunStatus::Failed);
    note_run(&mut store, "u", RunStatus::Failed);
    assert_eq!(
        resolve(&mut store, "u", Choice::Approve, LATER),
        Resolution::Enable
    );
    assert_eq!(store.items["u"].state, ReviewState::Approved);
    assert_eq!(store.items["u"].reason, "agent-created");
    assert_eq!(store.failure_count("u"), 0);
    assert_eq!(
        decide(&agent("u", INTERVAL_FLOOR_MS), &store, Some(&ask())),
        Action::Leave
    );
}

#[test]
fn a_webhook_from_an_agent_stays_pending() {
    let store = sealed(&[]);
    let mut task = agent("w", 60_000);
    task["schedule"] = json!({ "type": "webhook" });
    task["creationSource"] = json!("provider");
    task["runtimeMode"] = json!("full-access");
    let mut ctx = ask();
    ctx.runtime_cap = RuntimeMode::Auto;
    assert_eq!(
        decide(&task, &store, Some(&ctx)),
        Action::Hold {
            reason: Reason::Webhook,
            every_ms: None,
            runtime_mode: Some("auto"),
        }
    );
    assert_eq!(Reason::Webhook.as_str(), "webhook");
}
