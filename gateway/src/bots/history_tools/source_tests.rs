use super::cache::ProjectionCache;
use super::source::{find, Found};
use super::window::Role;
use crate::harness::adapters::mock::{MockHarness, SearchScript};
use crate::harness::ThreadHit;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

fn allow(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_string()).collect()
}

fn stamps(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(thread, stamp)| ((*thread).to_string(), (*stamp).to_string()))
        .collect()
}

fn user(id: &str, text: &str, at: &str) -> Value {
    json!({
        "id": id,
        "type": "user_message",
        "text": text,
        "status": "completed",
        "startedAt": at,
    })
}

fn projection(items: Vec<Value>, partial: bool) -> Value {
    json!({ "turnItems": items, "partial": partial })
}

fn hit(thread: &str, message: Option<&str>, snippet: &str) -> ThreadHit {
    ThreadHit {
        thread_id: thread.to_string(),
        project_id: "project".to_string(),
        source: "user".to_string(),
        snippet: snippet.to_string(),
        message_created_at: None,
        quality: None,
        hits: None,
        message_id: message.map(str::to_string),
    }
}

async fn search(
    mock: &MockHarness,
    query: &str,
    allowed: &BTreeSet<String>,
    stamps: &HashMap<String, String>,
    cache: &ProjectionCache,
) -> Found {
    find(mock, mock, cache, query, allowed, stamps)
        .await
        .expect("search")
}

fn ready(partial: bool) -> (MockHarness, BTreeSet<String>, ProjectionCache) {
    let mock = MockHarness::new();
    mock.set_projection(
        "thread-a",
        projection(
            vec![user(
                "named",
                "the lighthouse stands",
                "2026-01-02T00:00:00Z",
            )],
            partial,
        ),
    );
    (mock, allow(&["thread-a"]), ProjectionCache::new())
}

#[tokio::test]
async fn a_missing_command_scans_locally() {
    let (mock, allowed, cache) = ready(false);
    let found = search(&mock, "  lighthouse  ", &allowed, &HashMap::new(), &cache).await;
    assert_eq!(found.matches.len(), 1);
    assert_eq!(found.matches[0].message_id, "named");
    assert_eq!(found.matches[0].thread_id, "thread-a");
    assert_eq!(found.matches[0].role, Role::You);
    assert!(!found.truncated);
    assert_eq!(mock.search_calls(), vec![("lighthouse".to_string(), 50)]);
    assert!(found.matches[0].snippet.contains("lighthouse"));
}

#[tokio::test]
async fn a_search_error_scans_locally() {
    let (mock, allowed, cache) = ready(false);
    mock.set_search(SearchScript::Failed);
    let found = search(&mock, "lighthouse", &allowed, &HashMap::new(), &cache).await;
    assert_eq!(found.matches[0].message_id, "named");
    assert!(!found.truncated);
}

#[tokio::test]
async fn a_closed_socket_scans_locally() {
    let (mock, allowed, cache) = ready(false);
    mock.set_search(SearchScript::Down);
    let found = search(&mock, "lighthouse", &allowed, &HashMap::new(), &cache).await;
    assert_eq!(found.matches[0].message_id, "named");
    assert_eq!(mock.read_count("thread-a"), 1);
}

#[tokio::test]
async fn a_crowded_page_scans_locally() {
    let mock = MockHarness::new();
    mock.set_projection(
        "thread-a",
        projection(
            vec![
                user("weak", "unrelated note", "2026-01-01T00:00:00Z"),
                user("strong", "the lighthouse stands", "2026-01-02T00:00:00Z"),
            ],
            false,
        ),
    );
    mock.set_projection(
        "thread-b",
        projection(
            vec![user("secret", "secret lighthouse", "2026-01-03T00:00:00Z")],
            false,
        ),
    );
    let mut hits = Vec::new();
    for index in 0..49 {
        hits.push(hit("thread-b", Some("secret"), &format!("secret {index}")));
    }
    hits.push(hit("thread-a", Some("weak"), "unrelated note"));
    mock.set_search(SearchScript::Hits(hits));
    let found = search(
        &mock,
        "lighthouse",
        &allow(&["thread-a"]),
        &HashMap::new(),
        &ProjectionCache::new(),
    )
    .await;
    assert_eq!(found.matches.len(), 1);
    assert_eq!(found.matches[0].message_id, "strong");
    assert!(found
        .matches
        .iter()
        .all(|hit| !hit.snippet.contains("secret")));
    assert_eq!(mock.read_count("thread-b"), 0);
    assert_eq!(mock.read_count("thread-a"), 1);
}

#[tokio::test]
async fn another_bots_thread_ranked_first_is_not_read() {
    let mock = MockHarness::new();
    mock.set_projection(
        "thread-a",
        projection(
            vec![
                user("named", "the lighthouse stands", "2026-01-01T00:00:00Z"),
                user("extra", "lighthouse again", "2026-01-02T00:00:00Z"),
            ],
            false,
        ),
    );
    mock.set_projection(
        "thread-b",
        projection(
            vec![user("secret", "secret lighthouse", "2026-01-03T00:00:00Z")],
            false,
        ),
    );
    mock.set_search(SearchScript::Hits(vec![
        hit("thread-b", Some("secret"), "secret lighthouse"),
        hit("thread-a", Some("named"), "the lighthouse stands"),
    ]));
    let found = search(
        &mock,
        "lighthouse",
        &allow(&["thread-a"]),
        &HashMap::new(),
        &ProjectionCache::new(),
    )
    .await;
    assert_eq!(found.matches.len(), 1);
    assert_eq!(found.matches[0].message_id, "named");
    assert!(found
        .matches
        .iter()
        .all(|hit| hit.thread_id == "thread-a" && !hit.snippet.contains("secret")));
    assert_eq!(mock.read_count("thread-b"), 0);
    assert_eq!(mock.read_count("thread-a"), 1);
}

#[tokio::test]
async fn a_partial_projection_is_truncated() {
    let (mock, allowed, cache) = ready(true);
    let found = search(&mock, "lighthouse", &allowed, &HashMap::new(), &cache).await;
    assert!(found.truncated);
    assert_eq!(found.matches[0].message_id, "named");
}

#[tokio::test]
async fn framed_turns_are_not_returned() {
    let mock = MockHarness::new();
    mock.set_projection(
        "thread-a",
        projection(
            vec![
                user("plain", "thanks", "2026-01-01T00:00:00Z"),
                user(
                    "framed",
                    "[Group \"Duo\": Ada (@ada). You wrote:]\nfrom the group",
                    "2026-01-02T00:00:00Z",
                ),
            ],
            false,
        ),
    );
    mock.set_search(SearchScript::Hits(vec![hit(
        "thread-a",
        Some("framed"),
        "from the group",
    )]));
    let found = search(
        &mock,
        "from the group",
        &allow(&["thread-a"]),
        &HashMap::new(),
        &ProjectionCache::new(),
    )
    .await;
    assert!(found.matches.is_empty());
    assert_eq!(mock.read_count("thread-a"), 1);
}

#[tokio::test]
async fn a_whole_word_outranks_a_prefix() {
    let mock = MockHarness::new();
    mock.set_projection(
        "thread-a",
        projection(
            vec![
                user("whole", "a draft note", "2026-01-01T00:00:00Z"),
                user("prefix", "drafts of the note", "2026-01-02T00:00:00Z"),
            ],
            false,
        ),
    );
    let mut prefix = hit("thread-a", Some("prefix"), "drafts");
    prefix.quality = Some(1.0);
    let mut whole = hit("thread-a", Some("whole"), "draft");
    whole.quality = Some(0.1);
    mock.set_search(SearchScript::Hits(vec![prefix, whole]));
    let found = search(
        &mock,
        "draft",
        &allow(&["thread-a"]),
        &HashMap::new(),
        &ProjectionCache::new(),
    )
    .await;
    let ids: Vec<&str> = found
        .matches
        .iter()
        .map(|hit| hit.message_id.as_str())
        .collect();
    assert_eq!(ids, vec!["whole", "prefix"]);
    assert!(found.matches[0].quality > found.matches[1].quality);
    assert_eq!(mock.read_count("thread-a"), 1);
}

#[tokio::test]
async fn a_match_without_message_id_uses_the_snippet() {
    let mock = MockHarness::new();
    mock.set_projection(
        "thread-a",
        projection(
            vec![user(
                "named",
                "the lighthouse stands",
                "2026-01-02T00:00:00Z",
            )],
            false,
        ),
    );
    let mut hit = hit("thread-a", None, "…lighthouse stands");
    hit.message_created_at = Some("2026-01-02T00:00:00Z".to_string());
    mock.set_search(SearchScript::Hits(vec![hit]));
    let found = search(
        &mock,
        "lighthouse",
        &allow(&["thread-a"]),
        &HashMap::new(),
        &ProjectionCache::new(),
    )
    .await;
    assert_eq!(found.matches[0].message_id, "named");
}

#[tokio::test]
async fn a_routine_turn_is_labelled_routine() {
    let mock = MockHarness::new();
    mock.set_projection(
        "thread-a",
        projection(
            vec![json!({
                "id": "routine",
                "type": "user_message",
                "text": "lighthouse check",
                "status": "completed",
                "scheduledTaskId": "scheduled-task:1",
                "startedAt": "2026-01-02T00:00:00Z",
            })],
            false,
        ),
    );
    let found = search(
        &mock,
        "lighthouse",
        &allow(&["thread-a"]),
        &HashMap::new(),
        &ProjectionCache::new(),
    )
    .await;
    assert_eq!(found.matches[0].role, Role::Routine);
    assert_eq!(found.matches[0].role.as_str(), "routine");
}

#[tokio::test]
async fn the_projection_is_kept_until_its_stamp_moves() {
    let (mock, allowed, cache) = ready(false);
    let first = stamps(&[("thread-a", "1")]);
    search(&mock, "lighthouse", &allowed, &first, &cache).await;
    search(&mock, "lighthouse", &allowed, &first, &cache).await;
    assert_eq!(mock.read_count("thread-a"), 1);
    let second = stamps(&[("thread-a", "2")]);
    search(&mock, "lighthouse", &allowed, &second, &cache).await;
    assert_eq!(mock.read_count("thread-a"), 2);
}

#[tokio::test]
async fn the_local_scan_stops_after_five_thousand_messages() {
    let mock = MockHarness::new();
    let mut items = vec![user("old", "the lighthouse stands", "2020-01-01T00:00:00Z")];
    for index in 0..5_000 {
        items.push(user(
            &format!("n{index}"),
            "nope",
            &format!("2026-06-01T00:{:02}:{:02}Z", index / 60, index % 60),
        ));
    }
    mock.set_projection("thread-a", projection(items, false));
    let found = search(
        &mock,
        "lighthouse",
        &allow(&["thread-a"]),
        &HashMap::new(),
        &ProjectionCache::new(),
    )
    .await;
    assert!(found.matches.is_empty());
    assert!(found.truncated);
}

#[tokio::test]
async fn the_local_scan_stops_after_two_mebibytes() {
    let mock = MockHarness::new();
    mock.set_projection(
        "thread-a",
        projection(
            vec![
                user("old", "the lighthouse stands", "2020-01-01T00:00:00Z"),
                user("big", &"x".repeat(2 * 1024 * 1024), "2026-01-01T00:00:00Z"),
            ],
            false,
        ),
    );
    let found = search(
        &mock,
        "lighthouse",
        &allow(&["thread-a"]),
        &HashMap::new(),
        &ProjectionCache::new(),
    )
    .await;
    assert!(found.matches.is_empty());
    assert!(found.truncated);
}
