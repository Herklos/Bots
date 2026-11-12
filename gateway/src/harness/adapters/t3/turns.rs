use super::T3;
use crate::harness::{HarnessError, Turns, UserMessage};
use async_trait::async_trait;
use serde_json::json;

#[async_trait]
impl Turns for T3 {
    async fn send_user(&self, message: &UserMessage) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        let command = crate::chat::command_with(
            &message.thread_id,
            &message.text,
            &message.attachments,
            &message.message_id,
        );
        rpc.call("orchestration.dispatchCommand", command)
            .await
            .map_err(HarnessError::failed)?;
        Ok(())
    }

    async fn interrupt(&self, thread_id: &str, run_id: &str) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        rpc.dispatch(json!({ "type": "run.interrupt", "threadId": thread_id, "runId": run_id }))
            .await
            .map_err(HarnessError::failed)?;
        Ok(())
    }

    async fn steer(&self, thread_id: &str, text: &str) -> Result<String, HarnessError> {
        let id = uuid::Uuid::new_v4().to_string();
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        rpc.call(
            "orchestration.dispatchCommand",
            json!({
                "commandId": id,
                "type": "message.dispatch",
                "threadId": thread_id,
                "messageId": id,
                "text": text,
                "attachments": [],
                "createdBy": "user",
                "creationSource": "web",
                "deliveryIntent": "steer",
                "dispatchMode": { "type": "start_immediately" },
            }),
        )
        .await
        .map_err(HarnessError::failed)?;
        Ok(id)
    }
}
