//! One snapshot: grandfather existing tasks, count a finished run once, then apply `decide`.
//! Writes use `requireExisting` on an upsert. A disable is `{ id, enabled }`.
//! The prompt is not copied into the notice.

use super::notice::{describe, Notice, NoticeFile};
use super::store::{ReviewFile, ReviewStore};
use super::{
    decide, note_run, record, run_status, Action, Limits, ReviewContext, ReviewMode, ReviewState,
    RunStatus,
};
use crate::bots::runtime_mode::RuntimeMode;
use crate::chat::routines::schedule_label;
use crate::harness::Routines;
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub(crate) struct Apply<'a> {
    pub routines: &'a dyn Routines,
    pub bots: &'a [Value],
    pub file: &'a ReviewFile,
    pub notices: &'a NoticeFile,
    pub now: &'a str,
    pub applied: &'a mut BTreeSet<String>,
}

pub(crate) async fn apply_snapshot(apply: &mut Apply<'_>, doc: &Value) -> Result<(), String> {
    let Some(mut tasks) = task_list(doc) else {
        return Ok(());
    };
    let mut store = apply.file.load().await.map_err(|err| err.message)?;
    if store.baseline.is_none() {
        let ids: Vec<&str> = tasks
            .iter()
            .filter_map(|task| task["id"].as_str())
            .collect();
        store.seal_baseline(ids, apply.now);
        return save_store(apply.file, &store).await;
    }
    tasks.sort_by(|left, right| task_id(left).cmp(task_id(right)));
    let mut dirty = false;
    for task in &tasks {
        if observe_run(&mut store, task) {
            dirty = true;
        }
        let id = task_id(task);
        if id.is_empty() {
            continue;
        }
        let Some(owner) = owner_of(task, apply.bots) else {
            continue;
        };
        let ctx = context_for(&owner, &store, task, &tasks, apply.bots);
        let action = decide(task, &store, Some(&ctx));
        if matches!(action, Action::Leave) {
            continue;
        }
        let failures = store.failure_count(id);
        if covered(apply.applied, id, &action, failures) {
            continue;
        }
        if let Err(err) = enact(apply.routines, task, &action).await {
            if dirty {
                let _ = save_store(apply.file, &store).await;
            }
            return Err(err);
        }
        record(&mut store, id, &owner.slug, &action, apply.now);
        save_store(apply.file, &store).await?;
        dirty = false;
        if let Some(notice) = emit_notice(&owner, task, &action, apply.now) {
            if let Err(err) = apply.notices.append(&notice).await {
                tracing::warn!("routine review notice: {err}");
            }
        }
        apply.applied.insert(effect_key(id, &action, failures));
    }
    if dirty {
        save_store(apply.file, &store).await?;
    }
    Ok(())
}

fn task_list(doc: &Value) -> Option<Vec<Value>> {
    if let Some(tasks) = doc.get("tasks").and_then(Value::as_array) {
        return Some(tasks.clone());
    }
    doc.as_array().cloned()
}

fn task_id(task: &Value) -> &str {
    task["id"].as_str().unwrap_or("")
}

async fn save_store(file: &ReviewFile, store: &ReviewStore) -> Result<(), String> {
    file.save(store).await.map_err(|err| err.message)
}

fn observe_run(store: &mut ReviewStore, task: &Value) -> bool {
    let id = task_id(task);
    if id.is_empty() || store.grandfathered.contains(id) {
        return false;
    }
    let Some((status, mark)) = finished_mark(task) else {
        return false;
    };
    if store.seen_runs.get(id).is_some_and(|prev| prev == &mark) {
        return false;
    }
    note_run(store, id, status);
    store.seen_runs.insert(id.to_string(), mark);
    true
}

fn finished_mark(task: &Value) -> Option<(RunStatus, String)> {
    let status = run_status(task["lastRunStatus"].as_str());
    let label = match status {
        RunStatus::Failed => "failed",
        RunStatus::Succeeded => "succeeded",
        RunStatus::Never | RunStatus::Running | RunStatus::Other => return None,
    };
    let count = task["runCount"].as_u64().unwrap_or(0);
    let at = task["lastRunAt"].as_str().unwrap_or("");
    Some((status, format!("{label}:{count}:{at}")))
}

struct Owner {
    slug: String,
    name: String,
    mode: ReviewMode,
    cap: RuntimeMode,
}

fn owner_of(task: &Value, bots: &[Value]) -> Option<Owner> {
    let thread = task["threadId"].as_str().filter(|id| !id.is_empty());
    let by_thread = thread.and_then(|id| {
        bots.iter()
            .find(|bot| bot["mainThreadId"].as_str() == Some(id))
    });
    let bot = if let Some(bot) = by_thread {
        bot
    } else {
        project_owner(bots, task["projectId"].as_str().unwrap_or(""))?
    };
    let slug = bot["slug"].as_str().filter(|slug| !slug.is_empty())?;
    let name = bot["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .unwrap_or(slug);
    Some(Owner {
        slug: slug.to_string(),
        name: name.to_string(),
        mode: ReviewMode::parse(bot.get("routineReview").and_then(Value::as_str))
            .unwrap_or(ReviewMode::DEFAULT),
        cap: RuntimeMode::parse(bot.get("runtimeMode").and_then(Value::as_str))
            .unwrap_or(RuntimeMode::FullAccess),
    })
}

fn project_owner<'a>(bots: &'a [Value], project: &str) -> Option<&'a Value> {
    if project.is_empty() {
        return None;
    }
    let mut found = None;
    for bot in bots {
        if bot["projectId"].as_str() == Some(project) {
            if found.is_some() {
                return None;
            }
            found = Some(bot);
        }
    }
    found
}

fn context_for(
    owner: &Owner,
    store: &ReviewStore,
    task: &Value,
    tasks: &[Value],
    bots: &[Value],
) -> ReviewContext {
    let id = task_id(task);
    let mut pending = 0u32;
    for (task_id, item) in &store.items {
        if item.slug == owner.slug && item.state == ReviewState::Pending && task_id != id {
            pending = pending.saturating_add(1);
        }
    }
    let mut enabled = 0u32;
    for other in tasks {
        let other_id = task_id(other);
        if other_id.is_empty() || other_id == id {
            continue;
        }
        let Some(bound) = owner_of(other, bots) else {
            continue;
        };
        if bound.slug != owner.slug {
            continue;
        }
        if store
            .items
            .get(other_id)
            .is_some_and(|item| matches!(item.state, ReviewState::Pending | ReviewState::Rejected))
        {
            continue;
        }
        if other["enabled"].as_bool() == Some(true) {
            enabled = enabled.saturating_add(1);
        }
    }
    ReviewContext {
        mode: owner.mode,
        runtime_cap: owner.cap,
        enabled,
        pending,
        limits: Limits::default(),
    }
}

fn covered(applied: &BTreeSet<String>, id: &str, action: &Action, failures: u32) -> bool {
    let key = effect_key(id, action, failures);
    if applied.contains(&key) {
        return true;
    }
    if let Action::Revise {
        every_ms,
        runtime_mode,
    } = action
    {
        let tail = format!(
            ":{id}:{}:{}",
            every_ms.unwrap_or(0),
            runtime_mode.unwrap_or("")
        );
        return applied
            .iter()
            .any(|saved| saved.starts_with("hold:") && saved.ends_with(&tail));
    }
    false
}

fn effect_key(id: &str, action: &Action, failures: u32) -> String {
    match action {
        Action::Leave => String::new(),
        Action::Hold {
            reason,
            every_ms,
            runtime_mode,
        } => format!(
            "hold:{}:{id}:{}:{}",
            reason.as_str(),
            every_ms.unwrap_or(0),
            runtime_mode.unwrap_or("")
        ),
        Action::Revise {
            every_ms,
            runtime_mode,
        } => format!(
            "revise:{id}:{}:{}",
            every_ms.unwrap_or(0),
            runtime_mode.unwrap_or("")
        ),
        Action::Delete { reason } => format!("delete:{id}:{}", reason.as_str()),
        Action::Pause => format!("pause:{id}:{failures}"),
    }
}

async fn enact(routines: &dyn Routines, task: &Value, action: &Action) -> Result<(), String> {
    match action {
        Action::Leave => Ok(()),
        Action::Hold {
            every_ms,
            runtime_mode,
            ..
        } => {
            if every_ms.is_some() || runtime_mode.is_some() {
                let body = upsert_body(task, true, *every_ms, *runtime_mode, "hold");
                routines
                    .upsert(&body)
                    .await
                    .map(|_| ())
                    .map_err(|err| err.to_string())
            } else {
                routines
                    .set_enabled(&disable_body(task))
                    .await
                    .map(|_| ())
                    .map_err(|err| err.to_string())
            }
        }
        Action::Revise {
            every_ms,
            runtime_mode,
        } => {
            let body = upsert_body(task, false, *every_ms, *runtime_mode, "revise");
            routines
                .upsert(&body)
                .await
                .map(|_| ())
                .map_err(|err| err.to_string())
        }
        Action::Delete { .. } => {
            let id = task_id(task);
            routines
                .delete(&json!({ "id": id }))
                .await
                .map(|_| ())
                .map_err(|err| err.to_string())
        }
        Action::Pause => routines
            .set_enabled(&disable_body(task))
            .await
            .map(|_| ())
            .map_err(|err| err.to_string()),
    }
}

fn disable_body(task: &Value) -> Value {
    json!({ "id": task_id(task), "enabled": false })
}

fn upsert_body(
    task: &Value,
    disable: bool,
    every_ms: Option<u64>,
    runtime_mode: Option<&str>,
    kind: &str,
) -> Value {
    let mut next = task.clone();
    let Some(obj) = next.as_object_mut() else {
        return next;
    };
    obj.remove("environmentId");
    if disable {
        obj.insert("enabled".into(), json!(false));
    }
    if let Some(ms) = every_ms {
        let schedule = obj
            .entry("schedule")
            .or_insert_with(|| json!({ "type": "interval" }));
        if !schedule.is_object() {
            *schedule = json!({ "type": "interval" });
        }
        schedule["everyMs"] = json!(ms);
    }
    if let Some(mode) = runtime_mode {
        obj.insert("runtimeMode".into(), json!(mode));
    }
    let id = obj
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let every = every_ms.map(|ms| ms.to_string()).unwrap_or_default();
    let mode = runtime_mode.unwrap_or("");
    obj.insert("requireExisting".into(), json!(true));
    obj.insert(
        "commandId".into(),
        json!(format!("hbots-routine-review:{kind}:{id}:{every}:{mode}")),
    );
    next
}

fn emit_notice(owner: &Owner, task: &Value, action: &Action, now: &str) -> Option<Notice> {
    let title = task["title"]
        .as_str()
        .filter(|title| !title.is_empty())
        .unwrap_or("Routine");
    let schedule = labeled(task, action);
    let body = describe(&owner.name, title, &schedule, action, Limits::default())?;
    Some(Notice {
        kind: body.kind.to_string(),
        id: task_id(task).to_string(),
        slug: owner.slug.clone(),
        title: title.to_string(),
        schedule,
        text: body.text,
        at: now.to_string(),
    })
}

fn labeled(task: &Value, action: &Action) -> String {
    let mut shown = task.clone();
    let (every_ms, runtime_mode) = match action {
        Action::Hold {
            every_ms,
            runtime_mode,
            ..
        }
        | Action::Revise {
            every_ms,
            runtime_mode,
        } => (*every_ms, *runtime_mode),
        Action::Delete { .. } | Action::Pause | Action::Leave => (None, None),
    };
    if let Some(ms) = every_ms {
        shown["schedule"]["everyMs"] = json!(ms);
    }
    if let Some(mode) = runtime_mode {
        shown["runtimeMode"] = json!(mode);
    }
    schedule_label(&shown)
}
