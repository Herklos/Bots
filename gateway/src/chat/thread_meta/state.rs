use super::*;

pub(crate) const MAX_STAMPS: usize = 200;

/// Pins stay a short list. One past this is refused.
pub(crate) const PIN_CAP: usize = 20;

// Nothing in the gateway calls this yet.
#[allow(
    dead_code,
    reason = "only the pin tests read this; the gateway does not show pins yet"
)]
pub(crate) const NOT_IN_VIEW: &str = "This message is not in view.";

pub(crate) fn bad(message: &str) -> ApiError {
    ApiError::bad_request(message)
}

pub(crate) fn checked<'a>(
    value: Option<&'a str>,
    pattern: &Regex,
    label: &str,
) -> ApiResult<&'a str> {
    value
        .filter(|v| pattern.is_match(v))
        .ok_or_else(|| bad(&format!("invalid {label}")))
}

pub(crate) fn valid_emoji(value: &str) -> bool {
    EMOJI.is_match(value) && EMOJI_LEAD.is_match(value)
}

pub(crate) fn corrupt() -> ApiError {
    ApiError::new(500, "thread-meta.json has an invalid entry")
}

/// The state document, read once through the machine-local store. A missing
/// document starts empty; an unreadable one is an error, as in hbots.
pub(crate) async fn read_state(
    store: &dyn LocalStateStore,
    doc: &str,
) -> ApiResult<Map<String, Value>> {
    match store.read(doc).await {
        Ok(None) => Ok(Map::new()),
        Ok(Some(Value::Object(map))) => Ok(map),
        Ok(Some(_)) => Err(ApiError::new(500, "thread-meta.json is not valid")),
        Err(e) => Err(e.into()),
    }
}

pub(crate) async fn loaded<'a>(
    store: &dyn LocalStateStore,
    doc: &str,
    slot: &'a mut Option<Map<String, Value>>,
) -> ApiResult<&'a mut Map<String, Value>> {
    if slot.is_none() {
        *slot = Some(read_state(store, doc).await?);
    }
    slot.as_mut()
        .ok_or_else(|| ApiError::new(500, "thread-meta.json is not loaded"))
}

pub(crate) async fn save(
    store: &dyn LocalStateStore,
    doc: &str,
    state: &Map<String, Value>,
) -> ApiResult<()> {
    store.write(doc, &Value::Object(state.clone())).await
}

/// What the client sees for one thread: its reactions and its voice calls of at least ten seconds.
/// Short failed calls are hidden on read and kept in the file.
pub(crate) fn view(state: &Map<String, Value>, thread: &str) -> Value {
    let entry = state.get(thread).and_then(Value::as_object);
    let reactions = entry
        .and_then(|e| e.get("reactions"))
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let voice_calls: Vec<Value> = entry
        .and_then(|e| e.get("voiceCalls"))
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .filter(|c| {
                    c["seconds"]
                        .as_f64()
                        .is_some_and(|s| s.is_finite() && s >= 10.0)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let reply_to = entry
        .and_then(|e| e.get("replyTo"))
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let pins = entry
        .and_then(|e| e.get("pins"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let last_read = entry
        .and_then(|e| e.get("lastRead"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let latest = entry
        .and_then(|e| e.get("latest"))
        .and_then(Value::as_str)
        .unwrap_or("");
    json!({
        "reactions": reactions,
        "voiceCalls": voice_calls,
        "replyTo": reply_to,
        "pins": pins,
        "lastRead": last_read,
        "latest": latest,
    })
}

/// Moves the mark to `latest` only while that chat is open.
/// A hidden Bot that is not the open chat stays where it was.
pub(crate) fn next_last_read(stored: &str, latest: &str, open: bool, hidden: bool) -> String {
    if hidden && !open {
        return stored.to_string();
    }
    if open && !latest.is_empty() {
        return latest.to_string();
    }
    stored.to_string()
}

/// A later message is unread until this chat is the open one.
/// A hidden Bot keeps that mark until it is opened.
pub(crate) fn is_unread(stored: &str, latest: &str, open: bool, hidden: bool) -> bool {
    !latest.is_empty() && next_last_read(stored, latest, open, hidden) != latest
}

/// The inbox mark. A hidden Bot follows the message id, so the roster's
/// timestamp rule (a hidden Bot is never unread) does not apply. A visible
/// Bot keeps the flag the roster already computed and is not notified from here.
pub(crate) fn inbox_unread(
    hidden: bool,
    roster_unread: bool,
    last_read: &str,
    latest: &str,
) -> bool {
    if hidden {
        is_unread(last_read, latest, false, true)
    } else {
        roster_unread
    }
}

/// Pins in stored order. An id that is not in the loaded messages stays, and says so.
// Nothing in the gateway calls this yet.
#[allow(
    dead_code,
    reason = "only the pin tests call this; the gateway does not show pins yet"
)]
pub(crate) fn pin_lines(pins: &[Value], messages: &[Value]) -> Vec<Value> {
    pins.iter()
        .filter_map(Value::as_str)
        .map(|id| {
            match messages
                .iter()
                .find(|message| message["id"].as_str() == Some(id))
            {
                Some(message) => json!({
                    "id": id,
                    "text": message["text"].as_str().unwrap_or(""),
                    "missing": false,
                }),
                None => json!({ "id": id, "text": NOT_IN_VIEW, "missing": true }),
            }
        })
        .collect()
}

/// The thread's entry, created when it is missing.
pub(crate) fn thread_entry<'a>(
    state: &'a mut Map<String, Value>,
    thread: &str,
) -> ApiResult<&'a mut Map<String, Value>> {
    state
        .entry(thread.to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(corrupt)
}

pub(crate) fn rounded_seconds(seconds: f64) -> i64 {
    let seconds = seconds.round();
    #[allow(
        clippy::cast_possible_truncation,
        reason = "duration is finite and within 0..=86400, so the rounded second fits in i64"
    )]
    {
        seconds as i64
    }
}

/// Validates `stamps` as `createThreadPreviews` does: an object of at most 200 thread IDs to strings.
pub(crate) fn stamp_entries(stamps: Option<&Value>) -> ApiResult<Vec<(String, String)>> {
    let Some(Value::Object(map)) = stamps else {
        return Err(bad("stamps must be an object"));
    };
    if map.len() > MAX_STAMPS {
        return Err(bad("invalid stamps"));
    }
    map.iter()
        .map(|(id, stamp)| {
            let stamp = stamp
                .as_str()
                .filter(|_| THREAD_ID.is_match(id))
                .ok_or_else(|| bad("invalid stamps"))?;
            Ok((id.clone(), stamp.to_string()))
        })
        .collect()
}
