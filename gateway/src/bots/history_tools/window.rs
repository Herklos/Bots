//! The page around one visible message.
//!
//! At most three messages before the match and three after. Each message is
//! cut to 1,200 characters. A turn that carries `scheduledTaskId` is a
//! `routine`, never `you`. Framed group and Computer turns are absent, because
//! the page is built from `transcript()`, which drops them.

use crate::harness::t3::transcript::{transcript, turn_items};
use serde_json::Value;
use std::collections::HashSet;

/// Messages kept on either side of the match.
const RADIUS: usize = 3;
/// Characters kept from one message.
pub(crate) const TEXT_CAP: usize = 1_200;

/// Who wrote the line, as the search result names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    You,
    Bot,
    Routine,
}

impl Role {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::You => "you",
            Self::Bot => "bot",
            Self::Routine => "routine",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PageMessage {
    pub id: String,
    pub role: Role,
    pub at: String,
    pub text: String,
}

/// `anchor` is the match's place in `messages`. It is absent when that message
/// is not visible. `truncated` means the projection did not hold the whole thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Page {
    pub messages: Vec<PageMessage>,
    pub anchor: Option<usize>,
    pub truncated: bool,
}

pub(crate) fn around(projection: &Value, message_id: &str) -> Page {
    let truncated = projection_partial(projection);
    let routines = routine_ids(projection);
    let visible = visible_messages(projection, &routines);
    let Some(at) = visible.iter().position(|message| message.id == message_id) else {
        return Page {
            messages: Vec::new(),
            anchor: None,
            truncated,
        };
    };
    let start = at.saturating_sub(RADIUS);
    let end = (at + RADIUS + 1).min(visible.len());
    let messages = visible[start..end].to_vec();
    Page {
        messages,
        anchor: Some(at - start),
        truncated,
    }
}

fn visible_messages(projection: &Value, routines: &HashSet<String>) -> Vec<PageMessage> {
    let view = transcript(projection);
    view["messages"]
        .as_array()
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| one_message(message, routines))
                .collect()
        })
        .unwrap_or_default()
}

fn one_message(message: &Value, routines: &HashSet<String>) -> Option<PageMessage> {
    let id = message["id"].as_str()?.to_string();
    let role = if routines.contains(&id) {
        Role::Routine
    } else if message["role"] == "user" {
        Role::You
    } else {
        Role::Bot
    };
    Some(PageMessage {
        id,
        role,
        at: message["at"].as_str().unwrap_or("").to_string(),
        text: cut(message["text"].as_str().unwrap_or("")),
    })
}

pub(crate) fn routine_ids(projection: &Value) -> HashSet<String> {
    turn_items(projection)
        .iter()
        .filter_map(|raw| {
            let item = if raw["item"].is_object() {
                &raw["item"]
            } else {
                raw
            };
            if item["type"] != "user_message" || !item["scheduledTaskId"].is_string() {
                return None;
            }
            item["id"].as_str().map(str::to_string)
        })
        .collect()
}

fn projection_partial(projection: &Value) -> bool {
    projection["truncated"] == true
        || projection["hasMore"] == true
        || projection["partial"] == true
}

fn cut(text: &str) -> String {
    if text.chars().count() <= TEXT_CAP {
        return text.to_string();
    }
    text.chars().take(TEXT_CAP).collect()
}
