use super::tests::{settings_state, settings_state_with};
use super::*;
use crate::harness::adapters::mock::MockHarness;
use crate::harness::{LinkedChange, PullRow, RepoFile, RepoStatus};

fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    headers
}

async fn put(state: &Arc<AppState>, body: Value) -> Result<Value, ApiError> {
    put_settings(
        State(state.clone()),
        headers(),
        Bytes::from(body.to_string()),
    )
    .await
    .map(|Json(value)| value)
}

fn state_with(dir: &std::path::Path, is_repo: bool) -> (Arc<AppState>, Arc<MockHarness>) {
    let mock = Arc::new(MockHarness::new());
    mock.set_is_repo(is_repo);
    let state = settings_state_with(
        dir,
        Some(Arc::clone(&mock) as Arc<dyn crate::harness::Harness>),
    );
    (state, mock)
}

async fn ada(state: &AppState) {
    state.bots.create(&json!({ "name": "Ada" })).await.unwrap();
}

async fn ask(state: Arc<AppState>, slug: &str, open: bool) -> ApiResult<Json<Value>> {
    let mut query = HashMap::new();
    if open {
        query.insert("open".into(), "1".into());
    }
    code_get(State(state), Path(slug.into()), Query(query)).await
}

#[tokio::test]
async fn beta_code_defaults_off_and_stays_a_boolean() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    assert_eq!(state.settings().await["beta"]["code"], false);
    assert_eq!(state.settings().await["beta"]["projects"], false);

    let unknown = put(&state, json!({ "beta": { "code": true, "nope": true } }))
        .await
        .unwrap_err();
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    assert_eq!(unknown.message, "Unknown beta feature: nope");

    let bad = put(&state, json!({ "beta": { "code": "yes" } }))
        .await
        .unwrap_err();
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.message, "beta.code must be true or false");
    assert_eq!(state.settings().await["beta"]["code"], false);

    let next = put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();
    assert_eq!(next["beta"]["code"], true);
    assert_eq!(next["beta"]["projects"], false);
    let kept = put(&state, json!({ "userName": "Ada" })).await.unwrap();
    assert_eq!(kept["userName"], "Ada");
    assert_eq!(kept["beta"]["code"], true);
    assert_eq!(state.settings().await["beta"]["code"], true);
}

#[tokio::test]
async fn the_route_is_disabled_and_makes_no_harness_call() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), true);
    ada(&state).await;
    let Json(body) = ask(state, "ada", true).await.unwrap();
    assert_eq!(body, json!({ "enabled": false }));
    assert_eq!(mock.repo_calls(), 0);
    assert_eq!(mock.status_calls(), 0);
    assert_eq!(mock.pr_calls(), 0);
}

#[tokio::test]
async fn the_route_reports_auto_on_and_off_when_the_switch_is_on() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), true);
    ada(&state).await;
    put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();

    let Json(auto) = ask(state.clone(), "ada", false).await.unwrap();
    assert_eq!(auto, json!({ "enabled": true, "signal": true }));
    assert_eq!(mock.repo_calls(), 1);
    let Json(cached) = ask(state.clone(), "ada", false).await.unwrap();
    assert_eq!(cached["signal"], true);
    assert_eq!(mock.repo_calls(), 1);
    assert_eq!(mock.status_calls(), 0);
    assert_eq!(mock.pr_calls(), 0);

    state
        .bots
        .save("ada", &json!({ "code": "off" }), false)
        .await
        .unwrap();
    let Json(off) = ask(state.clone(), "ada", false).await.unwrap();
    assert_eq!(off, json!({ "enabled": true, "signal": false }));
    assert_eq!(mock.repo_calls(), 1);

    state
        .bots
        .save("ada", &json!({ "code": "on" }), false)
        .await
        .unwrap();
    mock.set_is_repo(false);
    let Json(on) = ask(state, "ada", false).await.unwrap();
    assert_eq!(on, json!({ "enabled": true, "signal": true }));
    assert_eq!(mock.repo_calls(), 1);
}

#[tokio::test]
async fn auto_without_a_repository_stays_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), false);
    ada(&state).await;
    put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();
    let Json(body) = ask(state, "ada", false).await.unwrap();
    assert_eq!(body, json!({ "enabled": true, "signal": false }));
    assert_eq!(mock.repo_calls(), 1);
}

#[tokio::test]
async fn a_missing_bot_is_not_found_and_makes_no_harness_call() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), true);
    let err = ask(state, "ada", false).await.unwrap_err();
    assert_eq!(err.status, StatusCode::NOT_FOUND);
    assert_eq!(mock.repo_calls(), 0);
    assert_eq!(mock.status_calls(), 0);
}

#[tokio::test]
async fn opening_the_tab_on_a_non_repo_does_not_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), false);
    ada(&state).await;
    put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();
    let Json(body) = ask(state, "ada", true).await.unwrap();
    assert_eq!(body, json!({ "enabled": true, "signal": false }));
    assert_eq!(mock.repo_calls(), 1);
    assert_eq!(mock.status_calls(), 0);
    assert_eq!(mock.pr_calls(), 0);
}

#[tokio::test]
async fn an_open_tab_returns_status_and_pull_requests_once() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), true);
    ada(&state).await;
    state
        .bots
        .save("ada", &json!({ "projectId": "project-ada" }), false)
        .await
        .unwrap();
    put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();
    mock.set_status(RepoStatus {
        is_repo: true,
        provider: "GitHub".into(),
        base_url: "https://user:token@github.com".into(),
        has_remote: true,
        default_branch: false,
        branch: "feature".into(),
        ahead: 2,
        behind: 1,
        insertions: 4,
        deletions: 1,
        files: vec![RepoFile {
            path: "src/main.rs".into(),
            insertions: 4,
            deletions: 1,
        }],
        file_total: 1,
        linked: Some(LinkedChange {
            number: 12,
            title: "Fix the gate".into(),
            url: "https://user:token@github.com/org/repo/pull/12".into(),
            state: "open".into(),
            draft: false,
        }),
    });
    mock.set_prs(vec![PullRow {
        number: 12,
        title: "Fix the gate".into(),
        url: "https://user:pass@github.com/org/repo/pull/12".into(),
        draft: true,
        checks: "failing".into(),
        review: "approved".into(),
        updated: "2026-01-02T00:00:00.000Z".into(),
    }]);

    let Json(first) = ask(state.clone(), "ada", true).await.unwrap();
    assert_eq!(first["signal"], true);
    assert_eq!(first["error"], "");
    assert_eq!(first["repo"]["branch"], "feature");
    assert_eq!(first["repo"]["ahead"], 2);
    assert_eq!(first["repo"]["defaultBranch"], false);
    assert_eq!(first["repo"]["baseUrl"], "https://github.com");
    assert_eq!(first["repo"]["files"][0]["path"], "src/main.rs");
    assert_eq!(
        first["repo"]["linked"]["url"],
        "https://github.com/org/repo/pull/12"
    );
    assert_eq!(first["prs"][0]["checks"], "failing");
    assert_eq!(first["prs"][0]["review"], "approved");
    assert_eq!(first["prs"][0]["draft"], true);
    assert_eq!(
        first["prs"][0]["url"],
        "https://github.com/org/repo/pull/12"
    );
    assert!(!first.to_string().contains("user:"));
    assert!(mock.last_remote());
    assert_eq!(mock.status_calls(), 1);
    assert_eq!(mock.pr_calls(), 1);

    let Json(second) = ask(state, "ada", true).await.unwrap();
    assert_eq!(second["repo"]["branch"], "feature");
    assert_eq!(mock.status_calls(), 1);
    assert_eq!(mock.pr_calls(), 1);
}

#[tokio::test]
async fn an_open_tab_maps_provider_failures() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), true);
    ada(&state).await;
    state
        .bots
        .save(
            "ada",
            &json!({ "code": "on", "projectId": "project-ada" }),
            false,
        )
        .await
        .unwrap();
    put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();

    mock.set_status_fault("signed-out");
    let Json(signed_out) = ask(state.clone(), "ada", true).await.unwrap();
    assert_eq!(signed_out["error"], "signed-out");
    assert!(signed_out["repo"].is_null());
    assert_eq!(mock.pr_calls(), 0);

    mock.set_status_fault("rate-limited");
    let Json(limited) = ask(state.clone(), "ada", true).await.unwrap();
    assert_eq!(limited["error"], "rate-limited");

    mock.set_status_fault("offline");
    let Json(offline) = ask(state.clone(), "ada", true).await.unwrap();
    assert_eq!(offline["error"], "offline");
}

#[tokio::test]
async fn a_repository_without_a_remote_still_lists_local_files() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), true);
    ada(&state).await;
    put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();
    let mut status = RepoStatus::not_a_repo();
    status.is_repo = true;
    status.branch = "main".into();
    status.default_branch = true;
    status.files = vec![RepoFile {
        path: "notes.md".into(),
        insertions: 1,
        deletions: 0,
    }];
    status.file_total = 1;
    mock.set_status(status);
    let Json(body) = ask(state, "ada", true).await.unwrap();
    assert_eq!(body["error"], "no-remote");
    assert_eq!(body["repo"]["hasRemote"], false);
    assert_eq!(body["repo"]["files"][0]["path"], "notes.md");
    assert_eq!(mock.pr_calls(), 0);
}

#[tokio::test]
async fn forcing_code_on_without_a_repository_is_an_empty_read() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), false);
    ada(&state).await;
    state
        .bots
        .save("ada", &json!({ "code": "on" }), false)
        .await
        .unwrap();
    put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();
    let Json(body) = ask(state, "ada", true).await.unwrap();
    assert_eq!(body["enabled"], true);
    assert_eq!(body["signal"], true);
    assert_eq!(body["error"], "");
    assert!(body["repo"].is_null());
    assert_eq!(body["prs"], json!([]));
    assert_eq!(mock.repo_calls(), 0);
    assert_eq!(mock.status_calls(), 1);
    assert!(mock.last_remote());
    assert_eq!(mock.pr_calls(), 0);
}

#[tokio::test]
async fn a_pull_request_failure_keeps_the_branch() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), true);
    ada(&state).await;
    state
        .bots
        .save("ada", &json!({ "projectId": "project-ada" }), false)
        .await
        .unwrap();
    put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();
    mock.set_pr_fault("signed-out");
    let Json(body) = ask(state, "ada", true).await.unwrap();
    assert_eq!(body["error"], "signed-out");
    assert_eq!(body["repo"]["branch"], "main");
    assert_eq!(body["prs"], json!([]));
    assert_eq!(mock.pr_calls(), 1);
}
