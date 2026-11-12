use super::T3;
use crate::harness::{HarnessError, Previews};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

#[async_trait]
impl Previews for T3 {
    async fn inbox(
        &self,
        thread_id: &str,
        names: &HashMap<String, String>,
    ) -> Result<Option<Value>, HarnessError> {
        let snap = crate::harness::Transcript::read(self, thread_id).await?;
        Ok(snap.preview(names))
    }
}
