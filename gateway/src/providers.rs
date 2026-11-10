//! The providers and models a Bot can use. T3 Code is the harness wired today: its server
//! config lists every provider instance with its models and what each model lets you tune.
//! Only what the app shows leaves this module: never an account's email or workspace.
use crate::t3::T3;
use serde_json::{json, Value};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

const KEEP: Duration = Duration::from_secs(30);

/// The provider list, read from T3 at most every 30 seconds.
#[derive(Default)]
pub struct Providers {
    cached: Mutex<Option<(Instant, Value)>>,
}

impl Providers {
    pub async fn list(&self, t3: &T3) -> anyhow::Result<Value> {
        if let Some((at, list)) = self.cached.lock().ok().and_then(|g| g.clone()) {
            if at.elapsed() < KEEP {
                return Ok(list);
            }
        }
        let config = t3.rpc().await?.call("server.getConfig", json!({})).await?;
        let list = from_config(&config);
        if let Ok(mut slot) = self.cached.lock() {
            *slot = Some((Instant::now(), list.clone()));
        }
        Ok(list)
    }
}

/// The effort levels a model offers, with its own default marked.
fn efforts(model: &Value) -> Vec<Value> {
    model["capabilities"]["optionDescriptors"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["id"] == "effort" && d["type"] == "select")
        .and_then(|d| d["options"].as_array())
        .into_iter()
        .flatten()
        .map(|o| {
            json!({
                "id": o["id"], "label": o["label"].as_str().unwrap_or(""),
                "isDefault": o["isDefault"] == true,
            })
        })
        .collect()
}

/// The providers a person can pick from: enabled, installed and ready, each with its visible models.
pub fn from_config(config: &Value) -> Value {
    let default = crate::provisioner::default_model();
    let providers: Vec<Value> = config["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["enabled"] == true && p["installed"] == true && p["status"] == "ready")
        .map(|p| {
            let instance = p["instanceId"].as_str().unwrap_or("");
            let hidden: Vec<&str> = config["settings"]["providerModelPreferences"][instance]
                ["hiddenModels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let models: Vec<Value> = p["models"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|m| m["slug"].as_str().is_some_and(|s| !hidden.contains(&s)))
                .map(|m| {
                    let levels = efforts(m);
                    let fallback = levels.iter().find(|l| l["isDefault"] == true);
                    json!({
                        "slug": m["slug"], "name": m["name"].as_str().or(m["slug"].as_str()).unwrap_or(""),
                        "efforts": levels, "effort": fallback.map(|l| l["id"].clone()).unwrap_or(Value::Null),
                        "recommended": instance == default["instanceId"] && m["slug"] == default["model"],
                    })
                })
                .collect();
            json!({
                "instanceId": instance, "driver": p["driver"],
                "displayName": p["displayName"].as_str().unwrap_or(instance),
                "account": p["auth"]["label"].as_str().unwrap_or(""),
                "models": models,
            })
        })
        .filter(|p| p["models"].as_array().is_some_and(|m| !m.is_empty()))
        .collect();
    json!({ "providers": providers })
}

/// The selection T3 stores for a choice, checked against what the provider offers. An effort the
/// model doesn't offer is refused; no effort means the model's own default. A choice that is the
/// Bot's current one is kept even when T3 no longer lists it.
pub fn selection(
    list: &Value,
    instance: &str,
    model: &str,
    effort: Option<&str>,
    current: &Value,
) -> Result<Value, String> {
    let keeps = current["instanceId"] == instance && current["model"] == model;
    let provider = list["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["instanceId"] == instance);
    let Some(provider) = provider else {
        return if keeps && effort.is_none() {
            Ok(current.clone())
        } else {
            Err("That provider isn't set up in T3 Code.".into())
        };
    };
    let name = provider["displayName"].as_str().unwrap_or(instance);
    let Some(entry) = provider["models"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["slug"] == model)
    else {
        return Err(format!("{name} doesn't offer that model."));
    };
    let levels = entry["efforts"].as_array().cloned().unwrap_or_default();
    let chosen = match effort.filter(|e| !e.is_empty()) {
        Some(e) if levels.iter().any(|l| l["id"] == e) => Some(e.to_string()),
        Some(_) => return Err(format!("{} doesn't offer that level.", entry["name"].as_str().unwrap_or(model))),
        None => entry["effort"].as_str().map(String::from),
    };
    let options = chosen
        .map(|v| json!([{ "id": "effort", "value": v }]))
        .unwrap_or_else(|| json!([]));
    Ok(json!({ "instanceId": instance, "model": model, "options": options }))
}

/// The effort in a stored selection.
pub fn effort_of(selection: &Value) -> String {
    selection["options"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|o| o["id"] == "effort")
        .and_then(|o| o["value"].as_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Value {
        let f: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/t3/providers.json")).unwrap();
        json!({ "providers": f["providers"], "settings": { "providerModelPreferences": {} } })
    }

    #[test]
    fn only_ready_providers_are_offered_with_their_models_and_effort_levels() {
        let list = from_config(&config());
        let providers = list["providers"].as_array().unwrap();
        let names: Vec<&str> = providers.iter().map(|p| p["displayName"].as_str().unwrap()).collect();
        assert!(names.contains(&"Claude Code") && names.contains(&"Codex"));
        assert!(!names.contains(&"Cursor"), "a disabled provider is not offered");
        let claude = providers.iter().find(|p| p["instanceId"] == "claudeAgent").unwrap();
        let opus = claude["models"].as_array().unwrap().iter().find(|m| m["slug"] == "claude-opus-5-5").unwrap();
        assert_eq!(opus["effort"], "medium");
        assert!(opus["efforts"].as_array().unwrap().len() >= 3);
        assert!(!list.to_string().contains("@"), "no email leaves this module");
    }

    #[test]
    fn hidden_models_and_the_account_email_stay_out() {
        let mut c = config();
        c["settings"]["providerModelPreferences"]["claudeAgent"] = json!({ "hiddenModels": ["claude-opus-5-5"] });
        c["providers"][0]["auth"]["email"] = json!("someone@example.com");
        let list = from_config(&c);
        let claude = list["providers"].as_array().unwrap().iter().find(|p| p["instanceId"] == "claudeAgent").unwrap();
        assert!(claude["models"].as_array().unwrap().iter().all(|m| m["slug"] != "claude-opus-5-5"));
        assert!(!list.to_string().contains("someone@example.com"));
    }

    #[test]
    fn a_choice_is_checked_against_what_the_provider_offers() {
        let list = from_config(&config());
        let ok = selection(&list, "claudeAgent", "claude-opus-5-5", Some("high"), &Value::Null).unwrap();
        assert_eq!(effort_of(&ok), "high");
        let default = selection(&list, "claudeAgent", "claude-opus-5-5", None, &Value::Null).unwrap();
        assert_eq!(effort_of(&default), "medium");
        assert!(selection(&list, "claudeAgent", "claude-opus-5-5", Some("turbo"), &Value::Null).unwrap_err().contains("level"));
        assert!(selection(&list, "claudeAgent", "gpt-nope", None, &Value::Null).unwrap_err().contains("model"));
        assert!(selection(&list, "cursor", "x", None, &Value::Null).unwrap_err().contains("provider"));
    }

    #[test]
    fn the_current_choice_is_kept_when_t3_no_longer_lists_it() {
        let list = from_config(&config());
        let current = json!({ "instanceId": "gone", "model": "old", "options": [] });
        assert_eq!(selection(&list, "gone", "old", None, &current).unwrap(), current);
        assert!(selection(&list, "gone", "other", None, &current).is_err());
    }
}
