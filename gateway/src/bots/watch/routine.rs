//! The hidden webhook routine a watch fires through (`docs/webhook-triggers-plan.md`,
//! Phase 4, option C). The poller's delivery reuses T3's dedupe, queue and log: the
//! user sees the same rows as any other routine. No signature: the POST stays local.
//! `coalesceKey` is the watch id, with a window equal to the cooldown, so a stream of
//! matches becomes one delivery.

use crate::bots::provisioner::default_model;
use serde_json::{json, Value};

const TITLE_PREFIX: &str = "Watch: ";
const PROMPT_MAX: usize = 2000;

/// The untrusted-payload wrapper prompt. `{{body...}}` keeps the webhook's own wrapper.
fn prompt(name: &str) -> String {
    let text = format!(
        "A watch named {{{{body.watch}}}} fired because {{{{body.reason}}}}. \
         Say in one short message what changed, using the previous and current values \
         as data. The watch is named {name}."
    );
    crate::chat::routines::webhook::wrap_prompt(&text)
}

/// The delivery body for one fire. `watchId` is the coalesce key; it is not a secret.
pub(crate) fn payload(watch: &Value, fire: &Value, at: &str) -> Value {
    json!({
        "watchId": watch["id"],
        "watch": watch["name"],
        "url": watch["target"]["url"],
        "at": at,
        "previous": fire["previous"],
        "current": fire["current"],
        "reason": fire["reason"],
    })
}

/// Build the hidden routine's task body. It is upserted with [`crate::harness::Routines`];
/// the id comes back and lives in the watch as `routineId`. The task body itself is
/// stored too, so a routine T3 lost can be recreated once.
pub(crate) fn hidden_task(bot: &Value, watch: &Value) -> Result<Value, &'static str> {
    let name = watch["name"].as_str().unwrap_or("").trim();
    if name.is_empty() {
        return Err("A watch needs a name.");
    }
    let title = format!("{TITLE_PREFIX}{name}");
    if title.chars().count() > 80 {
        return Err("A watch name is at most 74 characters.");
    }
    let prompt = prompt(name);
    if prompt.chars().count() > PROMPT_MAX {
        return Err("A watch name is too long.");
    }
    let project = bot["projectId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or("This Bot is still getting set up.")?;
    let thread = bot["mainThreadId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or("This Bot is still getting set up.")?;
    let model = if bot["model"].is_object() {
        bot["model"].clone()
    } else {
        default_model()
    };
    let cooldown = watch["fire"]["cooldownSec"].as_u64().unwrap_or(300);
    let schedule = json!({
        "type": "webhook",
        "signature": Value::Null,
        "maxDeliveryAgeMinutes": 60,
        "filters": [],
        "dedupe": {
            "coalesceKey": "{{body.watchId}}",
            "coalesceWindowSeconds": crate::bots::watch::coalesce_seconds(cooldown),
        },
    });
    Ok(json!({
        "title": title,
        "prompt": prompt,
        "enabled": true,
        "schedule": schedule,
        "projectId": project,
        "threadId": thread,
        "threadMode": "continue",
        "workspaceStrategy": { "type": "root" },
        "modelSelection": model,
        "runtimeMode": "approval-required",
        "interactionMode": "default",
        "createdBy": "user",
        "creationSource": "watch",
    }))
}
