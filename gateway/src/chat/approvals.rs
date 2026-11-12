//! Answering a pending request on a Bot's thread.
//!
//! This matches the request the client named to a secret or a runtime request.
//! The harness sends it. A secret is saved or declined. An approval or a question
//! carries a decision, answers, or feedback, and needs at least one of the first two.
//! A question may also carry files, already checked and turned into attachment records.
use crate::harness::{Approval, Approvals};
use crate::platform::error::{ApiError, ApiResult};
use serde_json::Value;

pub(crate) fn outbound(
    kind: &str,
    thread_id: &str,
    request_id: &str,
    body: &Value,
    attachments_by_question: Option<Value>,
) -> Result<Approval, String> {
    if thread_id.is_empty() || request_id.is_empty() {
        return Err("This Bot has no conversation to answer in.".into());
    }
    match kind {
        "secret" | "secret_request" => {
            if attachments_by_question.is_some() {
                return Err("Files go on a question.".into());
            }
            secret(thread_id, request_id, body)
        }
        "approval" | "approval_request" => {
            command(thread_id, request_id, body, false, attachments_by_question)
        }
        "question" | "user_input" | "user_input_request" => {
            command(thread_id, request_id, body, true, attachments_by_question)
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

fn command(
    thread_id: &str,
    request_id: &str,
    body: &Value,
    files_ok: bool,
    attachments_by_question: Option<Value>,
) -> Result<Approval, String> {
    let decision = body["decision"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let answers = body.get("answers").filter(|v| !v.is_null()).cloned();
    let feedback = body["feedback"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if attachments_by_question.is_some() && !files_ok {
        return Err("Files go on a question.".into());
    }
    if decision.is_none() && answers.is_none() && attachments_by_question.is_none() {
        return Err("Choose an answer first.".into());
    }
    Ok(Approval::Request {
        thread_id: thread_id.to_string(),
        request_id: request_id.to_string(),
        decision,
        answers,
        feedback,
        attachments_by_question,
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
            None,
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
            None,
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
            None,
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
            None,
        )
        .unwrap();
        let Approval::Request { answers, .. } = question else {
            panic!("a question is a request");
        };
        assert_eq!(answers.unwrap()["name"], "Ada");
        assert!(outbound("approval", "thread-1", "req-2", &json!({}), None).is_err());
        assert!(outbound(
            "nope",
            "thread-1",
            "req-2",
            &json!({ "decision": "accept" }),
            None,
        )
        .is_err());
        let files = json!({ "q1": [{ "type": "file", "id": "a", "name": "n.txt" }] });
        let with_file = outbound(
            "user_input_request",
            "thread-1",
            "req-3",
            &json!({}),
            Some(files.clone()),
        )
        .unwrap();
        let Approval::Request {
            answers,
            attachments_by_question,
            ..
        } = with_file
        else {
            panic!("files are enough to answer a question");
        };
        assert!(answers.is_none());
        assert_eq!(attachments_by_question.unwrap(), files);
        assert!(outbound(
            "approval_request",
            "thread-1",
            "req-2",
            &json!({ "decision": "accept" }),
            Some(files),
        )
        .is_err());
    }
}
