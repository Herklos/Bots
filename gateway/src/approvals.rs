//! Answers to a Bot's requests (port of the `runtime-request.respond`, `thread.user-input.dismiss`
//! and `secrets.answerRequest` calls in `web/src/store.ts`): an approval decision, the answers to a
//! question, or a secret that is saved or declined. Served as `POST /api/bots/{slug}/requests/{id}/respond`.
//! The body carries exactly one of `decision`, `answers` or `secret`.
use crate::{
    error::{ApiError, ApiResult},
    t3::T3Client,
};
use serde_json::{json, Map, Value};

/// The decisions `runtime-request.respond` accepts (`ProviderApprovalDecision` in T3's contracts).
pub const DECISIONS: [&str; 5] = [
    "accept",
    "acceptForSession",
    "acceptAlways",
    "decline",
    "cancel",
];

/// A validated answer. A secret is never printed: this type has no `Debug`.
pub enum Answer {
    Decision(String),
    Answers(Map<String, Value>),
    /// `None` declines the secret request; `Some` saves the secret.
    Secret(Option<String>),
}

fn bad(message: &str) -> ApiError {
    ApiError::bad_request(message)
}

/// Request and turn-item IDs use Threebot's ID alphabet, at most 600 characters.
pub fn valid_request_id(id: &str) -> bool {
    (1..=600).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'%' | b'.' | b'-'))
}

/// Reads the body. Errors never repeat a submitted value, since one may be a secret.
pub fn parse_answer(body: &Value) -> ApiResult<Answer> {
    let Value::Object(map) = body else {
        return Err(bad("Send a JSON object"));
    };
    let given: Vec<&str> = ["decision", "answers", "secret"]
        .into_iter()
        .filter(|key| map.contains_key(*key))
        .collect();
    if given.len() != 1 {
        return Err(bad("Send exactly one of decision, answers or secret"));
    }
    match given[0] {
        "decision" => map["decision"]
            .as_str()
            .filter(|d| DECISIONS.contains(d))
            .map(|d| Answer::Decision(d.to_string()))
            .ok_or_else(|| bad("Unknown decision")),
        "answers" => {
            let Value::Object(answers) = &map["answers"] else {
                return Err(bad("answers must be an object"));
            };
            let valid = answers.values().all(|value| match value {
                Value::String(_) => true,
                Value::Array(list) => list.iter().all(Value::is_string),
                _ => false,
            });
            if !valid {
                return Err(bad("Each answer is text or a list of text"));
            }
            Ok(Answer::Answers(answers.clone()))
        }
        _ => match &map["secret"] {
            Value::Null => Ok(Answer::Secret(None)),
            Value::String(secret) => Ok(Answer::Secret(Some(secret.clone()))),
            _ => Err(bad("secret must be text or null")),
        },
    }
}

/// `runtime-request.respond` with a decision. The commandId is added by `T3Client::dispatch`.
pub fn decision_command(thread: &str, request_id: &str, decision: &str) -> Value {
    json!({
        "type": "runtime-request.respond", "threadId": thread,
        "requestId": request_id, "decision": decision,
    })
}

/// `runtime-request.respond` with the answers to a question.
pub fn answers_command(thread: &str, request_id: &str, answers: &Map<String, Value>) -> Value {
    json!({
        "type": "runtime-request.respond", "threadId": thread,
        "requestId": request_id, "answers": answers,
    })
}

/// The `secrets.answerRequest` payload. `secret` is the value to save, or `None` to decline.
pub fn secret_call(thread: &str, turn_item_id: &str, secret: &Option<String>) -> Value {
    let answer = match secret {
        None => json!({ "type": "decline" }),
        Some(value) => json!({ "type": "save", "secret": value }),
    };
    json!({ "threadId": thread, "turnItemId": turn_item_id, "answer": answer })
}

/// Sends the answer to T3 for the Bot's main thread.
pub async fn send(
    rpc: &T3Client,
    thread: &str,
    request_id: &str,
    answer: &Answer,
) -> anyhow::Result<()> {
    match answer {
        Answer::Decision(decision) => {
            rpc.dispatch(decision_command(thread, request_id, decision))
                .await?;
        }
        Answer::Answers(answers) => {
            rpc.dispatch(answers_command(thread, request_id, answers))
                .await?;
        }
        Answer::Secret(secret) => {
            rpc.call(
                "secrets.answerRequest",
                secret_call(thread, request_id, secret),
            )
            .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn parse(body: Value) -> ApiResult<Answer> {
        parse_answer(&body)
    }

    #[test]
    fn each_answer_shape_parses() {
        assert!(
            matches!(parse(json!({ "decision": "acceptAlways" })), Ok(Answer::Decision(d)) if d == "acceptAlways")
        );
        assert!(matches!(
            parse(json!({ "answers": { "q1": "yes", "q2": ["a", "b"] } })),
            Ok(Answer::Answers(a)) if a.len() == 2
        ));
        assert!(matches!(
            parse(json!({ "secret": null })),
            Ok(Answer::Secret(None))
        ));
        assert!(
            matches!(parse(json!({ "secret": "hunter2" })), Ok(Answer::Secret(Some(s))) if s == "hunter2")
        );
    }

    #[test]
    fn malformed_answers_are_refused() {
        for body in [
            json!("accept"),
            json!({}),
            json!({ "decision": "accept", "answers": {} }),
            json!({ "decision": "approve" }),
            json!({ "answers": [] }),
            json!({ "answers": { "q": 5 } }),
            json!({ "answers": { "q": ["a", 1] } }),
            json!({ "secret": 42 }),
        ] {
            let err = parse(body.clone())
                .err()
                .unwrap_or_else(|| panic!("accepted {body}"));
            assert_eq!(err.status, StatusCode::BAD_REQUEST, "{body}");
        }
    }

    #[test]
    fn a_refused_secret_is_never_echoed() {
        let err = parse(json!({ "secret": { "value": "hunter2" } }))
            .err()
            .unwrap();
        assert!(!err.message.contains("hunter2"));
        let err = parse(json!({ "secret": "hunter2", "decision": "accept" }))
            .err()
            .unwrap();
        assert!(!err.message.contains("hunter2"));
    }

    #[test]
    fn commands_have_the_shapes_t3_expects() {
        assert_eq!(
            decision_command("thread-1", "req-1", "accept"),
            json!({ "type": "runtime-request.respond", "threadId": "thread-1", "requestId": "req-1", "decision": "accept" })
        );
        let answers = Map::from_iter([("q1".to_string(), json!("yes"))]);
        assert_eq!(
            answers_command("thread-1", "req-2", &answers),
            json!({ "type": "runtime-request.respond", "threadId": "thread-1", "requestId": "req-2", "answers": { "q1": "yes" } })
        );
        assert_eq!(
            secret_call("thread-1", "item-1", &None),
            json!({ "threadId": "thread-1", "turnItemId": "item-1", "answer": { "type": "decline" } })
        );
        assert_eq!(
            secret_call("thread-1", "item-1", &Some("s3cret".into())),
            json!({ "threadId": "thread-1", "turnItemId": "item-1", "answer": { "type": "save", "secret": "s3cret" } })
        );
    }

    #[test]
    fn request_ids_use_the_id_alphabet() {
        assert!(valid_request_id("runtime-request:1.abc_%-x"));
        assert!(!valid_request_id(""));
        assert!(!valid_request_id("../etc"));
        assert!(!valid_request_id(&"a".repeat(601)));
    }
}
