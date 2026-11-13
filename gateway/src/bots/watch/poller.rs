//! The watch poller (`docs/webhook-triggers-plan.md`, Phase 4). Started from `main.rs`
//! next to the other background loops. Every 15 seconds it runs the watches that are
//! due, at most 4 at a time; each poll is pinned to a checked IP, capped at 1 MiB,
//! and never repeated faster than the watch's interval. Failures back off
//! exponentially up to one hour; a day of failing disables the watch and says so in
//! its row. A fire POSTs to this machine's own T3 hook: no model run per poll.
//!
//! The cooldown is enforced here (`lastFireAt`), and again by T3's dedupe: the hidden
//! routine coalesces on the watch id with a window equal to the cooldown.

use super::evaluate::{self, Eval};
use super::{routine, WatchFile};
use crate::AppState;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

pub(crate) const TICK: Duration = Duration::from_secs(15);
pub(crate) const JITTER_MS: u64 = 15_000;
pub(crate) const AT_ONCE: usize = 4;
pub(crate) const BACKOFF_MAX_MS: u64 = 3_600_000;
pub(crate) const DISABLE_AFTER_MS: u64 = 24 * 3_600_000;
/// From this many consecutive failures on, the row carries `lastError`.
pub(crate) const FAILING_ROW_THRESHOLD: u64 = 5;

pub(crate) fn start(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            tick(&state, now_ms()).await;
            tokio::time::sleep(TICK).await;
        }
    });
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().saturating_mul(1000) + u64::from(d.subsec_millis()))
        .unwrap_or(0)
}

/// The next run: the interval (or the failure backoff, whichever is longer), plus a
/// jitter of up to 15 seconds. Never faster than the interval.
pub(crate) fn next_run_ms(every_ms: u64, failures: u64, now_ms: u64) -> u64 {
    let jitter = rand::random::<u64>() % JITTER_MS;
    next_run_at(every_ms, failures, now_ms, jitter)
}

pub(crate) fn next_run_at(every_ms: u64, failures: u64, now_ms: u64, jitter: u64) -> u64 {
    now_ms + backoff_or_every(every_ms, failures) + jitter
}

/// `everyMs * 2^failures`, capped at [`BACKOFF_MAX_MS`], floored at `everyMs`:
/// a watch with an interval above the cap waits its own interval.
pub(crate) fn backoff_ms(every_ms: u64, failures: u64) -> u64 {
    let factor = 1u64
        .checked_shl(failures.min(16) as u32)
        .unwrap_or(u64::MAX);
    every_ms
        .saturating_mul(factor)
        .min(BACKOFF_MAX_MS)
        .max(every_ms)
}

fn backoff_or_every(every_ms: u64, failures: u64) -> u64 {
    if failures == 0 {
        every_ms
    } else {
        backoff_ms(every_ms, failures)
    }
}

/// True once a watch has failed for a full day without a success in between.
pub(crate) fn failing_too_long(watch: &Value, now_ms: u64) -> bool {
    let failing_since = watch["state"]["failingSince"].as_u64().unwrap_or(0);
    watch["enabled"] == true
        && watch["state"]["failures"].as_u64().unwrap_or(0) > 0
        && failing_since > 0
        && now_ms.saturating_sub(failing_since) >= DISABLE_AFTER_MS
}

/// Run every due watch of every Bot at most [`AT_ONCE`] at a time. Returns the count.
pub(crate) async fn tick(state: &Arc<AppState>, now: u64) -> usize {
    let slugs = match state.bots.list().await {
        Ok(rows) => rows
            .iter()
            .filter_map(|bot| bot["slug"].as_str().map(str::to_string))
            .collect::<Vec<_>>(),
        Err(_) => return 0,
    };
    let mut due = Vec::new();
    for slug in &slugs {
        let file = WatchFile::new(&state.cfg.bots_root, slug);
        for watch in file.watches(&file.load().await) {
            if watch["enabled"] != true {
                continue;
            }
            if watch["state"]["nextAt"].as_u64().unwrap_or(0) <= now {
                due.push((slug.clone(), watch));
            }
        }
        if due.len() >= AT_ONCE {
            break;
        }
    }
    let count = due.len().min(AT_ONCE);
    for (slug, watch) in due.into_iter().take(AT_ONCE) {
        let state = Arc::clone(state);
        tokio::spawn(async move {
            watch_poll(&state, &slug, watch, now).await;
        });
    }
    count
}

/// One watch, one poll. The stored record is only rewritten through the store's lock.
pub(crate) async fn watch_poll(state: &AppState, slug: &str, watch: Value, now: u64) {
    if failing_too_long(&watch, now) {
        disable_after_a_day(state, slug, &watch).await;
        return;
    }
    let (mut fields, fire, eval) = match run_check(state, slug, &watch).await {
        // A failed poll or a body that would not evaluate is a failure: back off,
        // and count it towards the day of failures that disables the watch.
        Err(message) => (failure(&watch, now, &message), None, None),
        Ok(eval) => {
            let fired = evaluate::outcome(&watch["fire"], &watch["check"], &eval, &watch["state"]);
            (success(&eval), fired, Some(eval))
        }
    };
    // No refire inside the cooldown: the change is still recorded once.
    if let Some(fire) = &fire {
        let cooldown_ms = watch["fire"]["cooldownSec"].as_u64().unwrap_or(60) * 1000;
        let last_fire = watch["state"]["lastFireAt"].as_u64().unwrap_or(0);
        if now.saturating_sub(last_fire) < cooldown_ms {
            tracing::info!(
                "watch {} matched inside its cooldown",
                watch["id"].as_str().unwrap_or("?")
            );
        } else if deliver(state, slug, &watch, fire).await.is_some() {
            fields["lastFireAt"] = json!(now);
        } else {
            // The change stays live so the next poll can fire it again.
            fields = failure(
                &watch,
                now,
                "the delivery to this machine's hook did not land",
            );
        }
    }
    if let Some(eval) = eval {
        if watch["check"]["type"] == "hash" {
            fields["lastHash"] = json!(eval.value);
        }
    }
    fields["nextAt"] = json!(next_run_ms(
        watch["everyMs"].as_u64().unwrap_or(60_000),
        fields["failures"].as_u64().unwrap_or(0),
        now,
    ));
    commit(state, slug, &watch, &fields).await;
}

/// The check over the watch's target: an HTTP fetch guarded and pinned by `guard.rs`,
/// or a file under the Bot folder read through the Workspace.
async fn run_check(state: &AppState, slug: &str, watch: &Value) -> Result<Eval, String> {
    let target = &watch["target"];
    let check = &watch["check"];
    let allow_private = watch["allowPrivate"] == true;
    match target["type"].as_str() {
        Some("http") => {
            let outcome = super::guard::fetch(
                target["url"].as_str().unwrap_or(""),
                target["method"].as_str().unwrap_or("GET"),
                allow_private,
            )
            .await?;
            evaluate(check, outcome.status, &outcome.body)
        }
        Some("file") => {
            let file = WatchFile::new(&state.cfg.bots_root, slug);
            let rel = file.workspace_rel(target["path"].as_str().unwrap_or(""))?;
            let mut bytes = state
                .workspace
                .read(&rel)
                .await
                .map_err(|_bad| "The watched file could not be read.".to_string())?;
            bytes.truncate(super::guard::READ_CAP);
            evaluate(check, 0, &bytes)
        }
        _ => Err("A watch target is http or file.".to_string()),
    }
}

fn evaluate(check: &Value, status: u16, body: &[u8]) -> Result<Eval, String> {
    super::evaluate::evaluate(check, status, body)
}

/// Success clears the failure counters and keeps the value for the next comparison.
fn success(eval: &Eval) -> Value {
    json!({
        "last": eval.snapshot(),
        "lastTrue": match eval.truth { Some(v) => json!(v), None => Value::Null },
        "lastAt": crate::chat::routines::stamped(),
        "lastError": "",
        "failures": 0,
        "failingSince": 0,
    })
}

/// One failed poll. `lastError` shows in the row once the failures persist.
fn failure(watch: &Value, now: u64, message: &str) -> Value {
    let failures = watch["state"]["failures"].as_u64().unwrap_or(0) + 1;
    let mut fields = json!({
        "failures": failures,
        "failingSince": if failures == 1 { json!(now) } else { match watch["state"]["failingSince"].as_u64() { Some(s) if s > 0 => json!(s), _ => json!(now) } },
        "lastAt": crate::chat::routines::stamped(),
    });
    if failures >= FAILING_ROW_THRESHOLD {
        fields["lastError"] = json!(message);
    }
    fields
}

async fn disable_after_a_day(state: &AppState, slug: &str, watch: &Value) {
    tracing::warn!(
        "watch {} disabled after a day of failed checks",
        watch["id"].as_str().unwrap_or("?")
    );
    let fields = json!({ "lastError": "Disabled after 24 hours of failed checks." });
    commit(state, slug, watch, &fields).await;
    disable_flag(state, slug, watch).await;
}

/// The disable itself, one clean edit.
async fn disable_flag(state: &AppState, slug: &str, watch: &Value) {
    let file = WatchFile::new(&state.cfg.bots_root, slug);
    let id = watch["id"].as_str().unwrap_or("").to_string();
    let _ = file
        .update(|watches| {
            let stored = watches
                .iter_mut()
                .find(|w| w["id"].as_str() == Some(id.as_str()))
                .ok_or("The watch is gone.")?;
            stored["enabled"] = json!(false);
            stored["pausedReason"] = json!("twenty-four hours of failed checks");
            Ok(())
        })
        .await;
}

/// Merge the polled fields into the stored record and save it.
async fn commit(state: &AppState, slug: &str, watch: &Value, fields: &Value) {
    let id = match watch["id"].as_str() {
        Some(id) => id.to_string(),
        None => return,
    };
    let file = WatchFile::new(&state.cfg.bots_root, slug);
    let owned = fields.clone();
    if let Err(err) = file
        .update(|watches| {
            let stored = watches
                .iter_mut()
                .find(|w| w["id"].as_str() == Some(id.as_str()))
                .ok_or("The watch is gone.")?;
            let mut state_fields = stored["state"].take();
            merge_into(&mut state_fields, &owned);
            // A key the poll set to null is dropped; a key it never names stays.
            stored["state"] = state_fields;
            Ok(())
        })
        .await
    {
        tracing::warn!(slug, "watch state save failed: {err}");
    }
}

/// Overlay `fields` onto the stored state; null values are removed.
fn merge_into(state: &mut Value, fields: &Value) {
    let Value::Object(target) = state else {
        *state = fields.clone();
        return;
    };
    let Value::Object(overlay) = fields else {
        return;
    };
    for (key, value) in overlay {
        if value.is_null() {
            target.remove(key);
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
}

/// The local hook address for this watch's routine. The token is read from the task
/// row at fire time and never stored. A routine T3 lost is recreated once and logged.
async fn delivery_url(
    state: &AppState,
    slug: &str,
    watch: &Value,
    lost: &mut bool,
) -> Option<String> {
    let routine_id = watch["routineId"].as_str()?;
    match find_task(state, routine_id).await {
        Some(task) => {
            let path = task["webhook"]["path"].as_str().filter(|p| !p.is_empty())?;
            Some(format!("{}{path}", state.cfg.t3_url.trim_end_matches('/')))
        }
        // T3 lost the task: recreate it once from the stored body, keep the new id,
        // and log. A second loss on the same fire does not loop.
        None if !*lost => {
            *lost = true;
            recreate(state, slug, watch, routine_id).await
        }
        None => {
            tracing::warn!(
                "T3 still has no task for watch {}",
                watch["id"].as_str().unwrap_or("?")
            );
            None
        }
    }
}

/// POST the fire to the local hook. `None` when it did not land.
async fn deliver(state: &AppState, slug: &str, watch: &Value, fire: &Value) -> Option<()> {
    let mut lost = false;
    let url = delivery_url(state, slug, watch, &mut lost).await?;
    let payload = routine::payload(watch, fire, &crate::chat::routines::stamped());
    let sent = crate::http_client()
        .post(&url)
        .header("content-type", "application/json")
        .body(payload.to_string())
        .send()
        .await;
    match sent {
        Ok(response) if response.status().is_success() => {
            let _ = response.bytes().await;
            Some(())
        }
        // An error text could quote the token URL, so it is discarded.
        _ => {
            tracing::warn!(
                slug,
                "watch {} fired but the local hook refused",
                watch["id"].as_str().unwrap_or("?")
            );
            None
        }
    }
}

async fn find_task(state: &AppState, id: &str) -> Option<Value> {
    let routines = state.harness.routines().ok()?;
    let listed = routines.list().await.ok()?;
    listed["tasks"].as_array().and_then(|rows| {
        rows.iter()
            .find(|task| task["id"].as_str() == Some(id))
            .cloned()
    })
}

/// Recreate the hidden routine from the stored task body and update the stored id.
async fn recreate(state: &AppState, slug: &str, watch: &Value, lost_id: &str) -> Option<String> {
    tracing::warn!(
        "T3 lost the task for watch {}; recreating it once",
        watch["id"].as_str().unwrap_or("?")
    );
    let routines = state.harness.routines().ok()?;
    let saved = routines.upsert(&watch["task"]).await.ok()?;
    let task = saved.get("task").cloned().unwrap_or_else(|| saved.clone());
    let path = task["webhook"]["path"]
        .as_str()
        .filter(|p| !p.is_empty())?
        .to_string();
    let new_id = task["id"].as_str()?.to_string();
    if new_id != lost_id {
        let file = WatchFile::new(&state.cfg.bots_root, slug);
        let _ = file
            .update(|watches| {
                let stored = watches
                    .iter_mut()
                    .find(|w| w["id"].as_str() == watch["id"].as_str())
                    .ok_or("The watch is gone.")?;
                stored["routineId"] = json!(new_id);
                stored["task"] = task.clone();
                Ok(())
            })
            .await;
    }
    Some(format!("{}{path}", state.cfg.t3_url.trim_end_matches('/')))
}
