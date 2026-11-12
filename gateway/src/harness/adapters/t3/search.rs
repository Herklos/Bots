//! Conversation search. The command is `orchestration.searchThreads`.
//!
//! The page covers the whole environment. A match may omit `quality`, `hits`,
//! and `messageId`; older servers leave those out. `messageCreatedAt` may be null.

use super::T3;
use crate::harness::{HarnessError, ThreadHit, ThreadSearch};
use async_trait::async_trait;
use serde_json::{json, Value};

const SEARCH_THREADS: &str = "orchestration.searchThreads";
const MAX_LIMIT: u32 = 50;

pub(crate) fn search_payload(query: &str, limit: u32) -> Result<Value, HarnessError> {
    let query = query.trim();
    let chars = query.chars().count();
    if !(2..=200).contains(&chars) {
        return Err(HarnessError::failed(
            "search query must be 2 to 200 characters",
        ));
    }
    Ok(json!({ "query": query, "limit": limit.clamp(1, MAX_LIMIT) }))
}

fn matches_from(body: &Value) -> Option<Vec<ThreadHit>> {
    let list = body.get("matches")?.as_array()?;
    Some(list.iter().filter_map(one_match).collect())
}

fn one_match(value: &Value) -> Option<ThreadHit> {
    let thread_id = value.get("threadId")?.as_str()?.trim();
    if thread_id.is_empty() {
        return None;
    }
    let source = value.get("source")?.as_str()?;
    if source != "user" && source != "assistant" {
        return None;
    }
    let snippet: String = value.get("snippet")?.as_str()?.chars().take(240).collect();
    Some(ThreadHit {
        thread_id: thread_id.to_string(),
        project_id: value
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        source: source.to_string(),
        snippet,
        message_created_at: value
            .get("messageCreatedAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        quality: value
            .get("quality")
            .and_then(Value::as_f64)
            .filter(|quality| quality.is_finite()),
        hits: value.get("hits").and_then(Value::as_u64),
        message_id: value
            .get("messageId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string),
    })
}

#[async_trait]
impl ThreadSearch for T3 {
    async fn search_threads(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<ThreadHit>, HarnessError> {
        let payload = search_payload(query, limit)?;
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        let body = rpc
            .call(SEARCH_THREADS, payload)
            .await
            .map_err(HarnessError::failed)?;
        matches_from(&body).ok_or_else(|| HarnessError::failed("search reply has no matches"))
    }
}

#[cfg(test)]
mod tests {
    use super::{matches_from, search_payload, SEARCH_THREADS};
    use serde_json::json;

    #[test]
    fn the_command_is_search_threads() {
        assert_eq!(SEARCH_THREADS, "orchestration.searchThreads");
    }

    #[test]
    fn the_payload_trims_the_query_and_clamps_the_limit() {
        let payload = search_payload("  hello  ", 80).expect("payload");
        assert_eq!(payload["query"], "hello");
        assert_eq!(payload["limit"], 50);
        let payload = search_payload("hello", 0).expect("payload");
        assert_eq!(payload["limit"], 1);
        assert!(search_payload("a", 50).is_err());
        assert!(search_payload(&"a".repeat(201), 50).is_err());
        assert!(search_payload(&"a".repeat(200), 50).is_ok());
    }

    #[test]
    fn a_match_keeps_optional_fields_and_drops_a_null_time() {
        let hits = matches_from(&json!({
            "matches": [
                {
                    "threadId": " thread-a ",
                    "projectId": "project-a",
                    "source": "assistant",
                    "snippet": "the lighthouse",
                    "messageCreatedAt": null,
                    "quality": 0.8,
                    "hits": 4,
                    "messageId": "m1",
                    "extra": true
                },
                {
                    "threadId": "thread-b",
                    "projectId": "project-b",
                    "source": "user",
                    "snippet": "older shape",
                    "messageCreatedAt": "2026-01-01T00:00:00Z"
                },
                { "threadId": " ", "source": "user", "snippet": "no" },
                { "threadId": "thread-c", "source": "system", "snippet": "no" }
            ]
        }))
        .expect("matches");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].thread_id, "thread-a");
        assert_eq!(hits[0].project_id, "project-a");
        assert_eq!(hits[0].source, "assistant");
        assert_eq!(hits[0].quality, Some(0.8));
        assert_eq!(hits[0].hits, Some(4));
        assert_eq!(hits[0].message_id.as_deref(), Some("m1"));
        assert_eq!(hits[0].message_created_at, None);
        assert_eq!(hits[1].quality, None);
        assert_eq!(hits[1].hits, None);
        assert_eq!(hits[1].message_id, None);
        assert_eq!(
            hits[1].message_created_at.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
    }

    #[test]
    fn a_reply_without_matches_is_not_an_empty_page() {
        assert!(matches_from(&json!({ "ok": true })).is_none());
        let hits = matches_from(&json!({ "matches": [] })).expect("empty page");
        assert!(hits.is_empty());
    }

    #[test]
    fn a_snippet_is_cut_to_240_characters() {
        let snippet = "é".repeat(241);
        let hits = matches_from(&json!({
            "matches": [{
                "threadId": "thread-a",
                "source": "user",
                "snippet": snippet,
                "messageCreatedAt": null
            }]
        }))
        .expect("matches");
        assert_eq!(hits[0].snippet.chars().count(), 240);
    }
}
