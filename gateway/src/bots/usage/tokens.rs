use super::*;

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

pub(crate) fn shares_message(turn: &Value, message_id: &str) -> bool {
    [
        turn.get("messageId"),
        turn.get("assistantMessageId"),
        turn.get("id"),
    ]
    .into_iter()
    .any(|value| value.and_then(Value::as_str) == Some(message_id))
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
