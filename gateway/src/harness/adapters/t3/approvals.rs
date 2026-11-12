//! Secrets use `secrets.answerRequest`: `{ threadId, turnItemId, answer }` where
//! `answer` is `{ type: "save", secret }` or `{ type: "decline" }`.
//! Approvals and questions dispatch `runtime-request.respond` with `threadId`,
//! `requestId`, and whichever of `decision`, `answers`, and `feedback` were set.
use super::T3;
use crate::harness::{Approval, Approvals, HarnessError};
use async_trait::async_trait;
use serde_json::{json, Value};

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
            } => {
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
                rpc.dispatch(command).await.map_err(HarnessError::failed)
            }
        }
    }
}
