//! Reactions, the voice-call log, and roster previews kept beside the harness conversation.
//! T3 Code is the harness wired today; its wire projection doesn't carry these. Other backends
//! may be added. Stored as `thread-meta.json` in the app directory.
use crate::{
    error::{ApiError, ApiResult},
    storage,
};
use futures_util::future::join_all;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    sync::LazyLock,
};
use tokio::sync::Mutex;

static THREAD_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_:%.\-]{1,300}$").unwrap());
static ITEM_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_:%.\-]{1,600}$").unwrap());
// Emoji: 1 to 16 emoji code points, with at least one pictographic or regional
// indicator (so "1", "#" and "*" alone are refused).
static EMOJI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\p{Extended_Pictographic}\p{Regional_Indicator}\p{Emoji_Component}\x{200D}\x{FE0F}]{1,16}$").unwrap()
});
static EMOJI_LEAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{Extended_Pictographic}\p{Regional_Indicator}]").unwrap());
// The preview cleaning of `previewText`, applied in the same order.
static ULTRATHINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^Ultrathink:\s*").unwrap());
static REPLY_CONTEXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<reply-context>[\s\S]*?</reply-context>\s*").unwrap());
static IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[[^\]]*\]\([^)]*\)").unwrap());
static T3_THREAD_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*<t3-thread:[^>]*>").unwrap());
static FROM_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^From [^:\n]{1,80}:\s*").unwrap());
static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"```[^\n]*\n?").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
static LINE_MARK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s{0,3}(?:#{1,6}|>|[-*+])\s+").unwrap());
static EMPHASIS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*|__|`").unwrap());
static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static TOOL_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"__|\.").unwrap());

const MAX_STAMPS: usize = 200;
/// Pins stay a short list. One past this is refused.
pub(crate) const PIN_CAP: usize = 20;
// Nothing in the gateway calls this yet.
#[allow(
    dead_code,
    reason = "only the pin tests read this; the gateway does not show pins yet"
)]
pub(crate) const NOT_IN_VIEW: &str = "This message is not in view.";

fn bad(message: &str) -> ApiError {
    ApiError::bad_request(message)
}

fn checked<'a>(value: Option<&'a str>, pattern: &Regex, label: &str) -> ApiResult<&'a str> {
    value
        .filter(|v| pattern.is_match(v))
        .ok_or_else(|| bad(&format!("invalid {label}")))
}

fn valid_emoji(value: &str) -> bool {
    EMOJI.is_match(value) && EMOJI_LEAD.is_match(value)
}

fn corrupt() -> ApiError {
    ApiError::new(500, "thread-meta.json has an invalid entry")
}

/// A value as `String(value ?? "")` reads it in JavaScript.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// One line of Markdown as it reads, without its syntax or T3's hidden context.
pub(crate) fn preview_text(value: &str) -> String {
    let s = ULTRATHINK.replace(value, "").into_owned();
    let s = REPLY_CONTEXT.replace(&s, "").into_owned();
    let s = IMAGE.replace_all(&s, "").into_owned();
    let s = T3_THREAD_TAG.replace_all(&s, "").into_owned();
    let s = FROM_PREFIX.replace(&s, "").into_owned();
    let s = FENCE.replace_all(&s, "").into_owned();
    let s = LINK.replace_all(&s, "$1").into_owned();
    let s = LINE_MARK.replace_all(&s, "").into_owned();
    let s = EMPHASIS.replace_all(&s, "").into_owned();
    let s = WHITESPACE.replace_all(&s, " ");
    s.trim().chars().take(200).collect()
}

/// The last segment of a tool name, after `__` or `.` (`mcp__t3__t3_thread_send` is `t3_thread_send`).
fn tool_name(item: &Value) -> String {
    let raw = item["toolName"].as_str().unwrap_or("");
    TOOL_SEPARATOR.split(raw).last().unwrap_or("").to_string()
}

/// T3 sends some fields as JSON text; anything unreadable reads as an empty object.
fn parsed(value: &Value) -> Value {
    match value {
        Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
        Value::Null => json!({}),
        other => other.clone(),
    }
}

/// The newest thing a person would recognize in this conversation, or None when it has none.
/// `names` maps a thread to its Bot's name, for "Messaged <Bot>: ..." lines.
pub(crate) fn latest_preview(projection: &Value, names: &HashMap<String, String>) -> Option<Value> {
    let rows = crate::chat::without_group_turns(
        projection["visibleTurnItems"]
            .as_array()
            .or_else(|| projection["turnItems"].as_array())
            .cloned()
            .unwrap_or_default(),
    );
    let mut items: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            if row["item"].is_object() {
                row["item"].clone()
            } else {
                row
            }
        })
        .filter(Value::is_object)
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item["ordinal"].as_i64().unwrap_or(0)));
    for item in items {
        let at = ["completedAt", "startedAt", "updatedAt"]
            .iter()
            .find_map(|key| item[*key].as_str())
            .map(String::from);
        let kind = item["type"].as_str().unwrap_or("");
        match kind {
            "user_message" | "assistant_message" => {
                let role = if kind == "user_message" {
                    "user"
                } else {
                    "assistant"
                };
                let text = preview_text(&text_of(&item["text"]));
                if !text.is_empty() {
                    return Some(with_id(
                        json!({ "kind": "text", "role": role, "text": text, "at": at }),
                        &item,
                    ));
                }
                if item["attachments"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty())
                {
                    return Some(with_id(
                        json!({ "kind": "attachment", "role": role, "text": "Attachment", "at": at }),
                        &item,
                    ));
                }
            }
            "user_input_request" => {
                let text = preview_text(&text_of(&parsed(&item["questions"])[0]["question"]));
                if !text.is_empty() {
                    return Some(with_id(
                        json!({ "kind": "question", "role": "assistant", "text": text, "at": at }),
                        &item,
                    ));
                }
            }
            "dynamic_tool"
                if item["status"] == "completed" && tool_name(&item) == "t3_thread_send" =>
            {
                let input = parsed(&item["input"]);
                let text = preview_text(&text_of(&input["message"]));
                if !text.is_empty() {
                    let name = input["threadId"]
                        .as_str()
                        .and_then(|id| names.get(id))
                        .map(String::as_str)
                        .unwrap_or("a Bot");
                    return Some(with_id(
                        json!({
                            "kind": "sent", "role": "assistant",
                            "text": format!("Messaged {name}: {text}"), "at": at
                        }),
                        &item,
                    ));
                }
            }
            _ => {}
        }
    }
    None
}

/// Keeps the message id on a preview when the item has one. An id-less item stays as it was.
fn with_id(mut row: Value, item: &Value) -> Value {
    if let Some(id) = item["id"].as_str().filter(|id| !id.is_empty()) {
        row["id"] = json!(id);
    }
    row
}

/// The state file, read once. A missing file starts empty; an unreadable one is an error, as in hbots.
async fn read_state(file: &Path) -> ApiResult<Map<String, Value>> {
    match tokio::fs::read_to_string(file).await {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => Ok(map),
            _ => Err(ApiError::new(500, "thread-meta.json is not valid")),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(e) => Err(e.into()),
    }
}

async fn loaded<'a>(
    file: &Path,
    slot: &'a mut Option<Map<String, Value>>,
) -> ApiResult<&'a mut Map<String, Value>> {
    if slot.is_none() {
        *slot = Some(read_state(file).await?);
    }
    slot.as_mut()
        .ok_or_else(|| ApiError::new(500, "thread-meta.json is not loaded"))
}

async fn save(file: &Path, state: &Map<String, Value>) -> ApiResult<()> {
    let text =
        serde_json::to_string_pretty(state).map_err(|e| ApiError::new(500, e.to_string()))?;
    storage::write_atomic(file, (text + "\n").as_bytes(), None).await?;
    Ok(())
}

/// What the client sees for one thread: its reactions and its voice calls of at least ten seconds.
/// Short failed calls are hidden on read and kept in the file.
fn view(state: &Map<String, Value>, thread: &str) -> Value {
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
fn thread_entry<'a>(
    state: &'a mut Map<String, Value>,
    thread: &str,
) -> ApiResult<&'a mut Map<String, Value>> {
    state
        .entry(thread.to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(corrupt)
}

fn rounded_seconds(seconds: f64) -> i64 {
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
fn stamp_entries(stamps: Option<&Value>) -> ApiResult<Vec<(String, String)>> {
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

#[derive(Debug)]
pub struct ThreadMeta {
    file: PathBuf,
    state: Mutex<Option<Map<String, Value>>>,
    /// Per thread: the stamp its preview was computed at, and the preview (None when it had none).
    previews: Mutex<HashMap<String, (String, Option<Value>)>>,
}

impl ThreadMeta {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            state: Mutex::new(None),
            previews: Mutex::new(HashMap::new()),
        }
    }

    pub async fn get(&self, thread: Option<&str>) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        Ok(view(state, thread))
    }

    /// Adds the reaction if it is absent, removes it if present. Returns the thread's view.
    pub async fn toggle_reaction(
        &self,
        thread: Option<&str>,
        item: Option<&str>,
        emoji: Option<&str>,
    ) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let item = checked(item, &ITEM_ID, "message")?;
        let emoji = emoji
            .filter(|e| valid_emoji(e))
            .ok_or_else(|| bad("invalid reaction"))?;
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        let entry = thread_entry(state, thread)?;
        let reactions = entry
            .entry("reactions")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(corrupt)?;
        let current: Vec<String> = reactions
            .get(item)
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let next: Vec<String> = if current.iter().any(|e| e == emoji) {
            current.into_iter().filter(|e| e != emoji).collect()
        } else {
            current.into_iter().chain([emoji.to_string()]).collect()
        };
        if next.is_empty() {
            reactions.remove(item);
        } else {
            reactions.insert(item.to_string(), json!(next));
        }
        save(&self.file, state).await?;
        Ok(view(state, thread))
    }

    /// Records a call of `seconds` (0 to 86400), rounded, at the time `at` (an ISO stamp).
    pub async fn add_voice_call(
        &self,
        thread: Option<&str>,
        seconds: Option<&Value>,
        at: &str,
    ) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let seconds = seconds
            .and_then(Value::as_f64)
            .filter(|s| s.is_finite() && (0.0..=86_400.0).contains(s))
            .ok_or_else(|| bad("invalid duration"))?;
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        let entry = thread_entry(state, thread)?;
        entry
            .entry("voiceCalls")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(corrupt)?
            .push(json!({ "at": at, "seconds": rounded_seconds(seconds) }));
        save(&self.file, state).await?;
        Ok(view(state, thread))
    }

    /// Adds the message id if it is absent, removes it if present. Past [`PIN_CAP`] is refused.
    pub async fn toggle_pin(&self, thread: Option<&str>, item: Option<&str>) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let item = checked(item, &ITEM_ID, "message")?;
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        let entry = thread_entry(state, thread)?;
        let mut pins: Vec<String> = entry
            .get("pins")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(index) = pins.iter().position(|id| id == item) {
            pins.remove(index);
        } else if pins.len() >= PIN_CAP {
            return Err(bad("That conversation already has 20 pins."));
        } else {
            pins.push(item.to_string());
        }
        entry.insert("pins".into(), json!(pins));
        save(&self.file, state).await?;
        Ok(view(state, thread))
    }

    /// Stores `replyTo` under the message id the gateway already assigned.
    pub async fn set_reply(
        &self,
        thread: Option<&str>,
        message_id: Option<&str>,
        reply_to: Option<&str>,
        quote: Option<&str>,
    ) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let message_id = checked(message_id, &ITEM_ID, "message")?;
        let reply_to = checked(reply_to, &ITEM_ID, "message")?;
        let quote: String = quote.unwrap_or("").chars().take(500).collect();
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        let entry = thread_entry(state, thread)?;
        let replies = entry
            .entry("replyTo")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(corrupt)?;
        replies.insert(
            message_id.to_string(),
            json!({ "id": reply_to, "text": quote }),
        );
        save(&self.file, state).await?;
        Ok(view(state, thread))
    }

    /// Remembers the newest message id, and advances last-read only while the chat is open.
    pub async fn mark_read(
        &self,
        thread: Option<&str>,
        latest: Option<&str>,
        open: bool,
        hidden: bool,
    ) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let latest = match latest {
            Some(id) if !id.is_empty() => checked(Some(id), &ITEM_ID, "message")?,
            _ => "",
        };
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        let entry = thread_entry(state, thread)?;
        let stored = entry
            .get("lastRead")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let known = if latest.is_empty() {
            entry
                .get("latest")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        } else {
            latest.to_string()
        };
        if !known.is_empty() {
            entry.insert("latest".into(), json!(known));
        }
        let next = next_last_read(&stored, &known, open, hidden);
        entry.insert("lastRead".into(), json!(next));
        save(&self.file, state).await?;
        let mut seen = view(state, thread);
        seen["unread"] = json!(is_unread(&stored, &known, open, hidden));
        Ok(seen)
    }

    /// The threads whose preview must be computed again: new, or with a moved stamp.
    pub async fn stale(&self, stamps: Option<&Value>) -> ApiResult<Vec<(String, String)>> {
        let entries = stamp_entries(stamps)?;
        let cache = self.previews.lock().await;
        Ok(entries
            .into_iter()
            .filter(|(id, stamp)| cache.get(id).is_none_or(|(s, _)| s != stamp))
            .collect())
    }

    /// Keeps a computed preview. A failed load is never recorded, so the earlier preview stays.
    pub async fn remember(&self, thread: String, stamp: String, preview: Option<Value>) {
        self.previews.lock().await.insert(thread, (stamp, preview));
    }

    /// The previews for `stamps` whose stamp is current and that have one.
    pub async fn current(&self, stamps: Option<&Value>) -> ApiResult<Value> {
        let entries = stamp_entries(stamps)?;
        let cache = self.previews.lock().await;
        let mut out = Map::new();
        for (id, stamp) in entries {
            if let Some((s, Some(preview))) = cache.get(&id) {
                if *s == stamp {
                    out.insert(id, preview.clone());
                }
            }
        }
        Ok(Value::Object(out))
    }

    /// Recomputes the stale previews with `fetch` (at most four loads at a time), then returns them all.
    /// A thread whose projection fails keeps its earlier preview and is retried on the next call.
    pub async fn previews<F, Fut>(
        &self,
        stamps: Option<&Value>,
        names: &HashMap<String, String>,
        fetch: F,
    ) -> ApiResult<Value>
    where
        F: Fn(String) -> Fut,
        Fut: Future<Output = anyhow::Result<Value>>,
    {
        for chunk in self.stale(stamps).await?.chunks(4) {
            let loads = chunk.iter().map(|(id, stamp)| {
                let load = fetch(id.clone());
                async move { (id, stamp, load.await) }
            });
            for (id, stamp, result) in join_all(loads).await {
                if let Ok(projection) = result {
                    self.remember(
                        id.clone(),
                        stamp.clone(),
                        latest_preview(&projection, names),
                    )
                    .await;
                }
            }
        }
        self.current(stamps).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn message(ordinal: i64, kind: &str, extra: Value) -> Value {
        let mut item = json!({
            "ordinal": ordinal,
            "type": kind,
            "startedAt": format!("2026-10-30T10:00:0{ordinal}.000Z"),
        });
        for (key, value) in extra.as_object().cloned().unwrap_or_default() {
            item[key] = value;
        }
        item
    }

    fn peers() -> HashMap<String, String> {
        HashMap::from([("peer".to_string(), "Peer".to_string())])
    }

    fn scratch(name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(name);
        (dir, file)
    }

    #[test]
    fn preview_text_reads_as_prose() {
        assert_eq!(
            preview_text("<reply-context>\nOld\n</reply-context>\n\n**Ship** the [report](https://x.test) `now`"),
            "Ship the report now"
        );
        assert_eq!(
            preview_text("![a.csv](t3-context://1)\n# Title\n- item"),
            "Title item"
        );
        assert_eq!(preview_text("Ultrathink: go"), "go");
        assert_eq!(
            preview_text("Sent it to <t3-thread://v2/env/thread>."),
            "Sent it to."
        );
    }

    #[test]
    fn the_newest_recognizable_item_wins() {
        let preview = |items: Vec<Value>| latest_preview(&json!({ "turnItems": items }), &peers());
        assert_eq!(
            preview(vec![
                message(1, "user_message", json!({ "text": "hi" })),
                message(2, "reasoning", json!({ "text": "hidden" })),
            ]),
            Some(
                json!({ "kind": "text", "role": "user", "text": "hi", "at": "2026-10-30T10:00:01.000Z" })
            )
        );
        assert_eq!(
            preview(vec![message(
                1,
                "assistant_message",
                json!({ "text": "", "attachments": [{}] })
            )])
            .unwrap()["text"],
            "Attachment"
        );
        assert_eq!(
            preview(vec![message(
                1,
                "user_input_request",
                json!({ "questions": [{ "question": "Which one?" }] })
            )])
            .unwrap()["kind"],
            "question"
        );
        assert_eq!(
            preview(vec![message(
                1,
                "dynamic_tool",
                json!({
                    "status": "completed",
                    "toolName": "mcp__t3__t3_thread_send",
                    "input": json!({ "threadId": "peer", "message": "Done" }).to_string(),
                })
            )])
            .unwrap()["text"],
            "Messaged Peer: Done"
        );
        assert_eq!(
            preview(vec![message(
                1,
                "dynamic_tool",
                json!({ "status": "failed", "toolName": "t3_thread_send", "input": "{}" })
            )]),
            None
        );
        assert_eq!(
            latest_preview(
                &json!({ "visibleTurnItems": [{ "item": message(1, "assistant_message", json!({ "text": "wrapped" })) }] }),
                &HashMap::new()
            )
            .unwrap()["text"],
            "wrapped"
        );
    }

    #[tokio::test]
    async fn previews_reload_only_moved_threads_and_never_pass_on_a_stale_stamp() {
        let meta = ThreadMeta::new(scratch("unused.json").1);
        let calls = AtomicUsize::new(0);
        let failing = AtomicBool::new(false);
        let fetch = |_: String| {
            let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
            let fails = failing.load(Ordering::SeqCst);
            async move {
                if fails {
                    anyhow::bail!("offline");
                }
                Ok(
                    json!({ "turnItems": [message(i64::try_from(n).unwrap_or(i64::MAX), "assistant_message", json!({ "text": format!("reply {n}") }))] }),
                )
            }
        };
        let names = HashMap::new();
        let stamps = |pairs: &[(&str, &str)]| {
            Value::Object(
                pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), json!(v)))
                    .collect(),
            )
        };

        assert_eq!(
            meta.previews(Some(&stamps(&[("a", "1")])), &names, fetch)
                .await
                .unwrap()["a"]["text"],
            "reply 1"
        );
        assert_eq!(
            meta.previews(Some(&stamps(&[("a", "1")])), &names, fetch)
                .await
                .unwrap()["a"]["text"],
            "reply 1"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        failing.store(true, Ordering::SeqCst);
        assert!(meta
            .previews(Some(&stamps(&[("a", "2")])), &names, fetch)
            .await
            .unwrap()
            .get("a")
            .is_none());
        assert_eq!(
            meta.previews(Some(&stamps(&[("a", "1")])), &names, fetch)
                .await
                .unwrap()["a"]["text"],
            "reply 1"
        );

        failing.store(false, Ordering::SeqCst);
        assert_eq!(
            meta.previews(Some(&stamps(&[("a", "2")])), &names, fetch)
                .await
                .unwrap()["a"]["text"],
            "reply 3"
        );

        for bad_input in [json!({ "../bad": "1" }), json!([])] {
            let err = meta.stale(Some(&bad_input)).await.unwrap_err();
            assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
        }
        assert_eq!(
            meta.stale(None).await.unwrap_err().status,
            axum::http::StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn reactions_toggle_per_emoji_persist_and_reject_anything_but_emoji() {
        let (_dir, file) = scratch("meta.json");
        let meta = ThreadMeta::new(file.clone());
        meta.toggle_reaction(Some("t"), Some("m"), Some("🔥"))
            .await
            .unwrap();
        assert_eq!(
            meta.toggle_reaction(Some("t"), Some("m"), Some("👍🏽"))
                .await
                .unwrap()["reactions"]["m"],
            json!(["🔥", "👍🏽"])
        );
        assert_eq!(
            meta.toggle_reaction(Some("t"), Some("m"), Some("🔥"))
                .await
                .unwrap()["reactions"]["m"],
            json!(["👍🏽"])
        );
        assert_eq!(
            ThreadMeta::new(file.clone()).get(Some("t")).await.unwrap()["reactions"],
            json!({ "m": ["👍🏽"] })
        );
        assert_eq!(
            meta.toggle_reaction(Some("t"), Some("m"), Some("👍🏽"))
                .await
                .unwrap()["reactions"],
            json!({})
        );
        for emoji in ["<b>", "hello", "1", "#", ""] {
            let err = meta
                .toggle_reaction(Some("t"), Some("m"), Some(emoji))
                .await
                .unwrap_err();
            assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST, "{emoji:?}");
        }
        assert!(std::fs::read_to_string(&file).unwrap().contains("\"t\""));
    }

    #[tokio::test]
    async fn voice_calls_record_a_bounded_rounded_duration() {
        let (_dir, file) = scratch("meta.json");
        let meta = ThreadMeta::new(file);
        assert_eq!(
            meta.add_voice_call(Some("t"), Some(&json!(35.4)), "2026-10-30T21:46:00.000Z")
                .await
                .unwrap()["voiceCalls"],
            json!([{ "at": "2026-10-30T21:46:00.000Z", "seconds": 35 }])
        );
        for bad_seconds in [json!(-1), json!("12"), json!(86_401)] {
            let err = meta
                .add_voice_call(Some("t"), Some(&bad_seconds), "x")
                .await
                .unwrap_err();
            assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
        }
    }

    #[tokio::test]
    async fn failed_short_voice_calls_are_hidden_without_rewriting_the_file() {
        let (_dir, file) = scratch("meta.json");
        let saved = json!({ "t": { "reactions": {}, "voiceCalls": [
            { "at": "old", "seconds": 0 }, { "at": "short", "seconds": 9 }, { "at": "valid", "seconds": 10 }
        ] } })
        .to_string();
        std::fs::write(&file, &saved).unwrap();
        assert_eq!(
            ThreadMeta::new(file.clone()).get(Some("t")).await.unwrap()["voiceCalls"],
            json!([{ "at": "valid", "seconds": 10 }])
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
    }

    #[tokio::test]
    async fn invalid_thread_and_item_ids_are_refused() {
        let meta = ThreadMeta::new(scratch("meta.json").1);
        assert_eq!(
            meta.get(None).await.unwrap_err().status,
            axum::http::StatusCode::BAD_REQUEST
        );
        assert_eq!(
            meta.get(Some("../etc")).await.unwrap_err().status,
            axum::http::StatusCode::BAD_REQUEST
        );
        assert_eq!(
            meta.toggle_reaction(Some("t"), Some("bad item"), Some("🔥"))
                .await
                .unwrap_err()
                .status,
            axum::http::StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn toggling_one_reaction_twice_leaves_no_mark() {
        let meta = ThreadMeta::new(scratch("meta.json").1);
        meta.toggle_reaction(Some("t"), Some("m"), Some("🔥"))
            .await
            .unwrap();
        let cleared = meta
            .toggle_reaction(Some("t"), Some("m"), Some("🔥"))
            .await
            .unwrap();
        assert!(cleared["reactions"].get("m").is_none());
        assert_eq!(cleared["reactions"], json!({}));
    }

    #[tokio::test]
    async fn reply_to_round_trips_under_the_gateway_message_id() {
        let meta = ThreadMeta::new(scratch("meta.json").1);
        let command = crate::chat::send_command("thread-1", "thanks", &[]);
        assert!(command.get("quote").is_none());
        let message_id = command["messageId"].as_str().unwrap();
        assert_eq!(message_id, command["commandId"].as_str().unwrap());
        let seen = meta
            .set_reply(
                Some("thread-1"),
                Some(message_id),
                Some("m-old"),
                Some("the quoted line"),
            )
            .await
            .unwrap();
        assert_eq!(seen["replyTo"][message_id]["id"], "m-old");
        assert_eq!(seen["replyTo"][message_id]["text"], "the quoted line");
        let again = meta.get(Some("thread-1")).await.unwrap();
        assert_eq!(again["replyTo"][message_id]["text"], "the quoted line");
    }

    #[tokio::test]
    async fn pins_toggle_refuse_one_past_the_cap_and_keep_a_missing_id() {
        let (_dir, file) = scratch("meta.json");
        let meta = ThreadMeta::new(file.clone());
        for n in 0..PIN_CAP {
            meta.toggle_pin(Some("t"), Some(&format!("m{n}")))
                .await
                .unwrap();
        }
        let err = meta
            .toggle_pin(Some("t"), Some("overflow"))
            .await
            .unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
        let saved = std::fs::read_to_string(&file).unwrap();
        assert!(!saved.contains("overflow"));
        let removed = meta.toggle_pin(Some("t"), Some("m0")).await.unwrap();
        let pins = removed["pins"].as_array().unwrap();
        assert_eq!(pins.len(), PIN_CAP - 1);
        assert!(pins.iter().all(|id| id != "m0"));
        meta.toggle_pin(Some("t"), Some("m0")).await.unwrap();
        let seen = meta.get(Some("t")).await.unwrap();
        let lines = pin_lines(
            seen["pins"].as_array().unwrap(),
            &[json!({ "id": "m1", "text": "Hello" })],
        );
        assert!(lines
            .iter()
            .any(|line| line["id"] == "m1" && line["missing"] == false));
        assert!(lines.iter().any(|line| {
            line["id"] == "m2" && line["missing"] == true && line["text"] == NOT_IN_VIEW
        }));
    }

    #[tokio::test]
    async fn last_read_advances_while_open_and_a_hidden_bot_keeps_its_mark() {
        let (_dir, file) = scratch("meta.json");
        let meta = ThreadMeta::new(file.clone());
        let open = meta
            .mark_read(Some("t"), Some("m2"), true, false)
            .await
            .unwrap();
        assert_eq!(open["lastRead"], "m2");
        assert_eq!(open["unread"], false);
        assert_eq!(next_last_read("m1", "m2", true, false), "m2");

        let hidden = meta
            .mark_read(Some("h"), Some("m1"), false, true)
            .await
            .unwrap();
        assert_eq!(hidden["lastRead"], "");
        let later = meta
            .mark_read(Some("h"), Some("m2"), false, true)
            .await
            .unwrap();
        assert_eq!(later["lastRead"], "");
        assert_eq!(later["latest"], "m2");
        assert_eq!(later["unread"], true);
        assert_eq!(next_last_read("", "m2", false, true), "");
        assert!(is_unread("", "m2", false, true));
        assert!(!is_unread("m1", "m2", true, false));
        assert!(is_unread("m1", "m2", false, false));

        let opened = ThreadMeta::new(file)
            .mark_read(Some("h"), None, true, true)
            .await
            .unwrap();
        assert_eq!(opened["lastRead"], "m2");
        assert_eq!(opened["unread"], false);
    }

    #[test]
    fn a_hidden_bot_with_a_later_message_id_is_unread_until_that_chat_is_open() {
        assert!(inbox_unread(true, false, "", "m2"));
        assert!(is_unread("", "m2", false, true));
        assert!(!inbox_unread(true, false, "m2", "m2"));
        assert!(!is_unread("m2", "m2", true, true));
        assert!(!inbox_unread(false, false, "", "m2"));
        assert!(inbox_unread(false, true, "m2", "m2"));
        let item = message(
            1,
            "assistant_message",
            json!({ "id": "m2", "text": "Done" }),
        );
        let preview = latest_preview(&json!({ "turnItems": [item] }), &HashMap::new()).unwrap();
        assert_eq!(preview["id"], "m2");
        assert_eq!(preview["text"], "Done");
    }
}
