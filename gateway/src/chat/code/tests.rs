use super::cache::{evaluate, Gate, SignalCache, SIGNAL_TTL};
use super::signal::{
    mode_of, require_mode, signal, ActivityFacts, CodeMode, SignalInput, VcsStatus,
};
use crate::bots::Bots;
use crate::harness::adapters::mock::MockHarness;
use std::sync::Arc;
use std::time::Instant;

fn facts(file_change: bool, git_command: bool) -> ActivityFacts {
    ActivityFacts {
        file_change_in_repo: file_change,
        git_command_in_repo: git_command,
    }
}

fn decide(mode: CodeMode, is_repo: Option<bool>, activity: Option<ActivityFacts>) -> bool {
    let status = is_repo.map(|is_repo| VcsStatus { is_repo });
    signal(&SignalInput {
        mode,
        vcs: status.as_ref(),
        activity: activity.as_ref(),
    })
    .show
}

#[test]
fn auto_follows_the_repository_and_on_and_off_override_it() {
    assert!(decide(CodeMode::Auto, Some(true), None));
    assert!(!decide(CodeMode::Auto, Some(false), None));
    assert!(!decide(CodeMode::Auto, None, None));
    assert!(decide(CodeMode::On, Some(false), None));
    assert!(!decide(CodeMode::Off, Some(true), None));
    assert!(!decide(CodeMode::Off, Some(true), Some(facts(true, true))));
}

#[test]
fn activity_counts_only_when_the_caller_supplies_facts() {
    let change = facts(true, false);
    assert!(decide(CodeMode::Auto, Some(false), Some(change)));
    assert!(!decide(
        CodeMode::Auto,
        Some(false),
        Some(facts(false, false))
    ));
    assert!(decide(CodeMode::Auto, None, Some(facts(false, true))));
}

#[test]
fn a_missing_code_mode_is_auto_and_a_write_must_name_one() {
    assert_eq!(mode_of(&serde_json::json!({})), CodeMode::Auto);
    assert_eq!(mode_of(&serde_json::json!({ "code": "on" })), CodeMode::On);
    assert_eq!(
        mode_of(&serde_json::json!({ "code": "sideways" })),
        CodeMode::Auto
    );
    assert!(require_mode(&serde_json::json!("off")).is_ok());
    assert_eq!(
        require_mode(&serde_json::json!("ask")),
        Err("code must be auto, on, or off")
    );
    assert_eq!(
        require_mode(&serde_json::json!(true)),
        Err("code must be auto, on, or off")
    );
}

fn harness(is_repo: bool) -> Arc<MockHarness> {
    let mock = Arc::new(MockHarness::new());
    mock.set_is_repo(is_repo);
    mock
}

async fn ask(
    mock: &MockHarness,
    cache: &SignalCache,
    now: Instant,
    master_on: bool,
    mode: CodeMode,
) -> Gate {
    evaluate(mock, cache, now, "ada", "/bots/ada", master_on, mode).await
}

#[tokio::test]
async fn the_switch_off_makes_no_harness_call() {
    let mock = harness(true);
    let cache = SignalCache::default();
    let gate = ask(&mock, &cache, Instant::now(), false, CodeMode::Auto).await;
    assert_eq!(
        gate,
        Gate {
            enabled: false,
            show: false
        }
    );
    assert_eq!(mock.repo_calls(), 0);
}

#[tokio::test]
async fn auto_asks_once_per_repository_answer() {
    let now = Instant::now();
    let cache = SignalCache::default();

    let with_repo = harness(true);
    let shown = ask(&with_repo, &cache, now, true, CodeMode::Auto).await;
    assert!(shown.enabled && shown.show);
    assert_eq!(with_repo.repo_calls(), 1);
    assert_eq!(with_repo.repo_cwd(), "/bots/ada");

    let without = harness(false);
    let hidden = ask(&without, &SignalCache::default(), now, true, CodeMode::Auto).await;
    assert!(hidden.enabled && !hidden.show);
    assert_eq!(without.repo_calls(), 1);
}

#[tokio::test]
async fn on_and_off_do_not_ask_the_harness() {
    let now = Instant::now();
    let cache = SignalCache::default();

    let forced = harness(false);
    let on = ask(&forced, &cache, now, true, CodeMode::On).await;
    assert!(on.enabled && on.show);
    assert_eq!(forced.repo_calls(), 0);

    let blocked = harness(true);
    let off = ask(&blocked, &cache, now, true, CodeMode::Off).await;
    assert!(off.enabled && !off.show);
    assert_eq!(blocked.repo_calls(), 0);
}

#[tokio::test]
async fn a_cached_answer_is_reused_until_the_window_ends() {
    let mock = harness(true);
    let cache = SignalCache::default();
    let start = Instant::now();
    ask(&mock, &cache, start, true, CodeMode::Auto).await;
    let inside = start + (SIGNAL_TTL - std::time::Duration::from_secs(1));
    ask(&mock, &cache, inside, true, CodeMode::Auto).await;
    assert_eq!(mock.repo_calls(), 1);

    mock.set_is_repo(false);
    let expired = start + SIGNAL_TTL;
    let refreshed = ask(&mock, &cache, expired, true, CodeMode::Auto).await;
    assert!(!refreshed.show);
    assert_eq!(mock.repo_calls(), 2);
    ask(&mock, &cache, expired, true, CodeMode::Auto).await;
    assert_eq!(mock.repo_calls(), 2);
}

#[tokio::test]
async fn code_mode_is_stored_only_when_a_person_sets_it() {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    let ada = bots
        .create(&serde_json::json!({ "name": "Ada" }))
        .await
        .unwrap();
    assert!(ada.get("code").is_none());
    assert_eq!(mode_of(&ada), CodeMode::Auto);

    let saved = bots
        .save("ada", &serde_json::json!({ "code": "on" }), false)
        .await
        .unwrap();
    assert_eq!(saved["code"], "on");

    let err = bots
        .save("ada", &serde_json::json!({ "code": "ask" }), false)
        .await
        .unwrap_err();
    assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(err.message, "code must be auto, on, or off");
    assert_eq!(bots.get("ada").await.unwrap().unwrap()["code"], "on");

    let created = bots
        .create(&serde_json::json!({ "name": "Bea", "code": "off" }))
        .await
        .unwrap();
    assert_eq!(created["code"], "off");
    let refused = bots
        .create(&serde_json::json!({ "name": "Cy", "code": "yes" }))
        .await
        .unwrap_err();
    assert_eq!(refused.status, axum::http::StatusCode::BAD_REQUEST);
    assert!(bots.get("cy").await.unwrap().is_none());
}
