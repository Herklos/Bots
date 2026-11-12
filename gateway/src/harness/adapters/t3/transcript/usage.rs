use crate::bots::usage::{count_u64, parse_iso_ms, TokenTotals, Turn};
use serde_json::{json, Value};

/// Sum `providerTurns[].turnTokenUsage`. A turn with no usage object means this
/// runtime did not report tokens. `unavailable` and local-command turns are seen, and not added.
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

/// The finished turns of a thread projection that reported tokens.
pub(crate) fn turns_from_projection(projection: &Value) -> Vec<Turn> {
    projection["providerTurns"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|turn| {
            let usage = turn.get("turnTokenUsage").filter(|u| u.is_object())?;
            if usage["usageStatus"] == "unavailable" || usage["localCommand"] == true {
                return None;
            }
            let at = turn["completedAt"]
                .as_str()
                .or(turn["startedAt"].as_str())
                .and_then(parse_iso_ms)?;
            Some(Turn {
                at_ms: at,
                model: turn["model"].as_str().unwrap_or("").to_string(),
                input: count_u64(&usage["inputTokens"]),
                output: count_u64(&usage["outputTokens"]),
                cached: count_u64(&usage["cachedInputTokens"]),
                cache_write: count_u64(&usage["cacheCreationTokens"]),
            })
        })
        .collect()
}
