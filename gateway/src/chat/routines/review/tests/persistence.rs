use super::super::store::{ReviewFile, STORE_VERSION};
use super::super::*;
use super::{hold, sealed, LATER};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn a_missing_store_loads_empty_and_a_save_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let botapp = dir.path().join(".botapp");
    let file = ReviewFile::new(&botapp);
    assert_eq!(file.load().await.unwrap(), ReviewStore::empty());
    assert!(!botapp.join("routine-review.json").exists());

    let mut store = sealed(&["old"]);
    record(&mut store, "t", "ada", &hold(), LATER);
    note_run(&mut store, "t", RunStatus::Failed);
    note_run(&mut store, "t", RunStatus::Failed);
    file.save(&store).await.unwrap();

    let path = botapp.join("routine-review.json");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"flaggedAt\""));
    assert!(text.contains("\"decidedAt\""));
    assert!(text.contains("\"grandfathered\""));
    assert!(!text.contains("secret prompt text"));
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let again = file.load().await.unwrap();
    assert_eq!(again, store);
    assert_eq!(again.version, STORE_VERSION);
    assert_eq!(again.items["t"].state, ReviewState::Pending);
    assert_eq!(again.items["t"].slug, "ada");
    assert_eq!(again.items["t"].reason, "agent-created");
    assert_eq!(again.items["t"].decided_at, None);
    assert_eq!(again.failure_count("t"), 2);
    assert!(again.grandfathered.contains("old"));
}

#[tokio::test]
async fn a_damaged_store_and_a_newer_one_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let botapp = dir.path().join(".botapp");
    std::fs::create_dir_all(&botapp).unwrap();
    let path = botapp.join("routine-review.json");
    let file = ReviewFile::new(&botapp);

    std::fs::write(&path, "{").unwrap();
    assert_eq!(
        file.load().await.unwrap_err().message,
        "routine-review.json is not valid"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{");

    std::fs::write(&path, r#"{"version":2,"items":{}}"#).unwrap();
    assert_eq!(
        file.load().await.unwrap_err().message,
        "routine-review.json is from a newer hbots"
    );
    std::fs::write(&path, r#"{"version":0}"#).unwrap();
    assert_eq!(
        file.load().await.unwrap_err().message,
        "routine-review.json is not valid"
    );
    std::fs::write(&path, "[]").unwrap();
    assert_eq!(
        file.load().await.unwrap_err().message,
        "routine-review.json is not valid"
    );
    std::fs::write(&path, "{}").unwrap();
    assert_eq!(file.load().await.unwrap(), ReviewStore::empty());
}

#[test]
fn the_default_mode_is_ask_and_the_limits_match_the_decisions() {
    assert_eq!(ReviewMode::DEFAULT, ReviewMode::Ask);
    assert_eq!(ReviewMode::DEFAULT.as_str(), "ask");
    assert_eq!(ReviewMode::Trust.as_str(), "trust");
    assert_eq!(ReviewMode::parse(Some("ask")), Some(ReviewMode::Ask));
    assert_eq!(ReviewMode::parse(Some("trust")), Some(ReviewMode::Trust));
    assert_eq!(ReviewMode::parse(Some("nope")), None);
    assert_eq!(ReviewMode::parse(None), None);
    let limits = Limits::default();
    assert_eq!(limits.interval_floor_ms, INTERVAL_FLOOR_MS);
    assert_eq!(INTERVAL_FLOOR_MS, 15 * 60 * 1000);
    assert_eq!(limits.max_routines, DEFAULT_MAX_ROUTINES);
    assert_eq!(limits.max_pending, DEFAULT_MAX_PENDING);
    assert_eq!(limits.failure_limit, FAILURE_LIMIT);
    assert_eq!(run_status(Some("failed")), RunStatus::Failed);
    assert_eq!(run_status(Some("succeeded")), RunStatus::Succeeded);
    assert_eq!(run_status(Some("running")), RunStatus::Running);
    assert_eq!(run_status(Some("never")), RunStatus::Never);
    assert_eq!(run_status(Some("other")), RunStatus::Other);
    assert_eq!(run_status(None), RunStatus::Other);
    assert!(!failure_breaker(2, FAILURE_LIMIT));
    assert!(failure_breaker(FAILURE_LIMIT, FAILURE_LIMIT));
    assert!(failure_breaker(4, FAILURE_LIMIT));
    assert!(!failure_breaker(1, 0));
}
