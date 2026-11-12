use super::*;

#[test]
pub(super) fn settling_records_what_the_last_update_did() {
    let at = "2026-11-09T10:00:00Z";
    let pending = Pending {
        from: "1.0.0".into(),
        to: "1.1.0".into(),
        url: "u".into(),
        at: at.into(),
    };
    let mut state = AutoState {
        auto: true,
        pending: Some(pending.clone()),
        ..Default::default()
    };
    let line = settle(&mut state, "1.1.0", &|_| false, at).unwrap();
    assert!(line.contains("now running 1.1.0"));
    assert_eq!(state.notice.as_ref().unwrap().kind, "updated");
    assert!(state.pending.is_none());

    let mut state = AutoState {
        pending: Some(pending.clone()),
        ..Default::default()
    };
    settle(&mut state, "1.0.0", &|v| v == "1.1.0", at).unwrap();
    assert_eq!(state.notice.as_ref().unwrap().kind, "rolledBack");
    assert_eq!(state.skip, vec!["1.1.0".to_string()]);

    let mut state = AutoState {
        pending: Some(pending),
        ..Default::default()
    };
    settle(&mut state, "1.0.0", &|_| false, at).unwrap();
    assert_eq!(state.notice.as_ref().unwrap().kind, "failed");
    assert!(settle(&mut AutoState::default(), "1.0.0", &|_| false, at).is_none());
}

pub(super) fn fresh(version: &str, at: &str) -> UpdateStatus {
    UpdateStatus {
        latest: Some(Latest {
            version: version.into(),
            url: "https://r".into(),
            notes: String::new(),
            published_at: None,
        }),
        update_available: true,
        error: None,
        checked_at: Some(at.into()),
    }
}

#[test]
pub(super) fn an_automatic_update_waits_for_busy_threads_then_installs() {
    let now = 1_700_000_000_000u64;
    let at = iso8601(now / 1000);
    let check = fresh("1.1.0", &at);
    let mut state = AutoState {
        auto: true,
        ..Default::default()
    };
    let (step, _) = consider(&mut state, &check, "1.0.0", 2, now, &at);
    assert_eq!(step, Step::Wait);
    assert_eq!(state.waiting.as_ref().unwrap().since, now);
    let later = now + 60_000;
    let (step, _) = consider(&mut state, &check, "1.0.0", 0, later, &at);
    assert_eq!(
        step,
        Step::Install {
            version: "1.1.0".into()
        }
    );
    assert_eq!(state.pending.as_ref().unwrap().to, "1.1.0");
    assert!(state.waiting.is_none());
}

#[test]
pub(super) fn a_shell_snapshot_counts_threads_a_restart_would_interrupt() {
    let snapshot = json!({
        "threads": [
            { "status": "idle" },
            { "status": "running" },
            { "status": "completed", "activityRunStatus": "waiting" },
            { "status": "idle", "pendingRuntimeRequest": { "id": "r" } },
            { "status": "idle", "pendingBackgroundTasks": ["t"] },
            { "status": "failed" }
        ],
        "archivedThreads": [
            { "status": "queued" },
            { "status": "idle" }
        ]
    });
    assert_eq!(busy_in_snapshot(&snapshot), 5);
}

/// Live T3. Not part of the default run: it needs `~/.hbots/t3-token` and T3 on :3773.
#[tokio::test]
#[ignore]
pub(super) async fn probe_shell() {
    let token = std::fs::read_to_string(format!(
        "{}/.hbots/t3-token",
        std::env::var("HOME").unwrap_or_default()
    ))
    .expect("t3 token")
    .trim()
    .to_string();
    assert!(!token.is_empty());
    let t3 = crate::harness::t3::T3::new("http://127.0.0.1:3773".into(), token);
    let busy = read_busy(&t3).await.expect("shell snapshot");
    let tasks = crate::chat::routines::list(&t3).await.expect("routines");
    let n = tasks
        .get("tasks")
        .and_then(Value::as_array)
        .map(Vec::len)
        .expect("tasks array");
    eprintln!("busy={busy} tasks={n}");
}

#[test]
pub(super) fn an_automatic_update_gives_up_after_a_day_and_skips_failed_releases() {
    let now = 1_700_000_000_000u64;
    let at = iso8601(now / 1000);
    let check = fresh("1.1.0", &at);
    let mut state = AutoState {
        auto: true,
        waiting: Some(Waiting {
            version: "1.1.0".into(),
            since: now - MAX_WAIT_MS,
        }),
        ..Default::default()
    };
    let (step, _) = consider(&mut state, &check, "1.0.0", 1, now, &at);
    assert_eq!(step, Step::GaveUp);
    assert_eq!(state.gave_up.as_deref(), Some("1.1.0"));
    let (step, _) = consider(&mut state, &check, "1.0.0", 0, now, &at);
    assert_eq!(step, Step::Nothing);

    let mut state = AutoState {
        auto: true,
        skip: vec!["1.1.0".into()],
        ..Default::default()
    };
    assert_eq!(
        consider(&mut state, &check, "1.0.0", 0, now, &at).0,
        Step::Nothing
    );
}

#[test]
pub(super) fn nothing_happens_when_auto_is_off_or_the_check_failed_or_an_update_runs() {
    let now = 1_700_000_000_000u64;
    let at = iso8601(now / 1000);
    let check = fresh("1.1.0", &at);
    let mut off = AutoState::default();
    assert_eq!(
        consider(&mut off, &check, "1.0.0", 0, now, &at).0,
        Step::Nothing
    );
    let mut on = AutoState {
        auto: true,
        ..Default::default()
    };
    let failed = UpdateStatus {
        error: Some("offline".into()),
        ..check.clone()
    };
    assert_eq!(
        consider(&mut on, &failed, "1.0.0", 0, now, &at).0,
        Step::Nothing
    );
    let mut running = AutoState {
        auto: true,
        pending: Some(Pending {
            from: "1.0.0".into(),
            to: "1.1.0".into(),
            url: "u".into(),
            at: at.clone(),
        }),
        ..Default::default()
    };
    assert_eq!(
        consider(&mut running, &check, "1.0.0", 0, now, &at).0,
        Step::Nothing
    );
}

#[test]
pub(super) fn a_stuck_update_is_failed_after_half_an_hour() {
    let now = 1_700_000_000_000u64;
    let started = iso8601((now - PENDING_MS - 1000) / 1000);
    let at = iso8601(now / 1000);
    let mut state = AutoState {
        auto: true,
        pending: Some(Pending {
            from: "1.0.0".into(),
            to: "1.1.0".into(),
            url: "u".into(),
            at: started,
        }),
        ..Default::default()
    };
    let check = fresh("1.1.0", &at);
    let (step, lines) = consider(&mut state, &check, "1.0.0", 0, now, &at);
    assert_eq!(step, Step::Nothing);
    assert!(lines.iter().any(|l| l.contains("didn't finish")));
    assert_eq!(state.notice.as_ref().unwrap().kind, "failed");
    assert!(state.skip.contains(&"1.1.0".to_string()));
}

#[test]
pub(super) fn the_state_file_round_trips_in_camel_case() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("auto-update.json");
    assert_eq!(read_auto_state(&file), AutoState::default());
    let state = AutoState {
        auto: true,
        gave_up: Some("2.0.0".into()),
        ..Default::default()
    };
    write_auto_state(&file, &state).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"gaveUp\""), "{text}");
    assert_eq!(read_auto_state(&file), state);
}
