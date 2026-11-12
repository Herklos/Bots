use super::*;

#[async_trait]
impl Routines for MockHarness {
    async fn list(&self) -> Result<Value, HarnessError> {
        Ok(json!({"tasks":self.scheduled.lock().unwrap().values().cloned().collect::<Vec<_>>()}))
    }
    async fn upsert(&self, task: &Value) -> Result<Value, HarnessError> {
        let mut task = task.clone();
        if task["id"].as_str().is_none_or(str::is_empty) {
            task["id"] = json!(format!("scheduled-task:{}", uuid::Uuid::new_v4()));
        }
        let id = task["id"].as_str().unwrap().to_string();
        if task["schedule"]["type"] == "webhook" {
            let previous = self.scheduled.lock().unwrap().get(&id).cloned();
            apply_webhook(&mut task, previous.as_ref());
        }
        self.scheduled.lock().unwrap().insert(id, task.clone());
        Ok(json!({"task":task}))
    }
    async fn set_enabled(&self, body: &Value) -> Result<Value, HarnessError> {
        let mut tasks = self.scheduled.lock().unwrap();
        let task = tasks
            .get_mut(body["id"].as_str().unwrap_or(""))
            .ok_or_else(|| missing("routine"))?;
        task["enabled"] = body["enabled"].clone();
        Ok(json!({"task":task}))
    }
    async fn delete(&self, body: &Value) -> Result<Value, HarnessError> {
        let id = body["id"].as_str().unwrap_or("");
        self.scheduled.lock().unwrap().remove(id);
        self.deliveries.lock().unwrap().remove(id);
        Ok(body.clone())
    }
    async fn run_now(&self, body: &Value) -> Result<Value, HarnessError> {
        Ok(body.clone())
    }
    async fn rotate_webhook_token(&self, id: &str) -> Result<Value, HarnessError> {
        let mut tasks = self.scheduled.lock().unwrap();
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| HarnessError::failed("routine not found"))?;
        if task["schedule"]["type"] != "webhook" {
            return Err(HarnessError::failed("That routine is not a webhook."));
        }
        let has_secret = task["webhook"]["hasSecret"].clone();
        task["webhook"] = json!({
            "path": hook_path(id),
            "url": Value::Null,
            "hasSecret": has_secret,
        });
        Ok(json!({"task": task.clone()}))
    }
    async fn list_deliveries(&self, id: &str) -> Result<Value, HarnessError> {
        if !self.scheduled.lock().unwrap().contains_key(id) {
            return Err(HarnessError::failed("routine not found"));
        }
        let deliveries = self
            .deliveries
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .unwrap_or_default();
        Ok(json!({ "deliveries": deliveries }))
    }
    async fn get_delivery(&self, id: &str, delivery_id: &str) -> Result<Value, HarnessError> {
        let delivery = self
            .deliveries
            .lock()
            .unwrap()
            .get(id)
            .and_then(|rows| rows.iter().find(|row| row["id"] == delivery_id).cloned())
            .ok_or_else(|| HarnessError::failed("delivery not found"))?;
        Ok(json!({ "delivery": delivery }))
    }
    async fn watch(&self) -> Result<crate::harness::RoutineFeed, HarnessError> {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let tasks = self
            .scheduled
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        tokio::spawn(async move {
            let _ = tx.send(Ok(json!({ "tasks": tasks }))).await;
            std::future::pending::<()>().await;
        });
        Ok(rx)
    }
}

impl MockHarness {
    /// Record one delivery at the front of the task's log. API tests seed the list this way.
    pub(crate) fn push_delivery(&self, task_id: &str, delivery: Value) {
        self.deliveries
            .lock()
            .unwrap()
            .entry(task_id.to_string())
            .or_default()
            .insert(0, delivery);
    }
}

/// Drop `signature.secret` and keep a token-bearing path the row can hide.
fn apply_webhook(task: &mut Value, previous: Option<&Value>) {
    let incoming_secret = take_secret(task);
    if let Some(hook) = previous
        .and_then(|prev| prev.get("webhook"))
        .filter(|hook| hook.is_object())
    {
        task["webhook"] = hook.clone();
        if incoming_secret {
            task["webhook"]["hasSecret"] = json!(true);
        }
        return;
    }
    let id = task["id"].as_str().unwrap_or("");
    task["webhook"] = json!({
        "path": hook_path(id),
        "url": Value::Null,
        "hasSecret": incoming_secret,
    });
}

fn take_secret(task: &mut Value) -> bool {
    let Some(signature) = task
        .get_mut("schedule")
        .and_then(|schedule| schedule.get_mut("signature"))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    matches!(
        signature.remove("secret"),
        Some(Value::String(secret)) if !secret.is_empty()
    )
}

fn hook_path(id: &str) -> String {
    format!(
        "/api/hooks/{}/{}",
        id.replace(':', "%3A"),
        uuid::Uuid::new_v4()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_webhook_keeps_deliveries_and_drops_the_secret() {
        let harness = MockHarness::new();
        let saved = harness
            .upsert(&json!({
                "title": "Hooks",
                "schedule": {
                    "type": "webhook",
                    "signature": { "header": "x-hub-signature-256", "secret": "s3cret-not-for-rows" }
                },
                "threadId": "t1",
            }))
            .await
            .unwrap();
        let id = saved["task"]["id"].as_str().unwrap().to_string();
        let path = saved["task"]["webhook"]["path"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(path.contains("/api/hooks/"));
        assert_eq!(saved["task"]["webhook"]["hasSecret"], true);
        assert!(!saved.to_string().contains("s3cret-not-for-rows"));
        harness.push_delivery(
            &id,
            json!({ "id": "delivery:older", "outcome": "filtered" }),
        );
        harness.push_delivery(
            &id,
            json!({ "id": "delivery:newer", "outcome": "accepted" }),
        );
        let listed = harness.list_deliveries(&id).await.unwrap();
        assert_eq!(listed["deliveries"][0]["id"], "delivery:newer");
        assert_eq!(listed["deliveries"][1]["outcome"], "filtered");
        let one = harness.get_delivery(&id, "delivery:newer").await.unwrap();
        assert_eq!(one["delivery"]["outcome"], "accepted");
        let rotated = harness.rotate_webhook_token(&id).await.unwrap();
        assert_ne!(rotated["task"]["webhook"]["path"], path);
        assert_eq!(rotated["task"]["webhook"]["hasSecret"], true);
        assert!(harness.get_delivery(&id, "missing").await.is_err());
        let plain = harness
            .upsert(&json!({ "title": "Daily", "schedule": { "type": "interval" } }))
            .await
            .unwrap();
        let plain_id = plain["task"]["id"].as_str().unwrap();
        assert!(harness.rotate_webhook_token(plain_id).await.is_err());
        assert!(harness.list_deliveries("missing").await.is_err());
    }
}
