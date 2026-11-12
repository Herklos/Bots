use super::T3;
use crate::harness::{HarnessError, Routines};
use async_trait::async_trait;
use serde_json::{json, Value};

const LIST: &str = "scheduledTasks.subscribe";
const UPSERT: &str = "scheduledTasks.upsert";
const SET_ENABLED: &str = "scheduledTasks.setEnabled";
const DELETE: &str = "scheduledTasks.delete";
const RUN_NOW: &str = "scheduledTasks.runNow";

#[async_trait]
impl Routines for T3 {
    async fn list(&self) -> Result<Value, HarnessError> {
        let value = self
            .rpc()
            .await
            .map_err(HarnessError::unavailable)?
            .read_until(LIST, json!({}), |_| true)
            .await
            .map_err(HarnessError::failed)?;
        Ok(value)
    }

    async fn upsert(&self, task: &Value) -> Result<Value, HarnessError> {
        call(self, UPSERT, task).await
    }

    async fn set_enabled(&self, body: &Value) -> Result<Value, HarnessError> {
        call(self, SET_ENABLED, body).await
    }

    async fn delete(&self, body: &Value) -> Result<Value, HarnessError> {
        call(self, DELETE, body).await
    }

    async fn run_now(&self, body: &Value) -> Result<Value, HarnessError> {
        call(self, RUN_NOW, body).await
    }
}

async fn call(t3: &T3, tag: &str, payload: &Value) -> Result<Value, HarnessError> {
    t3.rpc()
        .await
        .map_err(HarnessError::unavailable)?
        .call(tag, payload.clone())
        .await
        .map_err(HarnessError::failed)
}
