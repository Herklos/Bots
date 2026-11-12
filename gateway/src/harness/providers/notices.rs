//! One warning per Bot when its provider crosses 80% or 95% of a limit window.
//!
//! The notice stays until usage falls below 75% or 90%, or the window resets.
//! A restart reads `.botapp/limit-notices.json` and does not send the same one again.
//! The alternative is the first ready provider whose worst window is under 70%.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Mutex;

const CLEAR_80: f64 = 75.0;
const CLEAR_95: f64 = 90.0;
const HEADROOM: f64 = 70.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BotUse {
    pub slug: String,
    pub instance_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    slug: String,
    #[serde(rename = "instanceId")]
    instance_id: String,
    #[serde(rename = "windowId")]
    window_id: String,
    #[serde(rename = "resetsAt", default)]
    resets_at: String,
    threshold: u8,
    at: String,
    label: String,
    #[serde(rename = "alternativeInstance", default)]
    alternative_instance: String,
    #[serde(rename = "alternativeModel", default)]
    alternative_model: String,
}

struct Window {
    id: String,
    resets_at: String,
    percent: f64,
}

#[derive(Debug)]
pub(crate) struct Book {
    path: PathBuf,
    records: Mutex<Vec<Record>>,
}

impl Default for Book {
    fn default() -> Self {
        Self::memory()
    }
}

impl Book {
    pub(crate) fn memory() -> Self {
        Self {
            path: PathBuf::new(),
            records: Mutex::new(Vec::new()),
        }
    }

    /// A missing or unreadable file starts empty. The next crossing can warn again.
    pub(crate) fn open(path: PathBuf) -> Self {
        let records = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|body| serde_json::from_value::<Vec<Record>>(body["notices"].clone()).ok())
            .unwrap_or_default();
        Self {
            path,
            records: Mutex::new(records),
        }
    }

    fn snapshot(&self) -> Vec<Record> {
        self.records
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    pub(super) fn attach_to(&self, view: &mut Value, slug: &str, instance: &str) {
        attach(view, &self.snapshot(), slug, instance);
    }

    pub(super) fn has_slug(&self, slug: &str) -> bool {
        self.snapshot().iter().any(|record| record.slug == slug)
    }

    pub(crate) async fn sync(&self, bots: &[BotUse], limits: &[Value], models: &Value, now: &str) {
        let current = self.snapshot();
        let (next, _) = evaluate(&current, bots, limits, models, now);
        if next == current {
            return;
        }
        if !self.path.as_os_str().is_empty() {
            let bytes = serde_json::to_vec_pretty(&json!({ "notices": &next }))
                .unwrap_or_else(|_| b"{}".to_vec());
            if let Err(err) = crate::storage::write_atomic(&self.path, &bytes, None).await {
                tracing::warn!("provider limit notices were not saved: {err}");
                return;
            }
        }
        *self.records.lock().unwrap_or_else(|err| err.into_inner()) = next;
    }
}

pub(crate) fn bot_uses(bots: &[Value], default_model: &Value) -> Vec<BotUse> {
    bots.iter()
        .filter_map(|bot| {
            let slug = bot["slug"].as_str().unwrap_or("").to_string();
            if slug.is_empty() {
                return None;
            }
            let running = crate::bots::provider_switch::running_model(&bot["model"], default_model);
            let instance_id = running["instanceId"].as_str().unwrap_or("").to_string();
            if instance_id.is_empty() {
                return None;
            }
            Some(BotUse { slug, instance_id })
        })
        .collect()
}

/// Records to keep, and the ones that are new this pass.
fn evaluate(
    book: &[Record],
    bots: &[BotUse],
    limits: &[Value],
    models: &Value,
    now: &str,
) -> (Vec<Record>, Vec<Record>) {
    let mut next = Vec::new();
    let mut emitted = Vec::new();
    let mut seen = BTreeSet::new();
    for bot in bots {
        if bot.slug.is_empty() || bot.instance_id.is_empty() || !seen.insert(bot.slug.clone()) {
            continue;
        }
        for record in book
            .iter()
            .filter(|record| record.slug == bot.slug && record.instance_id != bot.instance_id)
        {
            next.push(record.clone());
        }
        let Some(provider) = limits
            .iter()
            .find(|provider| provider["instanceId"].as_str() == Some(bot.instance_id.as_str()))
        else {
            continue;
        };
        let Some(worst) = worst(provider) else {
            continue;
        };
        let (alt_instance, alt_model) = alternative(limits, models, &bot.instance_id);
        let label = provider["label"]
            .as_str()
            .filter(|label| !label.is_empty())
            .unwrap_or(&bot.instance_id);
        for threshold in [95_u8, 80] {
            let clear = if threshold == 95 { CLEAR_95 } else { CLEAR_80 };
            if worst.percent < clear {
                continue;
            }
            let same = book.iter().find(|record| {
                record.slug == bot.slug
                    && record.instance_id == bot.instance_id
                    && record.threshold == threshold
                    && record.window_id == worst.id
                    && record.resets_at == worst.resets_at
            });
            if let Some(prev) = same {
                let mut kept = prev.clone();
                kept.label = label.to_string();
                kept.alternative_instance = alt_instance.clone();
                kept.alternative_model = alt_model.clone();
                next.push(kept);
            } else if worst.percent >= f64::from(threshold) {
                let record = Record {
                    slug: bot.slug.clone(),
                    instance_id: bot.instance_id.clone(),
                    window_id: worst.id.clone(),
                    resets_at: worst.resets_at.clone(),
                    threshold,
                    at: now.to_string(),
                    label: label.to_string(),
                    alternative_instance: alt_instance.clone(),
                    alternative_model: alt_model.clone(),
                };
                emitted.push(record.clone());
                next.push(record);
            }
        }
    }
    let order = |a: &Record, b: &Record| {
        (&a.slug, &a.instance_id, a.threshold).cmp(&(&b.slug, &b.instance_id, b.threshold))
    };
    next.sort_by(order);
    emitted.sort_by(order);
    (next, emitted)
}

fn attach(view: &mut Value, records: &[Record], slug: &str, instance: &str) {
    if slug.is_empty() || instance.is_empty() {
        return;
    }
    let mut mine: Vec<&Record> = records
        .iter()
        .filter(|record| record.slug == slug && record.instance_id == instance)
        .collect();
    if mine.is_empty() {
        return;
    }
    mine.sort_by_key(|record| record.threshold);
    let top = mine.last().map(|record| record.threshold);
    let base = max_ordinal(view);
    let mut events = view["events"].as_array().cloned().unwrap_or_default();
    for (index, record) in mine.iter().enumerate() {
        let show = Some(record.threshold) == top && !record.alternative_instance.is_empty();
        events.push(json!({
            "id": format!("limit-{}-{}-{}", record.slug, record.threshold, record.window_id),
            "ordinal": base + i64::try_from(index).unwrap_or(0) + 1,
            "at": record.at,
            "kind": "provider-near-limit",
            "text": sentence(record),
            "title": "",
            "label": record.label,
            "percent": record.threshold,
            "resetsAt": record.resets_at,
            "alternativeInstance": if show { record.alternative_instance.as_str() } else { "" },
            "alternativeModel": if show { record.alternative_model.as_str() } else { "" },
        }));
    }
    view["events"] = json!(events);
}

fn sentence(record: &Record) -> String {
    if record.resets_at.is_empty() {
        format!("{} is at {}%", record.label, record.threshold)
    } else {
        format!(
            "{} is at {}% until {}",
            record.label, record.threshold, record.resets_at
        )
    }
}

fn worst(provider: &Value) -> Option<Window> {
    let mut best: Option<Window> = None;
    for window in provider["windows"].as_array()? {
        let Some(percent) = window["usedPercent"].as_f64().filter(|n| n.is_finite()) else {
            continue;
        };
        let id = window["id"].as_str().unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let replace = best
            .as_ref()
            .is_none_or(|current| percent > current.percent);
        if replace {
            best = Some(Window {
                id: id.to_string(),
                resets_at: window["resetsAt"].as_str().unwrap_or("").to_string(),
                percent,
            });
        }
    }
    best
}

fn alternative(limits: &[Value], models: &Value, current: &str) -> (String, String) {
    for provider in limits {
        let id = provider["instanceId"].as_str().unwrap_or("");
        if id.is_empty() || id == current || provider["state"] != "ok" {
            continue;
        }
        if worst(provider).is_some_and(|window| window.percent >= HEADROOM) {
            continue;
        }
        if let Some(model) = first_model(models, id) {
            return (id.to_string(), model);
        }
    }
    (String::new(), String::new())
}

fn first_model(models: &Value, instance: &str) -> Option<String> {
    let provider = models["providers"]
        .as_array()?
        .iter()
        .find(|provider| provider["instanceId"].as_str() == Some(instance))?;
    provider["models"].as_array()?.iter().find_map(|model| {
        let slug = model["slug"].as_str()?;
        if slug.is_empty() {
            None
        } else {
            Some(slug.to_string())
        }
    })
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

pub(crate) fn start(state: std::sync::Arc<crate::AppState>) {
    tokio::spawn(async move {
        loop {
            if let Err(err) = tick(&state).await {
                tracing::warn!("provider limits: {err}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
}

pub(crate) async fn tick(state: &crate::AppState) -> anyhow::Result<()> {
    let limits_body = state.providers.limits(state.harness.as_ref()).await?;
    let models = state.providers.list(state.harness.as_ref()).await?;
    let bots = state
        .bots
        .list()
        .await
        .map_err(|err| anyhow::anyhow!(err.message))?;
    let settings = state.settings().await;
    let default = settings.get("defaultModel").cloned().unwrap_or(Value::Null);
    let limits = limits_body["providers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let now = crate::platform::release::iso8601(crate::platform::release::unix_now());
    state
        .providers
        .sync_notices(&bots, &default, &limits, &models, &now)
        .await;
    Ok(())
}

#[cfg(test)]
#[path = "notices_tests.rs"]
mod tests;
