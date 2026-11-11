//! Local usage counts in `bots/.botapp/usage.json`.
//!
//! The gateway counts what it did (messages, attachments, vault writes, icon fetches) and
//! keeps that on disk. Nothing is sent anywhere. A remote telemetry destination is not
//! in this repo.

use crate::error::{ApiError, ApiResult};
use crate::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Map, Value};
use std::{path::PathBuf, sync::Arc, time::SystemTime};
use tokio::sync::Mutex;

const KEEP_DAYS: usize = 400;

#[derive(Clone, Copy, Debug)]
pub enum Event {
    Message,
    Attachment,
    VaultWrite,
    IconFetch,
}

impl Event {
    fn key(self) -> &'static str {
        match self {
            Event::Message => "messages",
            Event::Attachment => "attachments",
            Event::VaultWrite => "vaultWrites",
            Event::IconFetch => "iconFetches",
        }
    }
}

#[derive(Debug)]
pub struct Usage {
    path: PathBuf,
    lock: Mutex<()>,
    /// Each Bot thread's turns, kept for as long as its `updatedAt` stamp holds.
    turns: Mutex<std::collections::HashMap<String, (String, Vec<Turn>)>>,
}

impl Usage {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
            turns: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Add one event. A broken file is left untouched.
    pub async fn note(&self, event: Event) {
        self.note_for("", event).await;
    }

    /// Count `event` for one Bot and for the machine. An empty slug is machine-only.
    pub async fn note_for(&self, slug: &str, event: Event) {
        if let Err(err) = self.note_at_for(slug, event, unix_now()).await {
            tracing::warn!("usage: {}", err.message);
        }
    }

    async fn note_at_for(&self, slug: &str, event: Event, unix_secs: u64) -> ApiResult<()> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        record_for(&mut doc, &day_key(unix_secs), slug, event.key(), KEEP_DAYS);
        crate::store::write_json(&self.path, &doc).await?;
        Ok(())
    }

    pub async fn snapshot(&self) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        let doc = read_doc(&self.path).await?;
        Ok(with_totals(doc))
    }

    /// The whole counts file.
    pub async fn doc(&self) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        read_doc(&self.path).await
    }

    /// A thread's finished turns. They are read again only when the thread's stamp moves.
    pub async fn turns_for(
        &self,
        s: &AppState,
        thread: &str,
        stamp: &str,
    ) -> anyhow::Result<Vec<Turn>> {
        if !stamp.is_empty() {
            if let Some((seen, turns)) = self.turns.lock().await.get(thread) {
                if seen == stamp {
                    return Ok(turns.clone());
                }
            }
        }
        let projection =
            s.t3.rpc()
                .await?
                .call(
                    "orchestration.getThreadProjection",
                    json!({ "threadId": thread }),
                )
                .await?;
        let turns = turns_from_projection(&projection);
        if !stamp.is_empty() {
            self.turns
                .lock()
                .await
                .insert(thread.to_string(), (stamp.to_string(), turns.clone()));
        }
        Ok(turns)
    }

    /// Messages and attachments this Bot sent through the gateway.
    pub async fn bot_activity(&self, slug: &str) -> ApiResult<(u64, u64)> {
        let _g = self.lock.lock().await;
        let doc = read_doc(&self.path).await?;
        Ok(bot_counts(&doc, slug))
    }
}

/// Token totals T3 reported on a thread projection. Cache figures are separate
/// from input and output, because T3 counts them that way.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct TokenTotals {
    pub turns: u64,
    pub input: u64,
    pub output: u64,
    pub cached: u64,
    pub cache_write: u64,
}

/// Sum `providerTurns[].turnTokenUsage`. A turn with no usage object means this
/// T3 did not report tokens. `unavailable` and local-command turns are seen, and not added.
pub(crate) fn tokens_from_projection(projection: &Value) -> (TokenTotals, bool) {
    let mut totals = TokenTotals::default();
    let mut known = false;
    let Some(turns) = projection.get("providerTurns").and_then(Value::as_array) else {
        return (totals, false);
    };
    for turn in turns {
        let Some(usage) = turn.get("turnTokenUsage").filter(|u| u.is_object()) else {
            continue;
        };
        known = true;
        if usage.get("usageStatus").and_then(Value::as_str) == Some("unavailable")
            || usage.get("localCommand") == Some(&Value::Bool(true))
        {
            continue;
        }
        totals.turns += 1;
        totals.input += count_u64(&usage["inputTokens"]);
        totals.output += count_u64(&usage["outputTokens"]);
        totals.cached += count_u64(&usage["cachedInputTokens"]);
        totals.cache_write += count_u64(&usage["cacheCreationTokens"]);
    }
    (totals, known)
}

/// One turn's token line, only when that turn and the message already share an id.
/// Cache stays separate from input and output. Unavailable turns and local commands contribute nothing.
pub(crate) fn turn_token_line(projection: &Value, message_id: &str) -> Option<Value> {
    if message_id.is_empty() {
        return None;
    }
    let turns = projection.get("providerTurns")?.as_array()?;
    for turn in turns {
        if !shares_message(turn, message_id) {
            continue;
        }
        let usage = turn.get("turnTokenUsage").filter(|item| item.is_object())?;
        if usage.get("usageStatus").and_then(Value::as_str) == Some("unavailable")
            || usage.get("localCommand") == Some(&Value::Bool(true))
        {
            return None;
        }
        let input = count_u64(&usage["inputTokens"]);
        let cached = count_u64(&usage["cachedInputTokens"]);
        return Some(json!({
            "inputTokens": input,
            "outputTokens": count_u64(&usage["outputTokens"]),
            "cachedInputTokens": cached,
            "cacheCreationTokens": count_u64(&usage["cacheCreationTokens"]),
        }));
    }
    None
}

fn shares_message(turn: &Value, message_id: &str) -> bool {
    [
        turn.get("messageId"),
        turn.get("assistantMessageId"),
        turn.get("id"),
    ]
    .into_iter()
    .any(|value| value.and_then(Value::as_str) == Some(message_id))
}

pub(crate) fn bot_payload(
    messages: u64,
    attachments: u64,
    tokens: &TokenTotals,
    tokens_known: bool,
    error: &str,
) -> Value {
    json!({
        "ok": true,
        "error": error,
        "messages": messages,
        "attachments": attachments,
        "turns": tokens.turns,
        "inputTokens": tokens.input,
        "outputTokens": tokens.output,
        "cachedInputTokens": tokens.cached,
        "cacheCreationTokens": tokens.cache_write,
        "tokensKnown": tokens_known,
    })
}

pub(crate) async fn get_usage(State(s): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    Ok(Json(s.usage.snapshot().await?))
}

async fn read_doc(path: &std::path::Path) -> ApiResult<Value> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => {
            #[allow(
                clippy::map_err_ignore,
                reason = "serde quotes usage.json; the client only hears that the file is not JSON"
            )]
            let doc: Value = serde_json::from_str(&text)
                .map_err(|_| ApiError::new(500, "usage.json is not valid JSON"))?;
            if !doc.is_object() {
                return Err(ApiError::new(500, "usage.json is not valid JSON"));
            }
            Ok(doc)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(json!({ "version": 1, "days": {} }))
        }
        Err(err) => Err(err.into()),
    }
}

/// Add one count on `day` for the machine, and under `bots[slug]` when `slug` is set.
pub(crate) fn record_for(doc: &mut Value, day: &str, slug: &str, key: &str, keep: usize) {
    record(doc, day, key, keep);
    if slug.is_empty() {
        return;
    }
    let Some(entry) = doc
        .get_mut("days")
        .and_then(|days| days.get_mut(day))
        .filter(|entry| entry.is_object())
    else {
        return;
    };
    if !entry["bots"].is_object() {
        entry["bots"] = json!({});
    }
    let bucket = entry["bots"]
        .as_object_mut()
        .expect("just set")
        .entry(slug.to_string())
        .or_insert_with(|| json!({}));
    if !bucket.is_object() {
        *bucket = json!({});
    }
    let n = bucket[key].as_u64().unwrap_or(0);
    bucket[key] = json!(n + 1);
}

pub(crate) fn bot_counts(doc: &Value, slug: &str) -> (u64, u64) {
    let mut messages = 0;
    let mut attachments = 0;
    let Some(days) = doc.get("days").and_then(Value::as_object) else {
        return (0, 0);
    };
    for day in days.values() {
        let Some(bucket) = day
            .get("bots")
            .and_then(|b| b.get(slug))
            .and_then(Value::as_object)
        else {
            continue;
        };
        messages += bucket.get("messages").and_then(Value::as_u64).unwrap_or(0);
        attachments += bucket
            .get("attachments")
            .and_then(Value::as_u64)
            .unwrap_or(0);
    }
    (messages, attachments)
}

fn count_u64(value: &Value) -> u64 {
    value.as_u64().unwrap_or(0)
}

/// Add one count on `day` and drop the oldest days past `keep`.
pub(crate) fn record(doc: &mut Value, day: &str, key: &str, keep: usize) {
    if !doc.is_object() {
        *doc = json!({ "version": 1, "days": {} });
    }
    doc["version"] = json!(1);
    if !doc["days"].is_object() {
        doc["days"] = json!({});
    }
    let days = doc["days"].as_object_mut().expect("just set");
    let entry = days.entry(day.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    let n = entry[key].as_u64().unwrap_or(0);
    entry[key] = json!(n + 1);
    if days.len() > keep {
        let mut keys: Vec<String> = days.keys().cloned().collect();
        keys.sort();
        let drop_n = days.len() - keep;
        for old in keys.into_iter().take(drop_n) {
            days.remove(&old);
        }
    }
}

pub(crate) fn with_totals(mut doc: Value) -> Value {
    let mut totals = Map::new();
    if let Some(days) = doc["days"].as_object() {
        for day in days.values() {
            if let Some(obj) = day.as_object() {
                for (k, v) in obj {
                    let Some(add) = v.as_u64() else { continue };
                    let n = totals.get(k).and_then(Value::as_u64).unwrap_or(0);
                    totals.insert(k.clone(), json!(n + add));
                }
            }
        }
    }
    doc["totals"] = Value::Object(totals);
    doc
}

pub(crate) fn day_key(unix_secs: u64) -> String {
    // Howard Hinnant, civil_from_days. UTC, no leap seconds.
    let z = i64::try_from(unix_secs / 86_400).unwrap_or(i64::MAX) + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = i64::try_from(yoe).unwrap_or(0) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------- the report: tokens by period, day and model ----------

/// One finished provider turn on a Bot's thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub at_ms: i64,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cached: u64,
    pub cache_write: u64,
}

/// `YYYY-MM-DDTHH:MM:SS(.sss)Z` as Unix milliseconds. T3 writes this form everywhere.
pub(crate) fn parse_iso_ms(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b' ') {
        return None;
    }
    let n = |from: usize, to: usize| text.get(from..to)?.parse::<i64>().ok();
    let (y, m, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (hh, mm, ss) = (n(11, 13)?, n(14, 16)?, n(17, 19)?);
    let millis = text
        .get(19..)
        .and_then(|rest| rest.strip_prefix('.'))
        .map(|rest| {
            rest.chars()
                .take_while(char::is_ascii_digit)
                .take(3)
                .collect::<String>()
        })
        .filter(|digits| !digits.is_empty())
        .map(|digits| format!("{digits:0<3}").parse::<i64>().unwrap_or(0))
        .unwrap_or(0);
    // days_from_civil (Howard Hinnant)
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + hh) * 60 + mm) * 60_000 + ss * 1000 + millis)
}

/// The finished turns of a thread projection that reported tokens. A turn is dated when it
/// completed, or started if it never did.
pub(crate) fn turns_from_projection(projection: &Value) -> Vec<Turn> {
    projection["providerTurns"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            let usage = t.get("turnTokenUsage").filter(|u| u.is_object())?;
            if usage["usageStatus"] == "unavailable" || usage["localCommand"] == true {
                return None;
            }
            let at = t["completedAt"]
                .as_str()
                .or(t["startedAt"].as_str())
                .and_then(parse_iso_ms)?;
            Some(Turn {
                at_ms: at,
                model: t["model"].as_str().unwrap_or("").to_string(),
                input: count_u64(&usage["inputTokens"]),
                output: count_u64(&usage["outputTokens"]),
                cached: count_u64(&usage["cachedInputTokens"]),
                cache_write: count_u64(&usage["cacheCreationTokens"]),
            })
        })
        .collect()
}

/// The day a moment falls on for someone `offset_min` minutes east of UTC, as a count of days.
fn day_index(ms: i64, offset_min: i64) -> i64 {
    (ms + offset_min * 60_000).div_euclid(86_400_000)
}

fn day_name(index: i64) -> String {
    day_key(u64::try_from((index * 86_400).max(0)).unwrap_or(0))
}

/// `(first day, last day, fill empty days)` of a period ending today, in the person's day.
pub(crate) fn period_days(period: &str, now_ms: i64, offset_min: i64) -> Option<(i64, i64, bool)> {
    let today = day_index(now_ms, offset_min);
    match period {
        "today" => Some((today, today, true)),
        "7d" => Some((today - 6, today, true)),
        "30d" => Some((today - 29, today, true)),
        "all" => Some((i64::MIN, today, false)),
        _ => None,
    }
}

/// Tokens and activity in a period: totals, one row per day, and a row per model.
/// `activity` holds each day's `(messages, attachments)` by day name.
pub(crate) fn report(
    turns: &[Turn],
    activity: &std::collections::BTreeMap<String, (u64, u64)>,
    range: (i64, i64, bool),
    offset_min: i64,
) -> Value {
    #[derive(Default, Clone, Copy)]
    struct Sum {
        turns: u64,
        input: u64,
        output: u64,
        cached: u64,
        cache_write: u64,
    }
    let (first, last, fill) = range;
    let mut by_day: std::collections::BTreeMap<i64, Sum> = std::collections::BTreeMap::new();
    let mut by_model: std::collections::BTreeMap<String, Sum> = std::collections::BTreeMap::new();
    let mut total = Sum::default();
    for t in turns {
        let day = day_index(t.at_ms, offset_min);
        if day < first || day > last {
            continue;
        }
        for sum in [
            by_day.entry(day).or_default(),
            by_model.entry(t.model.clone()).or_default(),
            &mut total,
        ] {
            sum.turns += 1;
            sum.input += t.input;
            sum.output += t.output;
            sum.cached += t.cached;
            sum.cache_write += t.cache_write;
        }
    }
    // Activity days come as names; turn them into indexes inside the range.
    let mut messages: std::collections::BTreeMap<i64, (u64, u64)> =
        std::collections::BTreeMap::new();
    for (name, (m, a)) in activity {
        if let Some(ms) = parse_iso_ms(&format!("{name}T00:00:00Z")) {
            let day = ms.div_euclid(86_400_000);
            if day >= first && day <= last {
                messages.insert(day, (*m, *a));
            }
        }
    }
    let (mut msgs, mut atts) = (0, 0);
    for (m, a) in messages.values() {
        msgs += m;
        atts += a;
    }
    let mut days: Vec<i64> = by_day.keys().chain(messages.keys()).copied().collect();
    if fill {
        days.extend(first..=last);
    }
    days.sort_unstable();
    days.dedup();
    let series: Vec<Value> = days
        .iter()
        .map(|d| {
            let s = by_day.get(d).copied().unwrap_or_default();
            let (m, a) = messages.get(d).copied().unwrap_or_default();
            json!({ "day": day_name(*d), "turns": s.turns, "input": s.input, "output": s.output,
                "cached": s.cached, "cacheWrite": s.cache_write, "messages": m, "attachments": a })
        })
        .collect();
    let mut models: Vec<(String, Sum)> = by_model.into_iter().collect();
    models.sort_by_key(|a| std::cmp::Reverse(a.1.input + a.1.output));
    json!({
        "ok": true,
        "totals": { "messages": msgs, "attachments": atts, "turns": total.turns, "input": total.input, "output": total.output,
            "cached": total.cached, "cacheWrite": total.cache_write, "tokens": total.input + total.output },
        "series": series,
        "byModel": models.iter().map(|(model, s)| json!({ "model": model, "turns": s.turns, "input": s.input,
            "output": s.output, "cached": s.cached, "cacheWrite": s.cache_write, "tokens": s.input + s.output })).collect::<Vec<_>>(),
    })
}

/// A Bot's `(messages, attachments)` for each day it has any, or the machine's with an empty slug.
pub(crate) fn activity_by_day(
    doc: &Value,
    slug: &str,
) -> std::collections::BTreeMap<String, (u64, u64)> {
    doc["days"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(day, entry)| {
            let bucket = if slug.is_empty() {
                entry
            } else {
                &entry["bots"][slug]
            };
            let m = bucket["messages"].as_u64().unwrap_or(0);
            let a = bucket["attachments"].as_u64().unwrap_or(0);
            (m + a > 0).then(|| (day.clone(), (m, a)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_utc_days() {
        assert_eq!(day_key(0), "1970-01-01");
        assert_eq!(day_key(86_400), "1970-01-02");
        assert_eq!(day_key(1_582_934_400), "2020-02-29");
        assert_eq!(day_key(1_794_268_800), "2026-11-10");
    }

    #[test]
    fn counts_add_up_and_old_days_drop() {
        let mut doc = json!({ "version": 1, "days": {} });
        record(&mut doc, "2026-11-10", "messages", 3);
        record(&mut doc, "2026-11-10", "messages", 3);
        record(&mut doc, "2026-11-09", "vaultWrites", 3);
        assert_eq!(doc["days"]["2026-11-10"]["messages"], 2);
        let view = with_totals(doc.clone());
        assert_eq!(view["totals"]["messages"], 2);
        assert_eq!(view["totals"]["vaultWrites"], 1);
        for n in 0..5 {
            record(&mut doc, &format!("2026-01-{n:02}"), "messages", 2);
        }
        assert_eq!(doc["days"].as_object().unwrap().len(), 2);
        assert!(doc["days"].get("2026-01-00").is_none());
    }

    #[tokio::test]
    async fn a_note_lands_on_disk_and_a_broken_file_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        let usage = Usage::new(path.clone());
        usage
            .note_at_for("", Event::Message, 1_794_268_800)
            .await
            .unwrap();
        usage
            .note_at_for("", Event::IconFetch, 1_794_268_800)
            .await
            .unwrap();
        let snap = usage.snapshot().await.unwrap();
        assert_eq!(snap["days"]["2026-11-10"]["messages"], 1);
        assert_eq!(snap["totals"]["iconFetches"], 1);
        tokio::fs::write(&path, "{").await.unwrap();
        assert!(usage.note_at_for("", Event::Message, 0).await.is_err());
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "{");
    }

    #[test]
    fn one_bots_messages_do_not_land_on_another() {
        let mut doc = json!({ "version": 1, "days": {} });
        record_for(&mut doc, "2026-11-10", "ada", "messages", 400);
        record_for(&mut doc, "2026-11-10", "ada", "attachments", 400);
        record_for(&mut doc, "2026-11-09", "milo", "messages", 400);
        assert_eq!(bot_counts(&doc, "ada"), (1, 1));
        assert_eq!(bot_counts(&doc, "milo"), (1, 0));
        assert_eq!(bot_counts(&doc, "missing"), (0, 0));
        let view = with_totals(doc);
        assert_eq!(view["totals"]["messages"], 2);
        assert_eq!(view["totals"]["attachments"], 1);
        assert!(view["totals"].get("bots").is_none());
    }

    #[test]
    fn tokens_sum_finished_turns_and_skip_what_t3_cannot_price() {
        let projection = json!({
            "providerTurns": [
                { "turnTokenUsage": { "inputTokens": 10, "outputTokens": 4, "cachedInputTokens": 3, "cacheCreationTokens": 1 } },
                { "turnTokenUsage": { "usageStatus": "unavailable", "inputTokens": 99, "outputTokens": 99 } },
                { "turnTokenUsage": { "localCommand": true, "inputTokens": 50, "outputTokens": 50 } },
                { "completedAt": "2026-11-10T00:00:00Z" }
            ]
        });
        let (totals, known) = tokens_from_projection(&projection);
        assert!(known);
        assert_eq!(totals.turns, 1);
        assert_eq!(totals.input, 10);
        assert_eq!(totals.output, 4);
        assert_eq!(totals.cached, 3);
        assert_eq!(totals.cache_write, 1);
        let (empty, seen) = tokens_from_projection(&json!({ "providerTurns": [] }));
        assert!(!seen);
        assert_eq!(empty, TokenTotals::default());
        let body = bot_payload(2, 1, &totals, true, "");
        assert_eq!(body["messages"], 2);
        assert_eq!(body["tokensKnown"], true);
        assert_eq!(body["error"], "");
    }

    #[test]
    fn a_token_line_matches_one_turn_only_when_the_ids_match() {
        let projection = json!({
            "providerTurns": [
                {
                    "id": "m2",
                    "turnTokenUsage": {
                        "inputTokens": 10,
                        "outputTokens": 2,
                        "cachedInputTokens": 4,
                        "cacheCreationTokens": 1
                    }
                },
                {
                    "id": "other",
                    "turnTokenUsage": {
                        "inputTokens": 99,
                        "outputTokens": 99,
                        "cachedInputTokens": 99,
                        "cacheCreationTokens": 99
                    }
                },
                {
                    "id": "bad",
                    "turnTokenUsage": {
                        "usageStatus": "unavailable",
                        "inputTokens": 5,
                        "outputTokens": 5,
                        "cachedInputTokens": 5,
                        "cacheCreationTokens": 5
                    }
                },
                {
                    "messageId": "local",
                    "turnTokenUsage": {
                        "localCommand": true,
                        "inputTokens": 7,
                        "outputTokens": 7,
                        "cachedInputTokens": 1,
                        "cacheCreationTokens": 1
                    }
                },
                {
                    "turnTokenUsage": {
                        "inputTokens": 3,
                        "outputTokens": 3,
                        "cachedInputTokens": 0,
                        "cacheCreationTokens": 0
                    }
                }
            ]
        });
        let line = turn_token_line(&projection, "m2").unwrap();
        assert_eq!(line["inputTokens"], 10);
        assert_eq!(line["outputTokens"], 2);
        assert_eq!(line["cachedInputTokens"], 4);
        assert_eq!(line["cacheCreationTokens"], 1);
        assert_ne!(line["inputTokens"].as_u64().unwrap(), 10 + 4);
        assert!(turn_token_line(&projection, "other").unwrap()["inputTokens"] == 99);
        assert!(turn_token_line(&projection, "no-such-id").is_none());
        assert!(turn_token_line(&projection, "bad").is_none());
        assert!(turn_token_line(&projection, "local").is_none());
        assert!(turn_token_line(&projection, "").is_none());
    }

    #[tokio::test]
    async fn a_missing_file_does_not_invent_counts() {
        let dir = tempfile::tempdir().unwrap();
        let usage = Usage::new(dir.path().join("missing.json"));
        let snap = usage.snapshot().await.unwrap();
        assert_eq!(snap["days"], json!({}));
        assert!(snap["totals"].as_object().unwrap().is_empty());
        assert!(snap["totals"].get("messages").is_none());
    }

    fn fixture_turns() -> Vec<Turn> {
        let f: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/t3/thread-projection.json"
        ))
        .unwrap();
        turns_from_projection(&f["value"])
    }

    #[test]
    fn iso_times_parse_to_milliseconds() {
        assert_eq!(parse_iso_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_ms("1970-01-02T00:00:01.250Z"), Some(86_401_250));
        assert_eq!(
            parse_iso_ms("2026-11-10T08:23:56.048Z"),
            Some(1_794_299_036_048)
        );
        assert_eq!(parse_iso_ms("nope"), None);
    }

    #[test]
    fn a_real_thread_gives_three_turns_with_a_time_and_a_model() {
        let turns = fixture_turns();
        assert_eq!(turns.len(), 3);
        assert!(turns
            .iter()
            .all(|t| t.model == "claude-opus-5-5" && t.at_ms > 0 && t.input > 0));
    }

    #[test]
    fn a_period_counts_the_persons_days_and_fills_the_empty_ones() {
        let turns = vec![
            Turn {
                at_ms: parse_iso_ms("2026-11-10T23:30:00Z").unwrap(),
                model: "a".into(),
                input: 100,
                output: 10,
                cached: 60,
                cache_write: 5,
            },
            Turn {
                at_ms: parse_iso_ms("2026-11-04T12:00:00Z").unwrap(),
                model: "b".into(),
                input: 50,
                output: 5,
                cached: 0,
                cache_write: 0,
            },
            Turn {
                at_ms: parse_iso_ms("2026-10-01T12:00:00Z").unwrap(),
                model: "a".into(),
                input: 1000,
                output: 100,
                cached: 0,
                cache_write: 0,
            },
        ];
        let now = parse_iso_ms("2026-11-10T23:45:00Z").unwrap();
        let none = std::collections::BTreeMap::new();
        // UTC: all three turns are on their own days; "today" holds one.
        let today = report(&turns, &none, period_days("today", now, 0).unwrap(), 0);
        assert_eq!(today["totals"]["turns"], 1);
        assert_eq!(today["series"].as_array().unwrap().len(), 1);
        // The week: 7 days filled, 2 turns (the October one is out).
        let week = report(&turns, &none, period_days("7d", now, 0).unwrap(), 0);
        assert_eq!(week["series"].as_array().unwrap().len(), 7);
        assert_eq!(week["totals"]["turns"], 2);
        assert_eq!(week["totals"]["tokens"], 165);
        assert_eq!(week["byModel"][0]["model"], "a");
        // Someone two hours east is already on the 11th: the 23:30 turn is their tomorrow-today.
        let east = report(&turns, &none, period_days("today", now, 120).unwrap(), 120);
        assert_eq!(east["series"][0]["day"], "2026-11-11");
        assert_eq!(east["totals"]["turns"], 1);
        // All time: only days with data, oldest first.
        let all = report(&turns, &none, period_days("all", now, 0).unwrap(), 0);
        assert_eq!(all["totals"]["turns"], 3);
        let days: Vec<&str> = all["series"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["day"].as_str().unwrap())
            .collect();
        assert_eq!(days, ["2026-10-01", "2026-11-04", "2026-11-10"]);
        assert!(period_days("century", now, 0).is_none());
    }

    #[test]
    fn messages_and_attachments_join_the_series_by_day() {
        let mut doc = json!({ "version": 1, "days": {} });
        record_for(&mut doc, "2026-11-10", "ada", "messages", 400);
        record_for(&mut doc, "2026-11-10", "ada", "messages", 400);
        record_for(&mut doc, "2026-11-10", "ada", "attachments", 400);
        record_for(&mut doc, "2026-11-10", "bob", "messages", 400);
        let ada = activity_by_day(&doc, "ada");
        let now = parse_iso_ms("2026-11-10T12:00:00Z").unwrap();
        let r = report(&[], &ada, period_days("7d", now, 0).unwrap(), 0);
        assert_eq!(
            (
                r["totals"]["messages"].as_u64(),
                r["totals"]["attachments"].as_u64()
            ),
            (Some(2), Some(1))
        );
        assert_eq!(r["series"][6]["messages"], 2);
        let machine = activity_by_day(&doc, "");
        assert_eq!(
            report(&[], &machine, period_days("today", now, 0).unwrap(), 0)["totals"]["messages"],
            3
        );
    }
}
