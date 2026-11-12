//! A side chat is a hidden fork of the Bot's main thread. It is not stored on the Bot
//! and it is not launched by the provisioner.

use crate::harness::{ForkPoint, ForkRequest, MergeRequest, ThreadSnapshot, UserMessage};
use crate::platform::error::{ApiError, ApiResult};
use crate::AppState;
use serde_json::{json, Value};

pub(crate) const BOTH_LANES: &str = "Send a child or a side chat, not both.";
pub(crate) const NOT_A_SIDE: &str = "That side chat is not on this Bot.";
pub(crate) const NEED_MESSAGE: &str = "Side chats start after the first message.";
pub(crate) const NOTHING_TO_MERGE: &str = "This side chat has nothing to bring back yet.";

pub(crate) fn lane_ids<'a>(
    child: Option<&'a str>,
    side: Option<&'a str>,
) -> Result<(Option<&'a str>, Option<&'a str>), ApiError> {
    let child = child.map(str::trim).filter(|id| !id.is_empty());
    let side = side.map(str::trim).filter(|id| !id.is_empty());
    if child.is_some() && side.is_some() {
        return Err(ApiError::bad_request(BOTH_LANES));
    }
    Ok((child, side))
}

pub(crate) async fn require_side(
    harness: &dyn crate::harness::Harness,
    parent: &str,
    id: &str,
) -> ApiResult<String> {
    let chats = harness
        .forks()
        .map_err(|err| err.api())?
        .list_side_chats(parent)
        .await
        .map_err(|err| err.api())?;
    if chats.iter().any(|chat| chat.id == id) {
        Ok(id.to_string())
    } else {
        Err(ApiError::bad_request(NOT_A_SIDE))
    }
}

pub(crate) fn has_user_message(snapshot: &ThreadSnapshot) -> bool {
    snapshot.items().iter().any(owner_message)
}

fn owner_message(raw: &Value) -> bool {
    let item = if raw.get("item").is_some_and(Value::is_object) {
        &raw["item"]
    } else {
        raw
    };
    if item["type"] != "user_message" {
        return false;
    }
    if item
        .get("scheduledTaskId")
        .is_some_and(|value| !value.is_null())
    {
        return false;
    }
    if item["senderThreadId"]
        .as_str()
        .is_some_and(|sender| !sender.trim().is_empty())
    {
        return false;
    }
    if item["createdBy"]
        .as_str()
        .is_some_and(|created_by| created_by != "user")
    {
        return false;
    }
    if item["creationSource"]
        .as_str()
        .is_some_and(|source| source == "server")
    {
        return false;
    }
    let text = item["text"].as_str().unwrap_or("").trim();
    if crate::chat::is_framed_prompt(text) {
        return false;
    }
    let files = item["attachments"]
        .as_array()
        .is_some_and(|list| !list.is_empty());
    !text.is_empty() || files
}

pub(crate) async fn list(state: &AppState, slug: &str) -> ApiResult<Value> {
    let bot = state
        .bots
        .get(slug)
        .await?
        .ok_or_else(|| ApiError::not_found("Bot not found"))?;
    let Some(parent) = bot["mainThreadId"].as_str().filter(|id| !id.is_empty()) else {
        return Ok(json!({ "chats": [] }));
    };
    let chats = state
        .harness
        .forks()
        .map_err(|err| err.api())?
        .list_side_chats(parent)
        .await
        .map_err(|err| err.api())?;
    Ok(json!({
        "chats": chats
            .into_iter()
            .map(|chat| json!({ "id": chat.id, "title": chat.title }))
            .collect::<Vec<_>>(),
    }))
}

pub(crate) async fn open(state: &AppState, slug: &str, text: &str) -> ApiResult<Value> {
    let (bot, parent) = parent_thread(state, slug).await?;
    let snapshot = state
        .harness
        .transcript()
        .map_err(|err| err.api())?
        .read(&parent)
        .await
        .map_err(|err| err.api())?;
    if !has_user_message(&snapshot) {
        return Err(ApiError::bad_request(NEED_MESSAGE));
    }
    let target = uuid::Uuid::new_v4().to_string();
    let settings = state.settings().await;
    let mode = snapshot.runtime_mode().unwrap_or_else(|| {
        crate::bots::provisioner::launch_runtime_mode(&bot, &settings).to_string()
    });
    state
        .harness
        .forks()
        .map_err(|err| err.api())?
        .fork(&ForkRequest {
            source_thread_id: parent,
            target_thread_id: target.clone(),
            title: "Side chat".into(),
            side_chat: true,
            point: ForkPoint::LatestStable,
            runtime_mode: Some(mode),
        })
        .await
        .map_err(|err| err.api())?;
    let text = text.trim();
    if text.is_empty() {
        return Ok(json!({ "ok": true, "threadId": target, "messageId": "" }));
    }
    let message_id = crate::harness::message_id(None);
    if let Err(err) = state
        .harness
        .turns()
        .map_err(|err| err.api())?
        .send_user(&UserMessage::new(&target, &message_id, text, &[]))
        .await
    {
        let _ = state
            .harness
            .forks()
            .map_err(|err| err.api())?
            .delete_if_unused(&target)
            .await;
        return Err(err.api());
    }
    Ok(json!({ "ok": true, "threadId": target, "messageId": message_id }))
}

pub(crate) async fn merge(state: &AppState, slug: &str, thread_id: &str) -> ApiResult<Value> {
    let thread_id = thread_id.trim();
    if thread_id.is_empty() {
        return Err(ApiError::bad_request(NOT_A_SIDE));
    }
    let (_bot, parent) = parent_thread(state, slug).await?;
    require_side(state.harness.as_ref(), &parent, thread_id).await?;
    let snapshot = state
        .harness
        .transcript()
        .map_err(|err| err.api())?
        .read(thread_id)
        .await
        .map_err(|err| err.api())?;
    let Some(run_id) = snapshot.latest_run_id() else {
        return Err(ApiError::new(409, NOTHING_TO_MERGE));
    };
    state
        .harness
        .forks()
        .map_err(|err| err.api())?
        .merge_back(&MergeRequest {
            source_thread_id: thread_id.to_string(),
            target_thread_id: parent,
            run_id,
        })
        .await
        .map_err(|err| err.api())?;
    Ok(json!({ "ok": true }))
}

async fn parent_thread(state: &AppState, slug: &str) -> ApiResult<(Value, String)> {
    let bot = state
        .bots
        .get(slug)
        .await?
        .ok_or_else(|| ApiError::not_found("Bot not found"))?;
    let parent = bot["mainThreadId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ApiError::new(409, "This Bot has no conversation yet"))?
        .to_string();
    Ok((bot, parent))
}
