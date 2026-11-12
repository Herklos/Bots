use super::*;

#[async_trait]
impl Routines for MockHarness {
    async fn list(&self) -> Result<Value, HarnessError> {
        Ok(json!({"tasks":self.scheduled.lock().unwrap().values().cloned().collect::<Vec<_>>()}))
    }
    async fn upsert(&self, task: &Value) -> Result<Value, HarnessError> {
        let mut task = task.clone();
        if task["id"].as_str().is_none_or(str::is_empty) {
            task["id"] = json!(uuid::Uuid::new_v4().to_string());
        }
        self.scheduled
            .lock()
            .unwrap()
            .insert(task["id"].as_str().unwrap().to_string(), task.clone());
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
        self.scheduled
            .lock()
            .unwrap()
            .remove(body["id"].as_str().unwrap_or(""));
        Ok(body.clone())
    }
    async fn run_now(&self, body: &Value) -> Result<Value, HarnessError> {
        Ok(body.clone())
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
