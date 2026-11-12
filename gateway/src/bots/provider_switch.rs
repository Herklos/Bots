//! A provider switch is a separate step from picking a model.
//!
//! The person confirms every switch. A request that names another provider and
//! does not set `confirmSwitch` to true is refused: nothing is saved, and the
//! harness is not told. A later request with `confirmSwitch: true` is the step
//! that applies it. A model on the provider already in use still applies on the
//! first request. The app sheet that asks for that confirmation waits on a real
//! cross-provider spike.

use serde_json::Value;

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
