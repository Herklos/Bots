//! With no client open, gives a new Bot a project and a main thread on the harness, then
//! sends its brief. T3 Code is the harness wired today; other backends may be added.
use crate::AppState;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

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
    rpc: &crate::t3::T3Client,
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
            crate::face_icon::write_face(std::path::Path::new(&workspace), &shape, &color)
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

    fn listed(title: &str, favicon: Option<&str>, icon: Option<&str>) -> Value {
        json!({
            "id": "p",
            "title": title,
            "faviconPath": favicon,
            "projectIcon": icon,
        })
    }

    #[test]
    fn a_matching_face_is_left_alone_and_any_drift_updates_the_project() {
        let path = ".bot/avatar-abc.svg";
        let project = listed("Ada", Some(path), None);
        assert!(face_in_sync(Some(&project), "Ada", path));

        let cases = [
            listed("Scout", Some(path), None),
            listed("Ada", Some(".bot/avatar-def.svg"), None),
            json!({ "id": "p", "title": "Ada", "projectIcon": null }),
            listed("Ada", Some(path), Some("icon")),
        ];
        for drifted in &cases {
            assert!(!face_in_sync(Some(drifted), "Ada", path));
            let update = project_face_update("p", "Ada", path);
            assert_eq!(update["type"], "project.update");
            assert_eq!(update["projectId"], "p");
            assert_eq!(update["title"], "Ada");
            assert_eq!(update["faviconPath"], path);
            assert!(update["projectIcon"].is_null());
            assert!(update["commandId"]
                .as_str()
                .is_some_and(|id| !id.is_empty()));
        }
        assert!(!face_in_sync(None, "Ada", path));
    }

    #[test]
    fn a_missing_mode_follows_auto_review_and_a_stored_mode_wins() {
        let review_on = json!({ "autoReview": true });
        let review_off = json!({ "autoReview": false });
        assert_eq!(launch_runtime_mode(&json!({}), &review_on), "auto");
        assert_eq!(
            launch_runtime_mode(
                &json!({}),
                &json!({"defaultRuntimeMode": "approval-required"})
            ),
            "approval-required"
        );
        assert_eq!(
            launch_runtime_mode(
                &json!({"runtimeMode": "auto"}),
                &json!({"defaultRuntimeMode": "approval-required"})
            ),
            "auto"
        );
        assert_eq!(launch_runtime_mode(&json!({}), &review_off), "full-access");
        assert_eq!(
            launch_runtime_mode(&json!({ "runtimeMode": "approval-required" }), &review_off),
            "approval-required"
        );
        assert_eq!(
            launch_runtime_mode(&json!({ "runtimeMode": "auto" }), &review_off),
            "auto"
        );
        assert_eq!(
            launch_runtime_mode(&json!({ "runtimeMode": "full-access" }), &review_on),
            "full-access"
        );
        assert_eq!(
            launch_runtime_mode(&json!({ "runtimeMode": "ask" }), &review_on),
            "auto"
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

    /// A local harness that records each RPC payload and answers success.
    async fn fake_harness() -> (String, Arc<std::sync::Mutex<Vec<Value>>>) {
        use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
        use axum::extract::State;
        use axum::response::IntoResponse;
        use axum::routing::get;
        use axum::Router;

        #[derive(Clone)]
        struct Log(Arc<std::sync::Mutex<Vec<Value>>>);

        async fn projects() -> axum::Json<Value> {
            axum::Json(json!({ "projects": [] }))
        }

        async fn socket(ws: WebSocketUpgrade, State(log): State<Log>) -> axum::response::Response {
            ws.on_upgrade(move |sock| read_rpc(sock, log))
                .into_response()
        }

        async fn read_rpc(mut sock: WebSocket, log: Log) {
            while let Some(Ok(msg)) = sock.recv().await {
                let Message::Text(text) = msg else {
                    continue;
                };
                let Ok(data) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if data["_tag"] != "Request" {
                    continue;
                }
                if let Some(payload) = data.get("payload") {
                    log.0.lock().expect("log").push(payload.clone());
                }
                let reply = json!({
                    "_tag": "Exit",
                    "requestId": data["id"],
                    "exit": { "_tag": "Success", "value": { "ok": true } },
                });
                if sock
                    .send(Message::Text(reply.to_string().into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }

        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/api/projects", get(projects))
            .route("/ws", get(socket))
            .with_state(Log(log.clone()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://127.0.0.1:{port}"), log)
    }

    fn fixture_state(dir: &std::path::Path, t3_url: String) -> Arc<AppState> {
        use crate::auth::Auth;
        use crate::bots::Bots;
        use crate::config::Config;
        use crate::exposure::{Exposure, Tailnet};
        use crate::projects::Projects;
        use crate::remote_access::{RemoteAccess, RemoteConfig};
        use crate::t3::T3;
        use crate::vault::Vault;

        let app_dir = dir.join("bots/.botapp");
        let cfg = Config {
            host: "127.0.0.1".into(),
            port: 0,
            t3_url: t3_url.clone(),
            t3_token: "fixture-only".into(),
            data_root: dir.to_path_buf(),
            bots_root: dir.join("bots"),
            static_dir: dir.join("dist"),
            password: String::new(),
            vault_file: dir.join("vault.enc"),
            vault_key_file: dir.join("vault-key"),
            bot_mcp_url: "http://127.0.0.1:1/mcp/bot".into(),
            allowed_origins: vec![],
            app_url: String::new(),
            access_team: String::new(),
            access_aud: String::new(),
            access_emails: vec![],
        };
        let tailnet = Arc::new(Tailnet::new());
        Arc::new(AppState {
            auth: Auth::load(&app_dir, String::new(), vec![]).unwrap(),
            t3: T3::new(t3_url, cfg.t3_token.clone()),
            bots: Bots::new(
                cfg.bots_root.clone(),
                cfg.bot_mcp_url.clone(),
                "fixture-only".into(),
            ),
            vault: Vault::new(cfg.vault_file.clone(), cfg.vault_key_file.clone()),
            projects: Projects::new(app_dir.join("projects.json")),
            threads: crate::thread_meta::ThreadMeta::new(app_dir.join("thread-meta.json")),
            roster: crate::roster::Roster::default(),
            providers: crate::providers::Providers::default(),
            rooms: crate::rooms::Rooms::new(app_dir.join("groups.json")),
            routine_history: crate::routines::RoutineHistory::new(
                app_dir.join("routine-history.json"),
            ),
            icons: crate::icons::Icons::new(dir.join("icons")),
            usage: crate::usage::Usage::new(app_dir.join("usage.json")),
            library: crate::library::Library::new(
                Arc::new(crate::workspace::LocalDisk::new(cfg.bots_root.clone())),
                cfg.bot_mcp_url.clone(),
                "fixture-only".into(),
            ),
            system: crate::release::System::new(cfg.data_root.clone(), cfg.app_dir()),
            mcp_token: "fixture-only".into(),
            provision_now: tokio::sync::Notify::new(),
            access: None,
            exposure: Exposure::from_env(|_| None),
            remote: Arc::new(RemoteAccess::new(
                RemoteConfig {
                    home: dir.to_path_buf(),
                    store_file: app_dir.join("access.json"),
                    url_file: app_dir.join("tunnel-url"),
                    local_port: 0,
                    env_password: String::new(),
                    env_tailnet: false,
                    env_tailnet_users: vec![],
                    tailscale_bin: None,
                },
                tailnet.clone(),
            )),
            tailnet,
            cfg,
        })
    }

    #[tokio::test]
    async fn a_child_is_launched_and_its_model_stays_on_the_child() {
        let (url, log) = fake_harness().await;
        let dir = tempfile::tempdir().unwrap();
        let state = fixture_state(dir.path(), url);
        state.bots.create(&json!({ "name": "Ada" })).await.unwrap();
        state
            .bots
            .create(&json!({
                "name": "Scout",
                "parentSlug": "ada",
                "role": "subagent",
            }))
            .await
            .unwrap();

        tick(&state).await.unwrap();

        let parent = state.bots.get("ada").await.unwrap().unwrap();
        let child = state.bots.get("scout").await.unwrap().unwrap();
        assert_eq!(child["role"], "subagent");
        assert_eq!(child["parentSlug"], "ada");
        let parent_project = parent["projectId"].as_str().unwrap();
        let child_project = child["projectId"].as_str().unwrap();
        let parent_thread = parent["mainThreadId"].as_str().unwrap();
        let child_thread = child["mainThreadId"].as_str().unwrap();
        assert_ne!(parent_project, child_project);
        assert_ne!(parent_thread, child_thread);

        let calls = log.lock().expect("log").clone();
        let child_root = child["workspaceRoot"].as_str().unwrap();
        assert!(calls
            .iter()
            .any(|p| { p["type"] == "project.create" && p["workspaceRoot"] == child_root }));
        assert!(calls.iter().any(|p| {
            p["threadId"] == child_thread
                && p["projectId"] == child_project
                && p["title"] == "Scout"
        }));

        log.lock().expect("log").clear();
        let selection = json!({
            "instanceId": "claudeAgent",
            "model": "claude-haiku-5-5",
            "options": [{ "id": "effort", "value": "low" }],
        });
        let rpc = state.t3.rpc().await.unwrap();
        push_model(&rpc, &child, &selection).await.unwrap();

        let calls = log.lock().expect("log").clone();
        let updates: Vec<_> = calls
            .iter()
            .filter(|p| p["type"] == "project.update")
            .collect();
        let threads: Vec<_> = calls
            .iter()
            .filter(|p| p["type"] == "thread.model-selection.set")
            .collect();
        assert_eq!(updates.len(), 1);
        assert_eq!(threads.len(), 1);
        assert_eq!(updates[0]["projectId"], child_project);
        assert_eq!(
            updates[0]["defaultModelSelection"]["options"][0]["value"],
            "low"
        );
        assert_eq!(threads[0]["threadId"], child_thread);
        assert!(calls.iter().all(|p| p["projectId"] != parent_project));
        assert!(calls.iter().all(|p| p["threadId"] != parent_thread));
    }
}
