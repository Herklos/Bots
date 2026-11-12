//! A provider switch is a separate step from picking a model.
//!
//! The person confirms every switch. A request that names another provider and
//! does not set `confirmSwitch` to true is refused: nothing is saved, and the
//! harness is not told. A later request with `confirmSwitch: true` is the step
//! that applies it. A model on the provider already in use still applies on the
//! first request. The app shows a confirm sheet before it sends that request.
//! A successful switch is remembered beside the Bot folder and shown as a chip.

use serde_json::{json, Value};
use std::path::Path;

pub(crate) const CONFIRM_SWITCH: &str = "Confirm the provider switch before it is applied.";

/// How `PUT /api/bots/{slug}/model` treats this choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderStep {
    /// Save the choice and tell the harness.
    Apply,
    /// Another provider, and this request did not confirm the switch.
    AwaitConfirm,
}

/// The model this Bot is running: its own choice, then the settings default, then Haiku.
pub(crate) fn running_model(bot_model: &Value, settings_default: &Value) -> Value {
    [bot_model, settings_default]
        .into_iter()
        .find(|model| model.is_object())
        .cloned()
        .unwrap_or_else(crate::bots::provisioner::default_model)
}

/// `next_instance` is the provider on a choice that already passed the model check.
pub(crate) fn provider_step(running: &Value, next_instance: &str, body: &Value) -> ProviderStep {
    let same = instance_of(running) == next_instance;
    let confirmed = body.get("confirmSwitch").and_then(Value::as_bool) == Some(true);
    if same || confirmed {
        ProviderStep::Apply
    } else {
        ProviderStep::AwaitConfirm
    }
}

/// True when this choice leaves the provider the Bot is running.
pub(crate) fn switched(running: &Value, next_instance: &str) -> bool {
    instance_of(running) != next_instance
}

/// The name shown on the chip. A missing list keeps the instance id.
pub(crate) fn display_name(list: Option<&Value>, instance: &str) -> String {
    list.and_then(|list| list["providers"].as_array())
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["instanceId"].as_str() == Some(instance))
        })
        .and_then(|row| row["displayName"].as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(instance)
        .to_string()
}

fn book_path(root: &Path) -> std::path::PathBuf {
    root.join(".botapp").join("provider-switches.json")
}

/// Remember the latest confirmed switch. Old versions ignore this file.
pub(crate) async fn remember(root: &Path, slug: &str, name: &str) {
    if slug.is_empty() || name.is_empty() {
        return;
    }
    let path = book_path(root);
    let mut book = crate::storage::read_json(&path)
        .await
        .unwrap_or_else(|| json!({}));
    book[slug] = json!({ "name": name, "at": crate::vault::now_iso() });
    if let Err(err) = crate::storage::write_json(&path, &book).await {
        tracing::warn!("provider switch note was not saved: {err}");
    }
}

/// Attach one chip for the latest switch. The view is rebuilt on each read.
pub(crate) async fn attach(view: &mut Value, root: &Path, slug: &str) {
    let Some(row) = crate::storage::read_json(&book_path(root))
        .await
        .and_then(|book| book.get(slug).cloned())
    else {
        return;
    };
    let Some(name) = row["name"].as_str().filter(|name| !name.is_empty()) else {
        return;
    };
    let base = max_ordinal(view);
    let mut events = view["events"].as_array().cloned().unwrap_or_default();
    events.push(json!({
        "id": format!("provider-switch-{slug}"),
        "ordinal": base + 1,
        "at": row["at"].as_str().unwrap_or(""),
        "kind": "provider-switch",
        "text": "Switched to",
        "title": name,
    }));
    view["events"] = json!(events);
}

fn max_ordinal(view: &Value) -> i64 {
    let mut max = 0_i64;
    for key in ["messages", "events"] {
        let Some(rows) = view[key].as_array() else {
            continue;
        };
        for row in rows {
            if let Some(ordinal) = row["ordinal"].as_i64() {
                max = max.max(ordinal);
            }
        }
    }
    max
}

fn instance_of(selection: &Value) -> String {
    selection["instanceId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            crate::bots::provisioner::default_model()["instanceId"]
                .as_str()
                .unwrap_or("claudeAgent")
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claude() -> Value {
        json!({ "instanceId": "claudeAgent", "model": "claude-haiku-5-5" })
    }

    #[test]
    fn only_a_boolean_confirm_switch_lets_another_provider_through() {
        let running = claude();
        assert_eq!(
            provider_step(&running, "claudeAgent", &json!({})),
            ProviderStep::Apply
        );
        assert_eq!(
            provider_step(&running, "codex", &json!({})),
            ProviderStep::AwaitConfirm
        );
        assert_eq!(
            provider_step(&running, "codex", &json!({ "confirmSwitch": false })),
            ProviderStep::AwaitConfirm
        );
        assert_eq!(
            provider_step(&running, "codex", &json!({ "confirmSwitch": "true" })),
            ProviderStep::AwaitConfirm
        );
        assert_eq!(
            provider_step(&running, "codex", &json!({ "confirmSwitch": true })),
            ProviderStep::Apply
        );
    }

    #[test]
    fn a_missing_bot_model_uses_the_settings_default_then_haiku() {
        let codex = json!({ "instanceId": "codex", "model": "gpt-5.4" });
        assert_eq!(running_model(&Value::Null, &codex), codex);
        assert_eq!(
            provider_step(&running_model(&Value::Null, &codex), "codex", &json!({})),
            ProviderStep::Apply
        );
        assert_eq!(
            provider_step(
                &running_model(&Value::Null, &codex),
                "claudeAgent",
                &json!({})
            ),
            ProviderStep::AwaitConfirm
        );
        let haiku = crate::bots::provisioner::default_model();
        assert_eq!(running_model(&Value::Null, &Value::Null), haiku);
        assert_eq!(
            provider_step(
                &running_model(&json!({}), &Value::Null),
                "codex",
                &json!({})
            ),
            ProviderStep::AwaitConfirm
        );
    }
}
