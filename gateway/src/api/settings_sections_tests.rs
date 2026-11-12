use super::super::super::tests::settings_state;
use super::super::put_settings;
use crate::platform::error::ApiError;
use crate::storage;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;

fn row(id: &str, name: &str, collapsed: bool, sort: &str, items: &[&str]) -> Value {
    json!({
        "id": id,
        "name": name,
        "collapsed": collapsed,
        "sort": sort,
        "items": items,
    })
}

fn listed(section: Value) -> Value {
    json!([section])
}

#[track_caller]
fn accept(value: &Value) {
    super::check_sections(value).unwrap();
    let saved = json!({ "userName": "Ada", "sections": value });
    assert_eq!(
        serde_json::to_vec(&super::sections_view(&saved)).unwrap(),
        serde_json::to_vec(value).unwrap()
    );
}

#[track_caller]
fn reject(value: &Value, message: &str) {
    let err = super::check_sections(value).expect_err("sections should be rejected");
    assert!(
        err.contains(message),
        "expected {message} in {err} for {value}"
    );
}

fn items(id: &str, name: &str, sort: &str, count: usize) -> Value {
    json!({
        "id": id,
        "name": name,
        "collapsed": false,
        "sort": sort,
        "items": (0..count).map(|i| format!("bot:b{i}")).collect::<Vec<_>>(),
    })
}

#[test]
fn missing_or_invalid_sections_view_is_an_empty_array() {
    assert_eq!(
        super::sections_view(&json!({ "userName": "Ada" })),
        json!([])
    );
    assert_eq!(
        super::sections_view(&json!({ "userName": "Ada", "sections": "nope" })),
        json!([])
    );
}

#[test]
fn settings_sections_accepts_a_sidebar_document() {
    accept(&json!([]));
    accept(&json!([
        row(
            "swork001",
            "Café",
            true,
            "manual",
            &["bot:ada-2", "group:550e8400-e29b-41d4-a716-446655440000",]
        ),
        row("chats", "", false, "manual", &["bot:ada"]),
        row(
            "shome002",
            "Café",
            false,
            "manual",
            &["group:123e4567-e89b-12d3-a456-426614174000",]
        ),
        row("hidden", "", true, "recent", &[]),
    ]));
    accept(&listed(row("chats", "", false, "recent", &[])));
    accept(&listed(row(
        "s1234567",
        &"a".repeat(40),
        false,
        "manual",
        &[],
    )));
    accept(&listed(row(
        "s1234567",
        &"é".repeat(40),
        false,
        "manual",
        &[],
    )));
}

#[test]
fn settings_sections_rejects_a_bad_document() {
    reject(&json!(null), "must be an array");
    reject(&json!({}), "must be an array");
    reject(&json!("[]"), "must be an array");
    reject(&json!([null]), "must be an object");
    reject(
        &listed(row("nope", "", false, "recent", &[])),
        "id is invalid",
    );
    reject(
        &listed(row("s123456", "Work", false, "manual", &[])),
        "id is invalid",
    );
    reject(
        &listed(row("s12345678", "Work", false, "manual", &[])),
        "id is invalid",
    );
    reject(
        &listed(row("sABCDEFG", "Work", false, "manual", &[])),
        "id is invalid",
    );
    reject(
        &json!([
            row("chats", "", false, "recent", &[]),
            row("chats", "", true, "manual", &[]),
        ]),
        "already used",
    );
    reject(
        &listed(row("chats", "Chats", false, "recent", &[])),
        "name is invalid",
    );
    reject(
        &listed(row("hidden", "Hidden", true, "recent", &[])),
        "name is invalid",
    );
    reject(
        &listed(row("swork001", "", false, "manual", &[])),
        "name is invalid",
    );
    reject(
        &listed(row("swork001", " Work", false, "manual", &[])),
        "name is invalid",
    );
    reject(
        &listed(row("swork001", "Work ", false, "manual", &[])),
        "name is invalid",
    );
    reject(
        &listed(row("swork001", &"a".repeat(41), false, "manual", &[])),
        "name is invalid",
    );
    reject(
        &listed(row("swork001", "a\nb", false, "manual", &[])),
        "name is invalid",
    );
    reject(
        &listed(row("swork001", "a\u{001c}b", false, "manual", &[])),
        "name is invalid",
    );
    reject(
        &listed(row("swork001", "a\u{001f}b", false, "manual", &[])),
        "name is invalid",
    );
    reject(
        &listed(row("swork001", "a\u{007f}b", false, "manual", &[])),
        "name is invalid",
    );
    reject(
        &json!([{
            "id": "chats",
            "name": "",
            "collapsed": "false",
            "sort": "recent",
            "items": [],
        }]),
        "collapsed must be true or false",
    );
    reject(
        &listed(row("swork001", "Work", false, "recent", &[])),
        "sort must be manual",
    );
    reject(
        &listed(row("chats", "", false, "newest", &[])),
        "sort must be recent or manual",
    );
    reject(
        &listed(row("chats", "", false, "recent", &["bot:../x"])),
        "is invalid",
    );
    reject(
        &listed(row("chats", "", false, "recent", &["bot:-ada"])),
        "is invalid",
    );
    reject(
        &listed(row("chats", "", false, "recent", &["bot:Ada"])),
        "is invalid",
    );
    reject(
        &listed(row("chats", "", false, "recent", &["Bot:ada"])),
        "is invalid",
    );
    reject(
        &listed(row("chats", "", false, "recent", &["group:not-a-uuid"])),
        "is invalid",
    );
    reject(
        &listed(row(
            "chats",
            "",
            false,
            "recent",
            &["group:550E8400-E29B-41D4-A716-446655440000"],
        )),
        "is invalid",
    );
    reject(
        &listed(row("hidden", "", false, "recent", &["bot:ada"])),
        "items must be empty",
    );
    reject(
        &json!([{
            "id": "chats",
            "name": "",
            "collapsed": false,
            "sort": "recent",
            "items": [],
            "extra": true,
        }]),
        "unknown field",
    );
    reject(
        &json!([{
            "id": "chats",
            "name": "",
            "collapsed": false,
            "sort": "recent",
        }]),
        "items must be an array",
    );
}

#[test]
fn settings_sections_enforces_count_limits() {
    assert!(super::check_sections(&json!([items("chats", "", "recent", 200)])).is_ok());
    let too_long = super::check_sections(&json!([items("chats", "", "recent", 201)])).unwrap_err();
    assert!(too_long.contains("too long"), "{too_long}");
    let at_cap = json!([
        items("chats", "", "recent", 200),
        items("s0000001", "A", "manual", 200),
        items("s0000002", "B", "manual", 100),
    ]);
    assert!(super::check_sections(&at_cap).is_ok());
    let over = json!([
        items("chats", "", "recent", 200),
        items("s0000001", "A", "manual", 200),
        items("s0000002", "B", "manual", 101),
    ]);
    let err = super::check_sections(&over).unwrap_err();
    assert!(err.contains("too many items"), "{err}");

    let mut allowed = Vec::new();
    for i in 0..12 {
        allowed.push(items(&format!("s{i:07}"), "A", "manual", 0));
    }
    allowed.push(items("chats", "", "recent", 0));
    allowed.push(items("hidden", "", "recent", 0));
    assert!(super::check_sections(&Value::Array(allowed)).is_ok());

    let mut too_many = Vec::new();
    for i in 0..13 {
        too_many.push(items(&format!("s{i:07}"), "A", "manual", 0));
    }
    let err = super::check_sections(&Value::Array(too_many)).unwrap_err();
    assert!(err.contains("too many custom sections"), "{err}");
}

fn sample() -> Value {
    json!([
        {
            "id": "swork001",
            "name": "Work \"notes\"",
            "collapsed": true,
            "sort": "manual",
            "items": ["bot:ada-2", "group:550e8400-e29b-41d4-a716-446655440000"],
        },
        {
            "id": "chats",
            "name": "",
            "collapsed": false,
            "sort": "manual",
            "items": ["bot:ada"],
        },
        {
            "id": "shome002",
            "name": "Work \"notes\"",
            "collapsed": false,
            "sort": "manual",
            "items": ["group:123e4567-e89b-12d3-a456-426614174000"],
        },
        {
            "id": "hidden",
            "name": "",
            "collapsed": true,
            "sort": "recent",
            "items": [],
        },
    ])
}

async fn put(state: &Arc<crate::api::AppState>, body: Value) -> Result<Value, ApiError> {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    put_settings(State(state.clone()), headers, Bytes::from(body.to_string()))
        .await
        .map(|Json(value)| value)
}

#[tokio::test]
async fn missing_sections_key_stays_empty_and_keeps_user_name() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    storage::write_json(
        &state.settings_path(),
        &json!({ "userName": "Ada", "theme": "light" }),
    )
    .await
    .unwrap();
    let viewed = state.settings().await;
    assert_eq!(viewed["sections"], json!([]));
    assert_eq!(viewed["userName"], "Ada");
    assert_eq!(viewed["theme"], "light");
    let on_disk = storage::read_json(&state.settings_path()).await.unwrap();
    assert!(on_disk.get("sections").is_none());
    assert_eq!(on_disk["userName"], "Ada");
}

#[tokio::test]
async fn invalid_sections_become_empty_and_keep_user_name() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    storage::write_json(
        &state.settings_path(),
        &json!({ "userName": "Ada", "theme": "light", "sections": { "bad": true } }),
    )
    .await
    .unwrap();
    let viewed = state.settings().await;
    assert_eq!(viewed["userName"], "Ada");
    assert_eq!(viewed["theme"], "light");
    assert_eq!(viewed["sections"], json!([]));
    let next = put(&state, json!({ "userName": "Bea" })).await.unwrap();
    assert_eq!(next["userName"], "Bea");
    assert_eq!(next["theme"], "light");
    assert_eq!(next["sections"], json!([]));
    let on_disk = storage::read_json(&state.settings_path()).await.unwrap();
    assert_eq!(on_disk["userName"], "Bea");
    assert_eq!(on_disk["theme"], "light");
    assert_eq!(on_disk["sections"], json!([]));
}

#[tokio::test]
async fn bad_sections_put_is_400_and_the_file_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    storage::write_json(
        &state.settings_path(),
        &json!({ "userName": "Ada", "sections": sample() }),
    )
    .await
    .unwrap();
    let before = tokio::fs::read(state.settings_path()).await.unwrap();
    let err = put(
        &state,
        json!({ "userName": "Bea", "sections": [{ "id": "nope" }] }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("sections"), "{}", err.message);
    let after = tokio::fs::read(state.settings_path()).await.unwrap();
    assert_eq!(before, after);

    let fresh = tempfile::tempdir().unwrap();
    let fresh_state = settings_state(fresh.path());
    let err = put(&fresh_state, json!({ "sections": {} }))
        .await
        .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(!fresh_state.settings_path().exists());
}

#[tokio::test]
async fn user_name_save_leaves_valid_sections_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    let sections = sample();
    let before = serde_json::to_vec(&sections).unwrap();
    storage::write_json(
        &state.settings_path(),
        &json!({ "userName": "Ada", "theme": "light", "sections": sections }),
    )
    .await
    .unwrap();
    let viewed = state.settings().await;
    assert_eq!(viewed["userName"], "Ada");
    assert_eq!(serde_json::to_vec(&viewed["sections"]).unwrap(), before);
    let next = put(&state, json!({ "userName": "Bea" })).await.unwrap();
    assert_eq!(next["userName"], "Bea");
    assert_eq!(next["theme"], "light");
    assert_eq!(serde_json::to_vec(&next["sections"]).unwrap(), before);
    let on_disk = storage::read_json(&state.settings_path()).await.unwrap();
    assert_eq!(on_disk["userName"], "Bea");
    assert_eq!(on_disk["theme"], "light");
    assert_eq!(serde_json::to_vec(&on_disk["sections"]).unwrap(), before);
}

#[tokio::test]
async fn empty_sections_array_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    let next = put(&state, json!({ "userName": "Ada", "sections": [] }))
        .await
        .unwrap();
    assert_eq!(next["userName"], "Ada");
    assert_eq!(next["sections"], json!([]));
    let on_disk = storage::read_json(&state.settings_path()).await.unwrap();
    assert_eq!(on_disk["sections"], json!([]));
    assert_eq!(state.settings().await["sections"], json!([]));
}

#[tokio::test]
async fn valid_sections_put_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    let sections = sample();
    let before = serde_json::to_vec(&sections).unwrap();
    let next = put(&state, json!({ "userName": "Ada", "sections": sections }))
        .await
        .unwrap();
    assert_eq!(next["userName"], "Ada");
    assert_eq!(serde_json::to_vec(&next["sections"]).unwrap(), before);
    let on_disk = storage::read_json(&state.settings_path()).await.unwrap();
    assert_eq!(serde_json::to_vec(&on_disk["sections"]).unwrap(), before);
}
