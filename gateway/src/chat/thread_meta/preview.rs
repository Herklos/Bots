use super::*;

pub(crate) static THREAD_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_:%.\-]{1,300}$").unwrap());

pub(crate) static ITEM_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_:%.\-]{1,600}$").unwrap());

// Emoji: 1 to 16 emoji code points, with at least one pictographic or regional
// indicator (so "1", "#" and "*" alone are refused).
pub(crate) static EMOJI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\p{Extended_Pictographic}\p{Regional_Indicator}\p{Emoji_Component}\x{200D}\x{FE0F}]{1,16}$").unwrap()
});

pub(crate) static EMOJI_LEAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{Extended_Pictographic}\p{Regional_Indicator}]").unwrap());

// The preview cleaning of `previewText`, applied in the same order.
pub(crate) static ULTRATHINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^Ultrathink:\s*").unwrap());

pub(crate) static REPLY_CONTEXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<reply-context>[\s\S]*?</reply-context>\s*").unwrap());

pub(crate) static IMAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[[^\]]*\]\([^)]*\)").unwrap());

pub(crate) static T3_THREAD_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*<t3-thread:[^>]*>").unwrap());

pub(crate) static FROM_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^From [^:\n]{1,80}:\s*").unwrap());

pub(crate) static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"```[^\n]*\n?").unwrap());

pub(crate) static LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());

pub(crate) static LINE_MARK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s{0,3}(?:#{1,6}|>|[-*+])\s+").unwrap());

pub(crate) static EMPHASIS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*|__|`").unwrap());

pub(crate) static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

pub(crate) static TOOL_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"__|\.").unwrap());

/// A value as `String(value ?? "")` reads it in JavaScript.
pub(crate) fn text_of(value: &Value) -> String {
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
pub(crate) fn tool_name(item: &Value) -> String {
    let raw = item["toolName"].as_str().unwrap_or("");
    TOOL_SEPARATOR.split(raw).last().unwrap_or("").to_string()
}

/// T3 sends some fields as JSON text; anything unreadable reads as an empty object.
pub(crate) fn parsed(value: &Value) -> Value {
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
pub(crate) fn with_id(mut row: Value, item: &Value) -> Value {
    if let Some(id) = item["id"].as_str().filter(|id| !id.is_empty()) {
        row["id"] = json!(id);
    }
    row
}
