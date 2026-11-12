use super::T3;
use crate::harness::{HarnessError, Turns, UserMessage};
use async_trait::async_trait;
use serde_json::json;

#[async_trait]
impl Turns for T3 {
    async fn send_user(&self, message: &UserMessage) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        let command = crate::harness::t3::transcript::command_with(
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
        let command = crate::bots::computer::steer_command(thread_id, text);
        let id = command["messageId"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        rpc.call("orchestration.dispatchCommand", command)
            .await
            .map_err(HarnessError::failed)?;
        Ok(id)
    }
}
