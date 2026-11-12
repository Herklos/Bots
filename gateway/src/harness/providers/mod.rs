//! The providers and models a Bot can use. T3 Code is the harness wired today: its server
//! config lists every provider instance with its models and what each model lets you tune.
//! Only what the app shows leaves this module: never an account's email or workspace.
//! The model list and the limit bars are two views of one 30-second cache.
use crate::harness::Harness;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

mod limits;
pub(crate) mod notices;

const REFRESH_WAIT: Duration = Duration::from_secs(60);

const KEEP: Duration = Duration::from_secs(30);

/// One read of the harness config, reduced to the model list and the limit bars.
/// The raw config is not kept: it can hold an email, a fingerprint, or a runtime path.
#[derive(Clone, Debug)]
struct Snapshot {
    fetched: Instant,
    at: String,
    list: Value,
    limits: Vec<Value>,
}

impl Snapshot {
    fn of(config: &Value) -> Self {
        let limits = config["providers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(limits::limits_of)
            .collect();
        Self {
            fetched: Instant::now(),
            at: crate::platform::release::iso8601(crate::platform::release::unix_now()),
            list: from_config(config),
            limits,
        }
    }
}

/// The provider list, read from T3 at most every 30 seconds, plus the limit-notice book.
#[derive(Debug, Default)]
pub struct Providers {
    cached: Mutex<Option<Snapshot>>,
    /// When a provider refresh last ran. A second one inside a minute does not call the harness.
    refreshed: Mutex<Option<Instant>>,
    notices: notices::Book,
}

impl Providers {
    /// Load saved limit notices. Tests keep [`Default`], which holds them in memory only.
    pub fn open(path: PathBuf) -> Self {
        Self {
            cached: Mutex::new(None),
            refreshed: Mutex::new(None),
            notices: notices::Book::open(path),
        }
    }

    async fn snapshot(&self, harness: &dyn Harness) -> anyhow::Result<Snapshot> {
        if let Some(hit) = self.cached.lock().ok().and_then(|guard| guard.clone()) {
            if hit.fetched.elapsed() < KEEP {
                return Ok(hit);
            }
        }
        let config = harness
            .provisioning()
            .map_err(|err| anyhow::anyhow!(err.to_string()))?
            .provider_config()
            .await
            .map_err(|err| anyhow::anyhow!(err.to_string()))?;
        let shot = Snapshot::of(&config);
        if let Ok(mut slot) = self.cached.lock() {
            *slot = Some(shot.clone());
        }
        Ok(shot)
    }

    pub async fn list(&self, harness: &dyn Harness) -> anyhow::Result<Value> {
        Ok(self.snapshot(harness).await?.list)
    }

    /// `{ providers, at }` for the same cached read as [`Self::list`]. `at` is when that read was taken.
    pub async fn limits(&self, harness: &dyn Harness) -> anyhow::Result<Value> {
        let shot = self.snapshot(harness).await?;
        Ok(json!({ "providers": shot.limits, "at": shot.at }))
    }

    /// Ask the harness to refresh provider limits, then drop the 30-second cache and re-read.
    /// A second call within a minute returns the cached list with `cooling: true` and does not
    /// call the harness again.
    pub async fn refresh(&self, harness: &dyn Harness) -> anyhow::Result<Value> {
        let cooling = self.cooling();
        if !cooling {
            harness
                .provisioning()
                .map_err(|err| anyhow::anyhow!(err.to_string()))?
                .refresh_providers()
                .await
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
            self.clear_cache();
            self.mark_refreshed();
        }
        let mut body = self.limits(harness).await?;
        body["cooling"] = json!(cooling);
        Ok(body)
    }

    pub fn attach_notices(&self, view: &mut Value, slug: &str, instance: &str) {
        self.notices.attach_to(view, slug, instance);
    }

    /// True when this Bot has a saved limit warning. The chat skips the settings read otherwise.
    pub fn has_notices(&self, slug: &str) -> bool {
        self.notices.has_slug(slug)
    }

    pub async fn sync_notices(
        &self,
        bots: &[Value],
        default_model: &Value,
        limits: &[Value],
        models: &Value,
        now: &str,
    ) {
        let uses = notices::bot_uses(bots, default_model);
        self.notices.sync(&uses, limits, models, now).await;
    }

    fn cooling(&self) -> bool {
        self.refreshed
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .is_some_and(|at| at.elapsed() < REFRESH_WAIT)
    }

    fn mark_refreshed(&self) {
        *self.refreshed.lock().unwrap_or_else(|err| err.into_inner()) = Some(Instant::now());
    }

    fn clear_cache(&self) {
        *self.cached.lock().unwrap_or_else(|err| err.into_inner()) = None;
    }
}

/// The effort levels a model offers, with its own default marked.
fn effort_descriptor(model: &Value) -> Option<&Value> {
    model["capabilities"]["optionDescriptors"]
        .as_array()?
        .iter()
        .find(|d| {
            matches!(d["id"].as_str(), Some("effort" | "reasoningEffort")) && d["type"] == "select"
        })
}

fn efforts(model: &Value) -> Vec<Value> {
    effort_descriptor(model)
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
pub(crate) fn from_config(config: &Value) -> Value {
    let default = crate::bots::provisioner::default_model();
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
                        "effortOptionId": effort_descriptor(m).and_then(|d| d["id"].as_str()).unwrap_or("effort"),
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

/// One provider from the harness config, without its auth object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProviderHealth {
    pub instance_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub installed: bool,
    pub status: String,
    pub models: usize,
}

/// Every provider in a config body, including ones that are not ready.
/// The auth object stays out: it can carry an email.
pub(crate) fn health_from_config(config: &Value) -> Vec<ProviderHealth> {
    config["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|provider| {
            let instance_id = provider["instanceId"].as_str().unwrap_or("").to_string();
            if instance_id.is_empty() {
                return None;
            }
            let display_name = provider["displayName"]
                .as_str()
                .filter(|name| !name.is_empty())
                .unwrap_or(&instance_id)
                .to_string();
            Some(ProviderHealth {
                instance_id,
                display_name,
                enabled: provider["enabled"] == true,
                installed: provider["installed"] == true,
                status: provider["status"].as_str().unwrap_or("unknown").to_string(),
                models: provider["models"].as_array().map(Vec::len).unwrap_or(0),
            })
        })
        .collect()
}

/// The selection T3 stores for a choice, checked against what the provider offers. An effort the
/// model doesn't offer is refused; no effort means the model's own default. A choice that is the
/// Bot's current one is kept even when T3 no longer lists it.
pub(crate) fn selection(
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
        Some(_) => {
            return Err(format!(
                "{} doesn't offer that level.",
                entry["name"].as_str().unwrap_or(model)
            ))
        }
        None => entry["effort"].as_str().map(String::from),
    };
    let options = chosen
        .map(
            |v| json!([{ "id": entry["effortOptionId"].as_str().unwrap_or("effort"), "value": v }]),
        )
        .unwrap_or_else(|| json!([]));
    Ok(json!({ "instanceId": instance, "model": model, "options": options }))
}

/// The effort in a stored selection.
pub(crate) fn effort_of(selection: &Value) -> String {
    selection["options"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|o| o["id"] == "effort" || o["id"] == "reasoningEffort")
        .and_then(|o| o["value"].as_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Value {
        let f: Value =
            serde_json::from_str(include_str!("../../../../tests/fixtures/t3/providers.json"))
                .unwrap();
        json!({ "providers": f["providers"], "settings": { "providerModelPreferences": {} } })
    }

    #[test]
    fn only_ready_providers_are_offered_with_their_models_and_effort_levels() {
        let list = from_config(&config());
        let providers = list["providers"].as_array().unwrap();
        let names: Vec<&str> = providers
            .iter()
            .map(|p| p["displayName"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"Claude Code") && names.contains(&"Codex"));
        assert!(
            !names.contains(&"Cursor"),
            "a disabled provider is not offered"
        );
        let claude = providers
            .iter()
            .find(|p| p["instanceId"] == "claudeAgent")
            .unwrap();
        let opus = claude["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["slug"] == "claude-opus-5-5")
            .unwrap();
        assert_eq!(opus["effort"], "medium");
        assert!(opus["efforts"].as_array().unwrap().len() >= 3);
        assert!(
            !list.to_string().contains("@"),
            "no email leaves this module"
        );
    }

    #[test]
    fn codex_reasoning_effort_is_exposed_and_saved_with_its_descriptor_id() {
        let list = from_config(&config());
        let selected =
            selection(&list, "codex", "gpt-6.1-sol", Some("high"), &Value::Null).unwrap();
        assert_eq!(selected["options"][0]["id"], "reasoningEffort");
        assert_eq!(effort_of(&selected), "high");
    }

    #[test]
    fn hidden_models_and_the_account_email_stay_out() {
        let mut c = config();
        c["settings"]["providerModelPreferences"]["claudeAgent"] =
            json!({ "hiddenModels": ["claude-opus-5-5"] });
        c["providers"][0]["auth"]["email"] = json!("someone@example.com");
        let list = from_config(&c);
        let claude = list["providers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["instanceId"] == "claudeAgent")
            .unwrap();
        assert!(claude["models"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["slug"] != "claude-opus-5-5"));
        assert!(!list.to_string().contains("someone@example.com"));
    }

    #[test]
    fn a_choice_is_checked_against_what_the_provider_offers() {
        let list = from_config(&config());
        let ok = selection(
            &list,
            "claudeAgent",
            "claude-opus-5-5",
            Some("high"),
            &Value::Null,
        )
        .unwrap();
        assert_eq!(effort_of(&ok), "high");
        let default =
            selection(&list, "claudeAgent", "claude-opus-5-5", None, &Value::Null).unwrap();
        assert_eq!(effort_of(&default), "medium");
        assert!(selection(
            &list,
            "claudeAgent",
            "claude-opus-5-5",
            Some("turbo"),
            &Value::Null
        )
        .unwrap_err()
        .contains("level"));
        assert!(
            selection(&list, "claudeAgent", "gpt-nope", None, &Value::Null)
                .unwrap_err()
                .contains("model")
        );
        assert!(selection(&list, "cursor", "x", None, &Value::Null)
            .unwrap_err()
            .contains("provider"));
    }

    #[test]
    fn health_lists_providers_that_are_not_ready_and_drops_auth() {
        let mut config = config();
        config["providers"][0]["auth"]["email"] = json!("someone@example.com");
        let rows = health_from_config(&config);
        let names: Vec<&str> = rows.iter().map(|row| row.display_name.as_str()).collect();
        assert!(names.contains(&"Cursor"));
        assert!(names.contains(&"Claude Code"));
        let cursor = rows.iter().find(|row| row.instance_id == "cursor").unwrap();
        assert!(!cursor.enabled);
        assert!(!cursor.installed);
        assert_eq!(cursor.status, "disabled");
        assert_eq!(cursor.models, 0);
        let dumped = format!("{rows:?}");
        assert!(!dumped.contains("someone@example.com"));
        assert!(!dumped.contains("auth"));
    }

    #[test]
    fn the_current_choice_is_kept_when_t3_no_longer_lists_it() {
        let list = from_config(&config());
        let current = json!({ "instanceId": "gone", "model": "old", "options": [] });
        assert_eq!(
            selection(&list, "gone", "old", None, &current).unwrap(),
            current
        );
        assert!(selection(&list, "gone", "other", None, &current).is_err());
    }
}
