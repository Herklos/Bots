//! With no client open, gives a new Bot a project and a main thread on the harness, then
//! sends its brief. T3 Code is the harness wired today; other backends may be added.
use crate::AppState;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

pub fn default_model() -> Value {
    json!({ "instanceId": "claudeAgent", "model": "claude-haiku-5-5", "options": [{ "id": "effort", "value": "medium" }] })
}

pub fn start(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = tick(&state).await {
                tracing::warn!("[provisioner] {e:#}");
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => {},
                _ = state.provision_now.notified() => {},
            }
        }
    });
}

async fn tick(state: &AppState) -> anyhow::Result<()> {
    let bots = state
        .bots
        .list()
        .await
        .map_err(|e| anyhow::anyhow!(e.message))?;
    if bots.is_empty() {
        return Ok(());
    }
    let projects = state.t3.get_json("/api/projects").await?;
    let projects = projects["projects"].as_array().cloned().unwrap_or_default();
    let settings = state.settings().await;
    let rpc = state.t3.rpc().await?;
    for bot in bots {
        let slug = bot["slug"].as_str().unwrap_or_default().to_string();
        let model = [&bot["model"], &settings["defaultModel"]]
            .into_iter()
            .find(|m| m.is_object())
            .cloned()
            .unwrap_or_else(default_model);
        let known = bot["projectId"]
            .as_str()
            .and_then(|id| projects.iter().find(|p| p["id"] == id));
        let project_id = match known {
            Some(p) => p["id"].as_str().unwrap_or_default().to_string(),
            None => {
                let existing = projects
                    .iter()
                    .find(|p| p["workspaceRoot"] == bot["workspaceRoot"]);
                let id = match existing.and_then(|p| p["id"].as_str()) {
                    Some(id) => id.to_string(),
                    None => {
                        let id = uuid::Uuid::new_v4().to_string();
                        rpc.call("projects.mutate", json!({
                            "type": "project.create", "commandId": uuid::Uuid::new_v4().to_string(), "projectId": id,
                            "title": bot["name"], "workspaceRoot": bot["workspaceRoot"], "createWorkspaceRootIfMissing": true,
                            "defaultModelSelection": model,
                        })).await?;
                        tracing::info!("[provisioner] created project for {slug}");
                        id
                    }
                };
                state
                    .bots
                    .save(&slug, &json!({ "projectId": id }), false)
                    .await
                    .map_err(|e| anyhow::anyhow!(e.message))?;
                id
            }
        };
        let thread = match bot["mainThreadId"].as_str() {
            Some(t) => t.to_string(),
            None => {
                let thread = uuid::Uuid::new_v4().to_string();
                rpc.call("orchestration.launchThread", json!({
                    "commandId": uuid::Uuid::new_v4().to_string(), "threadId": thread, "projectId": project_id,
                    "title": bot["name"], "modelSelection": model,
                    "runtimeMode": if settings["autoReview"] == false { "full-access" } else { "auto" },
                    "interactionMode": "default", "workspaceStrategy": { "type": "root" },
                })).await?;
                state
                    .bots
                    .save(
                        &slug,
                        &json!({ "mainThreadId": thread, "model": null }),
                        false,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!(e.message))?;
                tracing::info!("[provisioner] started main thread for {slug}");
                thread
            }
        };
        let brief = bot["pendingBrief"]
            .as_str()
            .or(bot["brief"].as_str())
            .filter(|b| !b.is_empty());
        if let Some(brief) = brief {
            let from = bot["createdBy"]
                .as_str()
                .map(|c| format!("From {c}: "))
                .unwrap_or_default();
            rpc.dispatch(json!({
                "type": "message.dispatch", "threadId": thread, "messageId": uuid::Uuid::new_v4().to_string(),
                "text": format!("{from}{brief}"), "attachments": [], "createdBy": "agent", "creationSource": "server",
                "dispatchMode": { "type": "queue_after_active" },
            })).await?;
            state
                .bots
                .save(
                    &slug,
                    &json!({ "brief": null, "pendingBrief": null }),
                    false,
                )
                .await
                .map_err(|e| anyhow::anyhow!(e.message))?;
        }
    }
    Ok(())
}
