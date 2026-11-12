//! Secrets use `secrets.answerRequest`: `{ threadId, turnItemId, answer }` where
//! `answer` is `{ type: "save", secret }` or `{ type: "decline" }`.
//! Approvals and questions dispatch `runtime-request.respond` with `threadId`,
//! `requestId`, and whichever of `decision`, `answers`, `feedback`, and
//! `attachmentsByQuestionId` were set.
use super::T3;
use crate::harness::{Approval, Approvals, HarnessError};
use async_trait::async_trait;
use serde_json::{json, Value};

/// `runtime-request.respond`. Files, when present, are `attachmentsByQuestionId`.
fn respond_command(
    thread_id: &str,
    request_id: &str,
    decision: Option<&str>,
    answers: Option<&Value>,
    feedback: Option<&str>,
    attachments_by_question: Option<&Value>,
) -> Value {
    let mut command = json!({
        "type": "runtime-request.respond",
        "threadId": thread_id,
        "requestId": request_id,
    });
    if let Some(decision) = decision {
        command["decision"] = json!(decision);
    }
    if let Some(answers) = answers {
        command["answers"] = answers.clone();
    }
    if let Some(feedback) = feedback {
        command["feedback"] = json!(feedback);
    }
    if let Some(attachments) = attachments_by_question {
        command["attachmentsByQuestionId"] = attachments.clone();
    }
    command
}

#[async_trait]
impl Approvals for T3 {
    async fn answer(&self, approval: &Approval) -> Result<Value, HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        match approval {
            Approval::Secret {
                thread_id,
                turn_item_id,
                secret,
            } => {
                let answer = match secret {
                    Some(secret) => json!({ "type": "save", "secret": secret }),
                    None => json!({ "type": "decline" }),
                };
                rpc.call(
                    "secrets.answerRequest",
                    json!({
                        "threadId": thread_id,
                        "turnItemId": turn_item_id,
                        "answer": answer,
                    }),
                )
                .await
                .map_err(HarnessError::failed)
            }
            Approval::Request {
                thread_id,
                request_id,
                decision,
                answers,
                feedback,
                attachments_by_question,
            } => {
                let command = respond_command(
                    thread_id,
                    request_id,
                    decision.as_deref(),
                    answers.as_ref(),
                    feedback.as_deref(),
                    attachments_by_question.as_ref(),
                );
                rpc.dispatch(command).await.map_err(HarnessError::failed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_answer_keeps_files_on_attachments_by_question_id() {
        let files = json!({
            "q1": [{
                "type": "file",
                "id": "file-1",
                "name": "notes.txt",
                "mimeType": "text/plain",
                "sizeBytes": 5
            }]
        });
        let command = respond_command(
            "thread-1",
            "req-2",
            None,
            Some(&json!({ "q1": "the notes" })),
            None,
            Some(&files),
        );
        assert_eq!(command["type"], "runtime-request.respond");
        assert_eq!(command["threadId"], "thread-1");
        assert_eq!(command["requestId"], "req-2");
        assert_eq!(command["answers"]["q1"], "the notes");
        assert_eq!(command["attachmentsByQuestionId"], files);
        assert!(command.get("decision").is_none());
        let plain = respond_command(
            "thread-1",
            "req-2",
            Some("decline"),
            None,
            Some("Use an explicit path."),
            None,
        );
        assert!(plain.get("attachmentsByQuestionId").is_none());
        assert_eq!(plain["decision"], "decline");
    }
}
