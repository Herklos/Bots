use super::*;

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

pub(crate) fn count_u64(value: &Value) -> u64 {
    value.as_u64().unwrap_or(0)
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

/// The day a moment falls on for someone `offset_min` minutes east of UTC, as a count of days.
pub(crate) fn day_index(ms: i64, offset_min: i64) -> i64 {
    (ms + offset_min * 60_000).div_euclid(86_400_000)
}

pub(crate) fn day_name(index: i64) -> String {
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
