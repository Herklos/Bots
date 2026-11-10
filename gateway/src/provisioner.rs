//! With no client open, gives a new Bot a project and a main thread on the harness, then
//! sends its brief. T3 Code is the harness wired today; other backends may be added.
use crate::AppState;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

pub fn default_model() -> Value {
    json!({ "instanceId": "claudeAgent", "model": "claude-haiku-5-5", "options": [{ "id": "effort", "value": "medium" }] })
}

/// The choice the picker writes. Effort stays medium unless the caller already set options.
pub fn model_selection(instance_id: &str, model: &str) -> Result<Value, &'static str> {
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

pub fn model_label(model: &str) -> String {
    match model {
        "claude-haiku-5-5" => "Haiku".into(),
        other => other.to_string(),
    }
}

/// Haiku, plus whatever this Bot or the settings already use, so the current choice is always listed.
pub fn model_choices(current: &Value, fallback: &Value) -> Vec<Value> {
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

pub fn project_model_update(project_id: &str, selection: &Value) -> Value {
    json!({
        "type": "project.update",
        "commandId": uuid::Uuid::new_v4().to_string(),
        "projectId": project_id,
        "defaultModelSelection": selection,
    })
}

pub fn thread_model_command(thread_id: &str, selection: &Value) -> Value {
    json!({
        "type": "thread.model-selection.set",
        "threadId": thread_id,
        "modelSelection": selection,
    })
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_keeps_haiku_and_the_current_choice() {
        let choices = model_choices(
            &json!({ "instanceId": "claudeAgent", "model": "claude-haiku-5-5" }),
            &Value::Null,
        );
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0]["label"], "Haiku");
        let extra = model_choices(
            &json!({ "instanceId": "other", "model": "local-model" }),
            &Value::Null,
        );
        assert_eq!(extra.len(), 2);
        assert_eq!(extra[1]["label"], "local-model");
        assert!(model_selection("claudeAgent", "claude-haiku-5-5").is_ok());
        assert!(model_selection("claudeAgent", "has space").is_err());
        let update = project_model_update("p", &default_model());
        assert_eq!(update["type"], "project.update");
        assert_eq!(update["defaultModelSelection"]["model"], "claude-haiku-5-5");
        assert_eq!(
            thread_model_command("t", &default_model())["type"],
            "thread.model-selection.set"
        );
    }

    #[test]
    fn empty_and_odd_models_are_refused_and_a_settings_default_is_listed_once() {
        assert!(model_selection("", "claude-haiku-5-5").is_err());
        assert!(model_selection("claudeAgent", "").is_err());
        assert!(model_selection("claudeAgent", "claude/haiku").is_err());
        let choices = model_choices(
            &json!({ "instanceId": "claudeAgent", "model": "claude-haiku-5-5" }),
            &json!({ "instanceId": "other", "model": "desk-model" }),
        );
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[1]["model"], "desk-model");
        let again = model_choices(
            &json!({ "instanceId": "other", "model": "desk-model" }),
            &json!({ "instanceId": "other", "model": "desk-model" }),
        );
        assert_eq!(again.len(), 2);
    }
}
