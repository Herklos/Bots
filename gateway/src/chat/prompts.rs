use super::*;

pub(crate) fn is_framed_prompt(text: &str) -> bool {
    is_group_prompt(text) || is_computer_prompt(text)
}

pub(crate) fn is_group_prompt(text: &str) -> bool {
    text.contains("[Group \"")
        || text.contains("Your reply is shared with the group")
        || text.contains("Write @slug only when")
}

pub(crate) fn is_computer_prompt(text: &str) -> bool {
    text.contains("[Computer]")
}

// Nothing in the gateway calls this yet.
#[allow(
    dead_code,
    reason = "only the chat tests read this; the live search path is not wired yet"
)]
pub(crate) const LOADED_SENTENCE: &str = "Search covers the loaded conversation.";

/// Filters messages already loaded from a projection. Nothing else is searched.
// Nothing in the gateway calls this yet.
#[allow(
    dead_code,
    reason = "only the chat tests call this; the live search path is not wired yet"
)]
pub(crate) fn search_messages(messages: &[Value], query: &str, partial: bool) -> Value {
    let query = query.trim().to_lowercase();
    let hits: Vec<Value> = if query.is_empty() {
        Vec::new()
    } else {
        messages
            .iter()
            .filter(|message| {
                message["text"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&query)
            })
            .cloned()
            .collect()
    };
    json!({
        "messages": hits,
        "note": if partial && !query.is_empty() { LOADED_SENTENCE } else { "" },
    })
}

/// Writes the clip as an attachment, records the duration, and builds the dispatch.
/// `voiceCalls` stays a time and a duration. The command gains no quote field.
pub(crate) async fn store_voice_send(
    root: &std::path::Path,
    slug: &str,
    threads: &crate::chat::thread_meta::ThreadMeta,
    thread: &str,
    bytes: &[u8],
    seconds: f64,
    at: &str,
) -> crate::platform::error::ApiResult<Value> {
    let meta =
        crate::chat::attachments::write_file(root, slug, "voice.m4a", "audio/mp4", bytes).await?;
    let seen = threads
        .add_voice_call(Some(thread), Some(&json!(seconds)), at)
        .await?;
    let command = crate::harness::t3::transcript::send_command(
        thread,
        "",
        &[crate::chat::attachments::command_part(&meta)],
    );
    Ok(json!({
        "attachment": meta,
        "voiceCalls": seen["voiceCalls"],
        "command": command,
    }))
}

pub(crate) const VOICE_TYPES: &[&str] = &["audio/mp4", "audio/webm", "audio/mpeg", "audio/wav"];

/// Dispatches a clip that is already stored. It does not write a second file.
/// A file that is not audio is refused. The command gains no quote field.
pub(crate) async fn dispatch_stored_voice(
    root: &std::path::Path,
    slug: &str,
    threads: &crate::chat::thread_meta::ThreadMeta,
    thread: &str,
    attachment_id: &str,
    seconds: f64,
    at: &str,
) -> crate::platform::error::ApiResult<Value> {
    let (meta, _) = crate::chat::attachments::read_file(root, slug, attachment_id).await?;
    let media = meta["mediaType"]
        .as_str()
        .unwrap_or("")
        .to_ascii_lowercase();
    if !VOICE_TYPES.contains(&media.as_str()) {
        return Err(crate::platform::error::ApiError::bad_request(
            "That file is not a voice clip.",
        ));
    }
    let seen = threads
        .add_voice_call(Some(thread), Some(&json!(seconds)), at)
        .await?;
    let command = crate::harness::t3::transcript::send_command(
        thread,
        "",
        &[crate::chat::attachments::command_part(&meta)],
    );
    Ok(json!({
        "attachment": meta,
        "voiceCalls": seen["voiceCalls"],
        "command": command,
    }))
}
