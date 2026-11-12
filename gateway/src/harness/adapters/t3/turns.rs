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

    async fn set_runtime_mode(&self, thread_id: &str, mode: &str) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        rpc.dispatch(runtime_mode_command(thread_id, mode))
            .await
            .map_err(HarnessError::failed)?;
        Ok(())
    }
}

/// `thread.runtime-mode.set` on an existing thread. `dispatch` adds the command id.
pub(crate) fn runtime_mode_command(thread_id: &str, mode: &str) -> serde_json::Value {
    json!({
        "type": "thread.runtime-mode.set",
        "threadId": thread_id,
        "runtimeMode": mode,
    })
}

#[cfg(test)]
mod tests {
    use super::runtime_mode_command;

    #[test]
    fn a_runtime_mode_command_names_the_existing_thread_command() {
        let command = runtime_mode_command("thread-1", "auto");
        assert_eq!(command["type"], "thread.runtime-mode.set");
        assert_eq!(command["threadId"], "thread-1");
        assert_eq!(command["runtimeMode"], "auto");
        assert!(command.get("commandId").is_none());
    }
}
