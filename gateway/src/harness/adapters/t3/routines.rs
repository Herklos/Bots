//! Tags, from `packages/contracts/src/rpc.ts` (`WS_METHODS`, the `scheduledTasks*` entries):
//! `scheduledTasks.subscribe`, `scheduledTasks.upsert`, `scheduledTasks.setEnabled`,
//! `scheduledTasks.delete`, `scheduledTasks.runNow`. `subscribe` is a stream; its first
//! item is `{ tasks }`. [`Routines::list`] takes that chunk and interrupts.
//! [`Routines::watch`] keeps the same stream open until it ends, then the caller drops it
//! (which interrupts) before opening another. There is no `scheduledTasks.listRuns`.
//!
//! Webhook log calls, from the same contract: `scheduledTasks.rotateWebhookToken`,
//! `scheduledTasks.listWebhookDeliveries`, `scheduledTasks.getWebhookDelivery`.
//! List and get always send `acceptAnyOutcome: true`.
use super::T3;
use crate::harness::{HarnessError, RoutineFeed, Routines};
use async_trait::async_trait;
use serde_json::{json, Value};

const LIST: &str = "scheduledTasks.subscribe";
const UPSERT: &str = "scheduledTasks.upsert";
const SET_ENABLED: &str = "scheduledTasks.setEnabled";
const DELETE: &str = "scheduledTasks.delete";
const RUN_NOW: &str = "scheduledTasks.runNow";
const ROTATE: &str = "scheduledTasks.rotateWebhookToken";
const LIST_DELIVERIES: &str = "scheduledTasks.listWebhookDeliveries";
const GET_DELIVERY: &str = "scheduledTasks.getWebhookDelivery";

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

    async fn rotate_webhook_token(&self, id: &str) -> Result<Value, HarnessError> {
        call(self, ROTATE, &json!({ "id": id })).await
    }

    async fn list_deliveries(&self, id: &str) -> Result<Value, HarnessError> {
        call(self, LIST_DELIVERIES, &list_body(id)).await
    }

    async fn get_delivery(&self, id: &str, delivery_id: &str) -> Result<Value, HarnessError> {
        call(self, GET_DELIVERY, &get_body(id, delivery_id)).await
    }

    async fn watch(&self) -> Result<RoutineFeed, HarnessError> {
        let client = self.rpc().await.map_err(HarnessError::unavailable)?;
        let mut stream = client
            .open_stream(LIST, json!({}))
            .await
            .map_err(HarnessError::unavailable)?;
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(item) = stream.recv().await {
                let item = item.map_err(HarnessError::failed);
                if tx.send(item).await.is_err() {
                    break;
                }
            }
        });
        Ok(rx)
    }
}

fn list_body(id: &str) -> Value {
    json!({ "id": id, "acceptAnyOutcome": true })
}

fn get_body(id: &str, delivery_id: &str) -> Value {
    json!({ "id": id, "deliveryId": delivery_id, "acceptAnyOutcome": true })
}

async fn call(t3: &T3, tag: &str, payload: &Value) -> Result<Value, HarnessError> {
    t3.rpc()
        .await
        .map_err(HarnessError::unavailable)?
        .call(tag, payload.clone())
        .await
        .map_err(HarnessError::failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_and_get_ask_for_every_outcome() {
        let listed = list_body("scheduled-task:1");
        assert_eq!(listed["id"], "scheduled-task:1");
        assert_eq!(listed["acceptAnyOutcome"], true);
        let one = get_body("scheduled-task:1", "delivery:2");
        assert_eq!(one["deliveryId"], "delivery:2");
        assert_eq!(one["acceptAnyOutcome"], true);
        assert!(one.get("secret").is_none());
    }
}
