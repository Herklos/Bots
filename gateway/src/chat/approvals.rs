//! Answering a pending request on a Bot's thread.
//!
//! Secrets use `secrets.answerRequest` (`packages/contracts/src/rpc.ts`, payload
//! `SecretRequestAnswerInput` in `packages/contracts/src/secretRequest.ts`):
//! `{ threadId, turnItemId, answer: { type: "save", secret } | { type: "decline" } }`.
//! Approvals and questions use the orchestration command `runtime-request.respond`
//! (sent through `orchestration.dispatchCommand`; the fields are `threadId`, `requestId`,
//! and optional `decision`, `answers` and `feedback`, as in
//! `apps/server/src/orchestration-v2/testkit/OrchestratorScenario.ts`).
use crate::harness::{Approval, Approvals};
use crate::platform::error::{ApiError, ApiResult};
use serde_json::Value;

pub(crate) fn outbound(
    kind: &str,
    thread_id: &str,
    request_id: &str,
    body: &Value,
) -> Result<Approval, String> {
    if thread_id.is_empty() || request_id.is_empty() {
        return Err("This Bot has no conversation to answer in.".into());
    }
    match kind {
        "secret" | "secret_request" => secret(thread_id, request_id, body),
        "approval" | "approval_request" | "question" | "user_input" | "user_input_request" => {
            command(thread_id, request_id, body)
        }
        _ => Err("That request is not one this app can answer.".into()),
    }
}

fn secret(thread_id: &str, request_id: &str, body: &Value) -> Result<Approval, String> {
    let decline = body["decision"].as_str() == Some("decline")
        || body["answer"]["type"].as_str() == Some("decline");
    let secret = if decline {
        None
    } else {
        Some(
            body["secret"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Enter the secret, or decline.")?
                .to_string(),
        )
    };
    Ok(Approval::Secret {
        thread_id: thread_id.to_string(),
        turn_item_id: request_id.to_string(),
        secret,
    })
}

fn command(thread_id: &str, request_id: &str, body: &Value) -> Result<Approval, String> {
    let decision = body["decision"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let answers = body.get("answers").filter(|v| !v.is_null()).cloned();
    let feedback = body["feedback"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if decision.is_none() && answers.is_none() {
        return Err("Choose an answer first.".into());
    }
    Ok(Approval::Request {
        thread_id: thread_id.to_string(),
        request_id: request_id.to_string(),
        decision,
        answers,
        feedback,
    })
}

pub(crate) async fn send(approvals: &dyn Approvals, outbound: Approval) -> ApiResult<Value> {
    approvals
        .answer(&outbound)
        .await
        .map_err(|err| ApiError::from(anyhow::anyhow!(err.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Approval;
    use serde_json::json;

    #[test]
    fn a_secret_is_saved_or_declined_without_a_command() {
        let saved = outbound(
            "secret_request",
            "thread-1",
            "turn-9",
            &json!({ "secret": "hunter2" }),
        )
        .unwrap();
        let Approval::Secret {
            thread_id,
            turn_item_id,
            secret,
        } = saved
        else {
            panic!("secret answers are not orchestration commands");
        };
        assert_eq!(thread_id, "thread-1");
        assert_eq!(turn_item_id, "turn-9");
        assert_eq!(secret.as_deref(), Some("hunter2"));

        let declined = outbound(
            "secret",
            "thread-1",
            "turn-9",
            &json!({ "decision": "decline" }),
        )
        .unwrap();
        let Approval::Secret { secret, .. } = declined else {
            panic!("a decline is still a secret answer");
        };
        assert!(secret.is_none());
    }

    #[test]
    fn approvals_and_questions_become_a_runtime_request_response() {
        let approval = outbound(
            "approval_request",
            "thread-1",
            "req-2",
            &json!({ "decision": "decline", "feedback": "Use an explicit path." }),
        )
        .unwrap();
        let Approval::Request {
            request_id,
            decision,
            feedback,
            ..
        } = approval
        else {
            panic!("an approval is a request");
        };
        assert_eq!(request_id, "req-2");
        assert_eq!(decision.as_deref(), Some("decline"));
        assert_eq!(feedback.as_deref(), Some("Use an explicit path."));

        let question = outbound(
            "user_input_request",
            "thread-1",
            "req-3",
            &json!({ "answers": { "name": "Ada" } }),
        )
        .unwrap();
        let Approval::Request { answers, .. } = question else {
            panic!("a question is a request");
        };
        assert_eq!(answers.unwrap()["name"], "Ada");
        assert!(outbound("approval", "thread-1", "req-2", &json!({})).is_err());
        assert!(outbound(
            "nope",
            "thread-1",
            "req-2",
            &json!({ "decision": "accept" })
        )
        .is_err());
    }
}
