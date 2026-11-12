//! Creating a project and setting its face or model use `projects.mutate`.
//! A main thread uses `orchestration.launchThread`. The brief is a dispatched
//! message queued behind an active run; a person's first message starts immediately.
use super::T3;
use crate::harness::{
    ComputerCatalog, HarnessError, LaunchThread, ProjectInfo, ProvisionedSend, Provisioning,
};
use async_trait::async_trait;
use serde_json::{json, Value};

#[async_trait]
impl Provisioning for T3 {
    async fn projects(&self) -> Result<Vec<ProjectInfo>, HarnessError> {
        let body = self
            .get_json("/api/projects")
            .await
            .map_err(HarnessError::failed)?;
        Ok(body["projects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|project| {
                Some(ProjectInfo {
                    id: project["id"].as_str()?.to_string(),
                    workspace_root: project["workspaceRoot"].as_str().unwrap_or("").to_string(),
                    title: project["title"].as_str().unwrap_or("").to_string(),
                    favicon_path: project["faviconPath"].as_str().unwrap_or("").to_string(),
                    project_icon_null: project["projectIcon"].is_null(),
                })
            })
            .collect())
    }

    async fn create_project(
        &self,
        project_id: &str,
        title: &str,
        workspace_root: &str,
        model: &Value,
    ) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        rpc.call(
            "projects.mutate",
            json!({
                "type": "project.create",
                "commandId": uuid::Uuid::new_v4().to_string(),
                "projectId": project_id,
                "title": title,
                "workspaceRoot": workspace_root,
                "createWorkspaceRootIfMissing": true,
                "defaultModelSelection": model,
            }),
        )
        .await
        .map_err(HarnessError::failed)?;
        Ok(())
    }

    async fn update_face(
        &self,
        project_id: &str,
        name: &str,
        favicon: &str,
    ) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        rpc.call(
            "projects.mutate",
            crate::bots::provisioner::project_face_update(project_id, name, favicon),
        )
        .await
        .map_err(HarnessError::failed)?;
        Ok(())
    }

    async fn launch(&self, spec: &LaunchThread) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        rpc.call(
            "orchestration.launchThread",
            json!({
                "commandId": uuid::Uuid::new_v4().to_string(),
                "threadId": spec.thread_id,
                "projectId": spec.project_id,
                "title": spec.title,
                "modelSelection": spec.model,
                "runtimeMode": spec.runtime_mode,
                "interactionMode": "default",
                "workspaceStrategy": { "type": "root" },
            }),
        )
        .await
        .map_err(HarnessError::failed)?;
        Ok(())
    }

    async fn send_provisioned(
        &self,
        thread_id: &str,
        text: &str,
        kind: ProvisionedSend,
    ) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        let (created_by, source, mode) = match kind {
            ProvisionedSend::Brief => ("agent", "server", "queue_after_active"),
            ProvisionedSend::First => ("user", "web", "start_immediately"),
        };
        rpc.dispatch(json!({
            "type": "message.dispatch",
            "threadId": thread_id,
            "messageId": uuid::Uuid::new_v4().to_string(),
            "text": text,
            "attachments": [],
            "createdBy": created_by,
            "creationSource": source,
            "dispatchMode": { "type": mode },
        }))
        .await
        .map_err(HarnessError::failed)?;
        Ok(())
    }

    async fn set_model(
        &self,
        project_id: Option<&str>,
        thread_id: Option<&str>,
        selection: &Value,
    ) -> Result<(), HarnessError> {
        let rpc = self.rpc().await.map_err(HarnessError::unavailable)?;
        if let Some(project) = project_id.filter(|id| !id.is_empty()) {
            rpc.call(
                "projects.mutate",
                crate::bots::provisioner::project_model_update(project, selection),
            )
            .await
            .map_err(HarnessError::failed)?;
        }
        if let Some(thread) = thread_id.filter(|id| !id.is_empty()) {
            rpc.dispatch(crate::bots::provisioner::thread_model_command(
                thread, selection,
            ))
            .await
            .map_err(HarnessError::failed)?;
        }
        Ok(())
    }

    async fn computers(&self) -> Result<ComputerCatalog, HarnessError> {
        super::computers::list(self).await
    }

    async fn provider_config(&self) -> Result<Value, HarnessError> {
        let config = self
            .rpc()
            .await
            .map_err(HarnessError::unavailable)?
            .call("server.getConfig", json!({}))
            .await
            .map_err(HarnessError::failed)?;
        Ok(crate::harness::providers::from_config(&config))
    }
}

/// Provider rows for `hbots doctor`. Same `server.getConfig` read as [`Provisioning::provider_config`].
/// Auth material stays in the adapter.
pub(crate) async fn provider_health(
    t3: &T3,
) -> Result<Vec<crate::harness::providers::ProviderHealth>, HarnessError> {
    let config = t3
        .rpc()
        .await
        .map_err(HarnessError::unavailable)?
        .call("server.getConfig", json!({}))
        .await
        .map_err(HarnessError::failed)?;
    Ok(crate::harness::providers::health_from_config(&config))
}
