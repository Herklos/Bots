//! With no client open, gives a new Bot a project and a main thread on the harness, then
//! sends its brief. T3 Code is the harness wired today; other backends may be added.

use crate::AppState;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

#[cfg(test)]
mod tests;

/// The mode `orchestration.launchThread` already accepts. A missing or unknown field keeps
/// today's launch: `autoReview == false` is full access, otherwise auto. A stored choice wins.
pub(crate) fn launch_runtime_mode(bot: &Value, settings: &Value) -> &'static str {
    match bot
        .get("runtimeMode")
        .and_then(Value::as_str)
        .filter(|mode| matches!(*mode, "approval-required" | "auto" | "full-access"))
        .or_else(|| settings.get("defaultRuntimeMode").and_then(Value::as_str))
    {
        Some("approval-required") => "approval-required",
        Some("auto") => "auto",
        Some("full-access") => "full-access",
        _ => {
            if settings["autoReview"] == false {
                "full-access"
            } else {
                "auto"
            }
        }
    }
}

pub(crate) fn default_model() -> Value {
    json!({ "instanceId": "claudeAgent", "model": "claude-haiku-5-5", "options": [{ "id": "effort", "value": "medium" }] })
}

/// The choice the picker writes. Effort stays medium unless the caller already set options.
pub(crate) fn model_selection(instance_id: &str, model: &str) -> Result<Value, &'static str> {
    if instance_id.is_empty()
        || model.is_empty()
        || !model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err("Pick a model from the list.");
    }
    Ok(json!({
        "instanceId": instance_id,
        "model": model,
        "options": [{ "id": "effort", "value": "medium" }],
    }))
}

pub(crate) fn model_label(model: &str) -> String {
    match model {
        "claude-haiku-5-5" => "Haiku".into(),
        other => other.to_string(),
    }
}

/// Haiku, plus whatever this Bot or the settings already use, so the current choice is always listed.
pub(crate) fn model_choices(current: &Value, fallback: &Value) -> Vec<Value> {
    let mut out = vec![choice_row(&default_model())];
    for extra in [current, fallback] {
        if extra.is_object() {
            let row = choice_row(extra);
            if row["model"].as_str().is_some_and(|m| !m.is_empty())
                && out
                    .iter()
                    .all(|c| c["model"] != row["model"] || c["instanceId"] != row["instanceId"])
            {
                out.push(row);
            }
        }
    }
    out
}

fn choice_row(selection: &Value) -> Value {
    let instance = selection["instanceId"].as_str().unwrap_or("claudeAgent");
    let model = selection["model"].as_str().unwrap_or("");
    json!({
        "instanceId": instance,
        "model": model,
        "label": model_label(model),
    })
}

pub(crate) fn project_model_update(project_id: &str, selection: &Value) -> Value {
    json!({
        "type": "project.update",
        "commandId": uuid::Uuid::new_v4().to_string(),
        "projectId": project_id,
        "defaultModelSelection": selection,
    })
}

/// The thread list already shows this Bot: same title, same favicon, and no project icon.
pub(crate) fn face_in_sync(project: Option<&Value>, name: &str, favicon: &str) -> bool {
    let Some(project) = project else {
        return false;
    };
    project["title"].as_str() == Some(name)
        && project["faviconPath"].as_str() == Some(favicon)
        && project["projectIcon"].is_null()
}

/// `projects.mutate` payload that sets the project title and favicon to this Bot's face.
pub(crate) fn project_face_update(project_id: &str, name: &str, favicon: &str) -> Value {
    json!({
        "type": "project.update",
        "commandId": uuid::Uuid::new_v4().to_string(),
        "projectId": project_id,
        "title": name,
        "faviconPath": favicon,
        "projectIcon": null,
    })
}

pub(crate) fn thread_model_command(thread_id: &str, selection: &Value) -> Value {
    json!({
        "type": "thread.model-selection.set",
        "threadId": thread_id,
        "modelSelection": selection,
    })
}

/// A model change lands on this Bot's own project and main thread. A subagent is a Bot, so
/// the ids are the child's, never its parent's.
pub(crate) async fn push_model(
    rpc: &crate::harness::t3::T3Client,
    bot: &Value,
    selection: &Value,
) -> anyhow::Result<()> {
    if let Some(project) = bot["projectId"].as_str().filter(|id| !id.is_empty()) {
        rpc.call("projects.mutate", project_model_update(project, selection))
            .await?;
    }
    if let Some(thread) = bot["mainThreadId"].as_str().filter(|id| !id.is_empty()) {
        rpc.dispatch(thread_model_command(thread, selection))
            .await?;
    }
    Ok(())
}

pub(crate) fn start(state: Arc<AppState>) {
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
        let workspace = bot["workspaceRoot"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let shape = bot["shape"].as_str().unwrap_or("ball").to_string();
        let color = bot["color"].as_str().unwrap_or_default().to_string();
        let favicon = tokio::task::spawn_blocking(move || {
            crate::bots::face_icon::write_face(std::path::Path::new(&workspace), &shape, &color)
        })
        .await
        .map_err(|e| anyhow::anyhow!("face write task: {e}"))?
        .map_err(|e| anyhow::anyhow!("face write: {e}"))?;
        let listed = projects
            .iter()
            .find(|p| p["id"].as_str() == Some(project_id.as_str()));
        let name = bot["name"].as_str().unwrap_or_default();
        if !face_in_sync(listed, name, &favicon) {
            rpc.call(
                "projects.mutate",
                project_face_update(&project_id, name, &favicon),
            )
            .await?;
        }
        let thread = match bot["mainThreadId"].as_str() {
            Some(t) => t.to_string(),
            None => {
                let thread = uuid::Uuid::new_v4().to_string();
                rpc.call("orchestration.launchThread", json!({
                    "commandId": uuid::Uuid::new_v4().to_string(), "threadId": thread, "projectId": project_id,
                    "title": bot["name"], "modelSelection": model,
                    "runtimeMode": launch_runtime_mode(&bot, &settings),
                    "interactionMode": "default", "workspaceStrategy": { "type": "root" },
                })).await?;
                state
                    .bots
                    .save(&slug, &json!({ "mainThreadId": thread }), false)
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
        // What the person typed on New Bot goes out once, as their own message.
        if let Some(first) = bot["firstMessage"].as_str().filter(|m| !m.is_empty()) {
            let id = uuid::Uuid::new_v4().to_string();
            rpc.dispatch(json!({
                "type": "message.dispatch", "threadId": thread, "messageId": id, "text": first,
                "attachments": [], "createdBy": "user", "creationSource": "web",
                "dispatchMode": { "type": "start_immediately" },
            }))
            .await?;
            state
                .bots
                .save(&slug, &json!({ "firstMessage": null }), false)
                .await
                .map_err(|e| anyhow::anyhow!(e.message))?;
        }
        // The launch above already set this thread's runtime mode. There is no later RPC for it.
        // Routines wait until that thread exists, then run once. A failed upsert leaves the Bot
        // and the entries that did not land, so the next pass can retry them.
        if let Some(entries) = bot["pendingRoutines"]
            .as_array()
            .filter(|list| !list.is_empty())
        {
            let mut left = Vec::new();
            for entry in entries {
                let task = match crate::chat::routines::pending_routine_task(
                    &project_id,
                    &thread,
                    &model,
                    entry,
                ) {
                    Ok(task) => task,
                    Err(_) => {
                        tracing::warn!("[provisioner] routine for {slug} was not saved");
                        left.push(entry.clone());
                        continue;
                    }
                };
                match crate::chat::routines::change(
                    &state.t3,
                    &state.routine_history,
                    "upsert",
                    crate::chat::routines::UPSERT,
                    task,
                )
                .await
                {
                    Ok(_) => {}
                    Err(_) => {
                        tracing::warn!("[provisioner] routine for {slug} was not saved");
                        left.push(entry.clone());
                    }
                }
            }
            let patch = if left.is_empty() {
                json!({ "pendingRoutines": null })
            } else {
                json!({ "pendingRoutines": left })
            };
            state
                .bots
                .save(&slug, &patch, false)
                .await
                .map_err(|e| anyhow::anyhow!(e.message))?;
        }
    }
    Ok(())
}
