//! A local room of Bots. This is not a T3 channel and not the Working on list.
//! The file is `groups.json` in the app directory: an id, a title, a description, optional
//! colour and shape (kept for older rooms; the app shows stacked member faces), member slugs,
//! and the messages stored on this machine. `lastSeenAt` is when the person last opened the
//! room. `notifications` is the mute switch: missing means on. Mute does not clear unread.
//!
//! A send is stored here, then dispatched to the members it wakes: everyone named with `@`,
//! `@everyone` or `@all` for the whole group, a single plain name, otherwise the room's answer
//! policy. `@team` and `@group` are not whole-group wake words. A member whose slug or name is
//! `everyone` or `all` is that member. `last` (the
//! default, and what a room without the field does) wakes whoever spoke last, or everyone while
//! no Bot has spoken. `everyone` wakes every member. `chief` wakes only the chief, and that
//! dispatch carries the chief line. Each dispatch remembers the message id it created, its
//! anchor. A sync reads each member's group thread and copies the replies after that anchor into
//! the room. A reply that @-mentions another member can wake that member within the hop budget
//! ([`max_hops`]: one for `last` and `everyone`, one less than the member count for `chief`). A
//! courtesy reply never wakes anyone. `answerPolicy` and `chief` are optional. Missing fields are
//! not filled in on disk.

use crate::error::{ApiError, ApiResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::Mutex;

/// Bot-to-Bot wake-ups one message from the person can set off. One handoff is enough: a Bot
/// can ask another once, and that answer lands in the room without starting a ping-pong.
pub(crate) const MAX_HOPS: u64 = 1;

/// Who answers a group message that does not name anyone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerPolicy {
    Last,
    Everyone,
    Chief,
}

impl AnswerPolicy {
    pub(crate) fn parse(raw: Option<&str>) -> Option<Self> {
        match raw {
            Some("last") => Some(Self::Last),
            Some("everyone") => Some(Self::Everyone),
            Some("chief") => Some(Self::Chief),
            Some(_) | None => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Last => "last",
            Self::Everyone => "everyone",
            Self::Chief => "chief",
        }
    }
}

/// Stored room policy, or the settings default when the field is missing or `"default"`.
/// An unrecognised stored value behaves as `last`.
pub(crate) fn resolve_policy(room: &Value, default: AnswerPolicy) -> AnswerPolicy {
    match room["answerPolicy"].as_str() {
        None | Some("default") => default,
        Some(stored) => AnswerPolicy::parse(Some(stored)).unwrap_or(AnswerPolicy::Last),
    }
}

/// The chief slug when it is still a member, otherwise the first member.
pub(crate) fn effective_chief(room: &Value, members: &[Member]) -> Option<String> {
    let stored = room["chief"].as_str().unwrap_or("");
    if !stored.is_empty() && members.iter().any(|member| member.slug == stored) {
        return Some(stored.to_string());
    }
    members.first().map(|member| member.slug.clone())
}

/// Bot-to-Bot wake-ups one message from the person can still set off.
pub(crate) fn max_hops(policy: AnswerPolicy, members_len: usize) -> u64 {
    match policy {
        AnswerPolicy::Last | AnswerPolicy::Everyone => MAX_HOPS,
        AnswerPolicy::Chief => u64::try_from(members_len.saturating_sub(1)).unwrap_or(0),
    }
}

#[derive(Clone, Debug)]
pub struct Member {
    pub slug: String,
    pub title: String,
    /// The member's thread for this group, empty until the first send creates it.
    pub thread_id: String,
    pub working: bool,
    /// The Bot is set up (it has a main thread), so a group thread can be made for it.
    pub provisioned: bool,
}

/// One message to send into a member's main thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dispatch {
    pub slug: String,
    pub thread_id: String,
    /// The T3 message id the dispatch creates; replies in the thread after it belong to the room.
    pub message_id: String,
    pub text: String,
}

#[derive(Debug)]
pub struct Rooms {
    path: PathBuf,
    lock: Mutex<()>,
}

impl Rooms {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    /// Rooms on disk. A room that predates `lastSeenAt` is marked seen now, once, so an old
    /// transcript does not start out unread.
    pub async fn list(&self) -> ApiResult<Vec<Value>> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let mut stamped = false;
        let now = crate::vault::now_iso();
        if let Some(rooms) = doc.get_mut("rooms").and_then(Value::as_array_mut) {
            for room in rooms {
                if room["lastSeenAt"].as_str().is_none() {
                    room["lastSeenAt"] = json!(now);
                    stamped = true;
                }
            }
        }
        if stamped {
            write_doc(&self.path, &doc).await?;
        }
        Ok(rooms_of(&doc))
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "answerPolicy and chief are optional fields on the same create call"
    )]
    pub async fn create(
        &self,
        title: &str,
        description: &str,
        color: &str,
        shape: &str,
        slugs: &[String],
        bots: &[Value],
        answer_policy: Option<&str>,
        chief: Option<&str>,
    ) -> ApiResult<Value> {
        let title = title.trim();
        if title.is_empty() {
            return Err(ApiError::bad_request("Name the group."));
        }
        let slugs = distinct(slugs);
        if slugs.len() < 2 {
            return Err(ApiError::bad_request("A group needs two Bots."));
        }
        for slug in &slugs {
            if !crate::bots::valid_slug(slug) {
                return Err(ApiError::bad_request("Pick Bots that already exist."));
            }
        }
        // `bots` is the list `Bots::list` already loaded. A subagent is not a second store.
        refuse_subagent(&slugs, bots)?;
        let answer_policy = answer_policy
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(policy) = answer_policy {
            if !matches!(policy, "default" | "last" | "everyone" | "chief") {
                return Err(ApiError::bad_request(
                    "answerPolicy must be default, last, everyone, or chief",
                ));
            }
        }
        let chief = chief.map(str::trim).filter(|value| !value.is_empty());
        if let Some(slug) = chief {
            if !slugs.iter().any(|member| member == slug) {
                return Err(ApiError::bad_request("chief must be a member"));
            }
        }
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let mut room = json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "title": title,
            "description": description.trim(),
            "color": clean_color(color),
            "shape": crate::bots::shape_name(shape).unwrap_or_else(|| "ball".into()),
            "members": slugs,
            "messages": [],
            "anchors": {},
            "threads": {},
            "hops": 0,
            "notifications": true,
        });
        if let Some(policy) = answer_policy {
            room["answerPolicy"] = json!(policy);
        }
        if let Some(slug) = chief {
            room["chief"] = json!(slug);
        }
        push_room(&mut doc, room.clone())?;
        write_doc(&self.path, &doc).await?;
        Ok(room)
    }

    /// Changes the fields present in `patch`. Messages stay.
    /// `bots` is the list `Bots::list` already loaded, used when `members` is replaced.
    pub async fn update(&self, room_id: &str, patch: &Value, bots: &[Value]) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = room_mut(&mut doc, room_id)?;
        let replacement = if let Some(members) = patch.get("members") {
            let list: Vec<String> = members
                .as_array()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let list = distinct(&list);
            if list.len() < 2 {
                return Err(ApiError::bad_request("A group needs two Bots."));
            }
            if list.iter().any(|slug| !crate::bots::valid_slug(slug)) {
                return Err(ApiError::bad_request("Pick Bots that already exist."));
            }
            // A list that includes a subagent is refused before the file is written.
            refuse_subagent(&list, bots)?;
            Some(list)
        } else {
            None
        };
        if let Some(title) = patch.get("title") {
            let title = title.as_str().unwrap_or("").trim();
            if title.is_empty() {
                return Err(ApiError::bad_request("Name the group."));
            }
            room["title"] = json!(title);
        }
        if let Some(description) = patch.get("description") {
            room["description"] = json!(description.as_str().unwrap_or("").trim());
        }
        if let Some(color) = patch.get("color") {
            room["color"] = json!(clean_color(color.as_str().unwrap_or("")));
        }
        if let Some(shape) = patch.get("shape") {
            room["shape"] = json!(crate::bots::shape_name(shape.as_str().unwrap_or(""))
                .unwrap_or_else(|| "ball".into()));
        }
        if let Some(list) = replacement {
            room["members"] = json!(list);
            let chief = room["chief"].as_str().unwrap_or("");
            if !chief.is_empty() && !list.iter().any(|slug| slug == chief) {
                room.as_object_mut().map(|object| object.remove("chief"));
            }
        }
        if let Some(value) = patch.get("answerPolicy") {
            let policy = value.as_str().unwrap_or("");
            if !matches!(policy, "default" | "last" | "everyone" | "chief") {
                return Err(ApiError::bad_request(
                    "answerPolicy must be default, last, everyone, or chief",
                ));
            }
            room["answerPolicy"] = json!(policy);
        }
        if let Some(value) = patch.get("chief") {
            let chief = value.as_str().unwrap_or("").trim();
            if chief.is_empty() {
                room.as_object_mut().map(|object| object.remove("chief"));
            } else if member_slugs(room).iter().any(|slug| slug == chief) {
                room["chief"] = json!(chief);
            } else {
                return Err(ApiError::bad_request("chief must be a member"));
            }
        }
        if let Some(value) = patch.get("notifications") {
            let on = value
                .as_bool()
                .ok_or_else(|| ApiError::bad_request("notifications must be a boolean"))?;
            room["notifications"] = json!(on);
        }
        let stored = room.clone();
        write_doc(&self.path, &doc).await?;
        Ok(stored)
    }

    /// Remembers the thread made for one member of the group.
    pub async fn set_thread(&self, room_id: &str, slug: &str, thread: &str) -> ApiResult<()> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = room_mut(&mut doc, room_id)?;
        if !room["threads"].is_object() {
            room["threads"] = json!({});
        }
        room["threads"][slug] = json!(thread);
        write_doc(&self.path, &doc).await
    }

    /// Nothing stored, or the Bot's one-to-one thread, is not a thread for this group.
    pub fn group_thread_missing(stored: &str, main_thread_id: &str) -> bool {
        stored.is_empty() || stored == main_thread_id
    }

    /// Marks the room seen now. A later Bot message is unread again.
    pub async fn seen(&self, room_id: &str) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = room_mut(&mut doc, room_id)?;
        room["lastSeenAt"] = json!(crate::vault::now_iso());
        let stored = room.clone();
        write_doc(&self.path, &doc).await?;
        Ok(stored)
    }

    pub async fn delete(&self, room_id: &str) -> ApiResult<()> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let list = doc
            .get_mut("rooms")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| ApiError::new(500, "groups.json is not valid"))?;
        let before = list.len();
        list.retain(|room| room["id"].as_str() != Some(room_id));
        if list.len() == before {
            return Err(ApiError::not_found("No such group"));
        }
        write_doc(&self.path, &doc).await
    }

    /// Stores one message on the room and returns who it wakes.
    /// Members with no main thread are not dispatched.
    pub async fn send(
        &self,
        room_id: &str,
        directory: &[Member],
        text: &str,
        id: &str,
        at: &str,
        effective_default: AnswerPolicy,
    ) -> ApiResult<(Value, Vec<Dispatch>)> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ApiError::bad_request("Write a message first."));
        }
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = room_mut(&mut doc, room_id)?;
        let members = resolve(member_slugs(room), directory);
        let last = last_responder(room);
        let policy = resolve_policy(room, effective_default);
        let chief = effective_chief(room, &members);
        let chosen = plan_dispatches(&members, text, last.as_deref(), policy, chief.as_deref());
        let title = room["title"].as_str().unwrap_or("").to_string();
        let chief_line = (policy == AnswerPolicy::Chief)
            .then_some(chief.as_deref())
            .flatten();
        let dispatches = frame(
            &title,
            &members,
            "You",
            text,
            targets(&members, &chosen),
            chief_line,
            wants_everyone(&members, text),
        );
        push_message(room, &message(id, at, "user", "You", text))?;
        room["hops"] = json!(0);
        anchor_all(room, &dispatches);
        let stored = room.clone();
        write_doc(&self.path, &doc).await?;
        Ok((stored, dispatches))
    }

    /// Appends one reply under that Bot's name. A repeated id is kept once.
    pub async fn reply(
        &self,
        room_id: &str,
        slug: &str,
        name: &str,
        text: &str,
        id: &str,
        at: &str,
    ) -> ApiResult<Value> {
        let text = text.trim();
        if text.is_empty() || slug.is_empty() {
            return Err(ApiError::bad_request("A reply needs a Bot and some text."));
        }
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = room_mut(&mut doc, room_id)?;
        if !has_message(room, id) {
            push_message(room, &message(id, at, slug, name, text))?;
        }
        let stored = room.clone();
        write_doc(&self.path, &doc).await?;
        Ok(stored)
    }

    /// Copies each member's finished replies after its anchor into the room, and wakes the
    /// members those replies mention. `threads` maps a slug to its thread's messages
    /// (`id`, `role`, `text`, `streaming`, `at`), oldest first.
    pub async fn sync(
        &self,
        room_id: &str,
        directory: &[Member],
        threads: &HashMap<String, Vec<Value>>,
        effective_default: AnswerPolicy,
    ) -> ApiResult<(Value, Vec<Dispatch>)> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let room = room_mut(&mut doc, room_id)?;
        let members = resolve(member_slugs(room), directory);
        let policy = resolve_policy(room, effective_default);
        let budget = max_hops(policy, members.len());
        let chief = effective_chief(room, &members);
        let chief_line = (policy == AnswerPolicy::Chief)
            .then_some(chief.as_deref())
            .flatten();
        let title = room["title"].as_str().unwrap_or("").to_string();
        let mut wake: Vec<Dispatch> = Vec::new();
        let mut changed = false;
        for member in &members {
            let Some(messages) = threads.get(&member.slug) else {
                continue;
            };
            let Some(anchor) = room["anchors"][&member.slug].as_str().map(str::to_string) else {
                continue;
            };
            // T3 names a thread item `turn-item:message:<id>` after the id the command gave it.
            let suffix = format!(":{anchor}");
            let Some(position) = messages.iter().position(|m| {
                m["id"]
                    .as_str()
                    .is_some_and(|id| id == anchor || id.ends_with(&suffix))
            }) else {
                continue;
            };
            for reply in &messages[position + 1..] {
                let text = reply["text"].as_str().unwrap_or("").trim();
                let id = reply["id"].as_str().unwrap_or("");
                if reply["role"] != "bot"
                    || reply["streaming"] == true
                    || text.is_empty()
                    || id.is_empty()
                {
                    continue;
                }
                if has_message(room, id) {
                    continue;
                }
                let at = reply["at"].as_str().unwrap_or("");
                push_message(room, &message(id, at, &member.slug, &member.title, text))?;
                changed = true;
                if is_courtesy(text) {
                    continue;
                }
                let hops = room["hops"].as_u64().unwrap_or(0);
                if hops >= budget {
                    continue;
                }
                let others: Vec<Member> = members
                    .iter()
                    .filter(|m| m.slug != member.slug)
                    .cloned()
                    .collect();
                // A Bot's own `@everyone` or `@all` is not a wake-up. Only a named member is.
                let named = mentioned(&others, text);
                let mut chosen = targets(&others, &named);
                chosen.truncate(usize::try_from(budget - hops).unwrap_or(0));
                if chosen.is_empty() {
                    continue;
                }
                room["hops"] = json!(hops + u64::try_from(chosen.len()).unwrap_or(0));
                let framed = frame(
                    &title,
                    &members,
                    &member.title,
                    text,
                    chosen,
                    chief_line,
                    false,
                );
                anchor_all(room, &framed);
                wake.extend(framed);
                changed = true;
            }
        }
        let stored = room.clone();
        if changed {
            write_doc(&self.path, &doc).await?;
        }
        Ok((stored, wake))
    }

    pub async fn get(&self, room_id: &str) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        let doc = read_doc(&self.path).await?;
        rooms_of(&doc)
            .into_iter()
            .find(|room| room["id"].as_str() == Some(room_id))
            .ok_or_else(|| ApiError::not_found("No such group"))
    }
}

/// The commands a room send dispatches. No quote field is added.
pub(crate) fn dispatch_commands(dispatches: &[Dispatch]) -> Vec<Value> {
    dispatches
        .iter()
        .map(|item| {
            let mut command = crate::chat::send_command(&item.thread_id, &item.text, &[]);
            command["commandId"] = json!(item.message_id);
            command["messageId"] = json!(item.message_id);
            command
        })
        .collect()
}

fn distinct(slugs: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for slug in slugs {
        if !out.contains(slug) {
            out.push(slug.clone());
        }
    }
    out
}

fn clean_color(raw: &str) -> String {
    let raw = raw.trim();
    let avatar = &hbots_design::tokens().avatar;
    let name = raw.strip_prefix("brand.").unwrap_or(raw);
    let named = avatar.picker.iter().any(|c| c == name) || (raw == "brand.default");
    let hex =
        raw.len() == 7 && raw.starts_with('#') && raw[1..].chars().all(|c| c.is_ascii_hexdigit());
    if named || hex {
        raw.to_string()
    } else {
        avatar.default.to_string()
    }
}

/// Whoever spoke last in the room, if a Bot has.
fn last_responder(room: &Value) -> Option<String> {
    room["messages"].as_array()?.iter().rev().find_map(|m| {
        let from = m["from"].as_str()?;
        (from != "user").then(|| from.to_string())
    })
}

/// Who a message from the person wakes, as indexes into `members`.
pub(crate) fn plan_dispatches(
    members: &[Member],
    text: &str,
    last: Option<&str>,
    policy: AnswerPolicy,
    chief: Option<&str>,
) -> Vec<usize> {
    // A real member named `everyone` or `all` wins over the whole-group wake word.
    let named = mentioned(members, text);
    if !named.is_empty() && !wants_everyone(members, text) {
        return named;
    }
    if wants_everyone(members, text) {
        return (0..members.len()).collect();
    }
    if let Some(index) = named_one(members, text) {
        return vec![index];
    }
    match policy {
        AnswerPolicy::Last => last_or_everyone(members, last),
        AnswerPolicy::Everyone => (0..members.len()).collect(),
        AnswerPolicy::Chief => {
            if let Some(index) = chief.and_then(|slug| members.iter().position(|m| m.slug == slug))
            {
                let member = &members[index];
                if member.provisioned && !member.thread_id.is_empty() {
                    return vec![index];
                }
            }
            last_or_everyone(members, last)
        }
    }
}

fn last_or_everyone(members: &[Member], last: Option<&str>) -> Vec<usize> {
    if let Some(index) = last.and_then(|slug| members.iter().position(|m| m.slug == slug)) {
        return vec![index];
    }
    (0..members.len()).collect()
}

/// Slug and thread of each chosen member that has a thread to wake.
fn targets(members: &[Member], indexes: &[usize]) -> Vec<(String, String)> {
    indexes
        .iter()
        .filter_map(|i| members.get(*i))
        .filter(|m| !m.thread_id.is_empty())
        .map(|m| (m.slug.clone(), m.thread_id.clone()))
        .collect()
}

/// Wraps `text` for each target so a Bot knows it is in a group and how to bring someone in.
/// The roster lists names without `@`, so a Bot that echoes the header does not wake everyone.
fn frame(
    title: &str,
    members: &[Member],
    from: &str,
    text: &str,
    targets: Vec<(String, String)>,
    chief: Option<&str>,
    everyone: bool,
) -> Vec<Dispatch> {
    let names = members
        .iter()
        .map(|m| m.title.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let slugs = members
        .iter()
        .map(|m| m.slug.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    targets
        .into_iter()
        .map(|(slug, thread_id)| {
            let chief_line = if chief == Some(slug.as_str()) {
                " You are the group chief. Answer first, and bring in members with @slug only when you need them."
            } else {
                ""
            };
            let wrote = if everyone { "wrote to everyone" } else { "wrote" };
            let everyone_line = if everyone {
                "\nEveryone is being asked, so answer for yourself."
            } else {
                ""
            };
            Dispatch {
                slug,
                thread_id,
                message_id: uuid::Uuid::new_v4().to_string(),
                text: format!(
                    "[Group \"{title}\": {names}. {from} {wrote}:]{chief_line}{everyone_line}\n{text}\n\n[Your reply is shared with the group. To ask one member for something, write @ and their slug ({slugs}). Do not mention anyone to agree, thank, or acknowledge.]"
                ),
            }
        })
        .collect()
}

/// A short thanks or acknowledgement, even with an `@`, must not start another turn.
fn is_courtesy(text: &str) -> bool {
    const ACK: &[&str] = &[
        "thanks",
        "thank",
        "thx",
        "ty",
        "ok",
        "okay",
        "k",
        "got",
        "it",
        "on",
        "sure",
        "noted",
        "done",
        "yep",
        "yeah",
        "yes",
        "welcome",
        "np",
        "cool",
        "great",
        "perfect",
        "sounds",
        "good",
        "will",
        "do",
        "acknowledged",
        "roger",
        "alright",
        "right",
        "nice",
        "awesome",
        "cheers",
        "appreciated",
        "you",
        "too",
        "as",
        "well",
        "no",
        "problem",
        "of",
        "course",
    ];
    let mentions = mention_words(text);
    let words: Vec<String> = text
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .filter(|w| !mentions.iter().any(|m| m == w))
        .collect();
    words.is_empty() || (words.len() <= 8 && words.iter().all(|w| ACK.contains(&w.as_str())))
}

fn anchor_all(room: &mut Value, dispatches: &[Dispatch]) {
    if !room["anchors"].is_object() {
        room["anchors"] = json!({});
    }
    for item in dispatches {
        room["anchors"][&item.slug] = json!(item.message_id);
    }
}

/// `@everyone` and `@all` ask the whole group. `@team` and `@group` do not.
/// A token that [`mentioned`] would claim as a member is that member, not this wake.
fn wants_everyone(members: &[Member], text: &str) -> bool {
    mention_words(text).iter().any(|word| {
        matches!(word.as_str(), "all" | "everyone")
            && !members.iter().any(|member| mention_matches(member, word))
    })
}

fn mention_matches(member: &Member, word: &str) -> bool {
    let compact: String = member
        .title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    word == member.slug.to_lowercase() || word == compact
}

/// The lowercase words that follow an `@`.
fn mention_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let starts_word = i == 0 || !chars[i - 1].is_alphanumeric();
        if chars[i] == '@' && starts_word {
            let mut word = String::new();
            let mut j = i + 1;
            while j < chars.len()
                && (chars[j].is_alphanumeric() || chars[j] == '-' || chars[j] == '_')
            {
                word.extend(chars[j].to_lowercase());
                j += 1;
            }
            if !word.is_empty() {
                out.push(word);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Members named with `@slug` or `@name` (the name without spaces), in room order.
pub(crate) fn mentioned(members: &[Member], text: &str) -> Vec<usize> {
    let words = mention_words(text);
    members
        .iter()
        .enumerate()
        .filter(|(_, member)| words.iter().any(|word| mention_matches(member, word)))
        .map(|(i, _)| i)
        .collect()
}

pub(crate) fn present(room: &Value, directory: &[Member], default: AnswerPolicy) -> Value {
    let members = resolve(member_slugs(room), directory);
    let stored = room["answerPolicy"].as_str().unwrap_or("default");
    let stored = if matches!(stored, "default" | "last" | "everyone" | "chief") {
        stored
    } else {
        "default"
    };
    json!({
        "id": room["id"],
        "title": room["title"],
        "description": room["description"].as_str().unwrap_or(""),
        "color": room["color"].as_str().unwrap_or(&hbots_design::tokens().avatar.default),
        "shape": room["shape"].as_str().unwrap_or("ball"),
        "members": members.iter().map(member_row).collect::<Vec<_>>(),
        "messages": room["messages"].as_array().cloned().unwrap_or_default(),
        "notifications": notifications_on(room),
        "answerPolicy": stored,
        "answerPolicyEffective": resolve_policy(room, default).as_str(),
        "chief": room["chief"].as_str().filter(|slug| !slug.is_empty()),
    })
}

/// One row of the list: no messages, the last one as a preview.
pub(crate) fn summary(room: &Value) -> Value {
    let last = room["messages"]
        .as_array()
        .and_then(|l| l.last())
        .map(|m| json!({ "name": m["name"], "text": m["text"], "at": m["at"], "from": m["from"] }));
    json!({
        "id": room["id"],
        "title": room["title"],
        "description": room["description"].as_str().unwrap_or(""),
        "color": room["color"].as_str().unwrap_or(&hbots_design::tokens().avatar.default),
        "shape": room["shape"].as_str().unwrap_or("ball"),
        "members": room["members"],
        "last": last,
        "unread": is_unread(room),
        "notifications": notifications_on(room),
    })
}

/// Missing means on. `false` mutes the room and leaves the unread dot as it is.
fn notifications_on(room: &Value) -> bool {
    room["notifications"].as_bool().unwrap_or(true)
}

/// The last message is new when a Bot wrote it after the person last opened the room.
fn is_unread(room: &Value) -> bool {
    let Some(last) = room["messages"].as_array().and_then(|list| list.last()) else {
        return false;
    };
    let from = last["from"].as_str().unwrap_or("");
    let at = last["at"].as_str().unwrap_or("");
    let seen = room["lastSeenAt"].as_str().unwrap_or("");
    !from.is_empty() && from != "user" && !at.is_empty() && at > seen
}

pub(crate) fn member_row(member: &Member) -> Value {
    json!({
        "slug": member.slug,
        "name": member.title,
        "working": member.provisioned && member.working,
        "ready": member.provisioned,
        "state": face_state(member),
        "line": status_line(member),
    })
}

pub(crate) fn face_state(member: &Member) -> &'static str {
    if !member.provisioned {
        "asleep"
    } else if member.working {
        "working"
    } else {
        "idle"
    }
}

pub(crate) fn status_line(member: &Member) -> &'static str {
    if !member.provisioned {
        "Getting set up…"
    } else if member.working {
        "Working"
    } else {
        "Ready"
    }
}

/// Directory rows for the slugs stored on the room, in that order.
pub(crate) fn resolve(slugs: Vec<String>, directory: &[Member]) -> Vec<Member> {
    slugs
        .into_iter()
        .filter_map(|slug| directory.iter().find(|member| member.slug == slug).cloned())
        .collect()
}

fn named_one(members: &[Member], text: &str) -> Option<usize> {
    let mut hit: Option<usize> = None;
    for (index, member) in members.iter().enumerate() {
        if word_hit(text, &member.slug) || word_hit(text, &member.title) {
            if hit.is_some() {
                return None;
            }
            hit = Some(index);
        }
    }
    hit
}

fn word_hit(text: &str, needle: &str) -> bool {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == needle)
}

fn message(id: &str, at: &str, from: &str, name: &str, text: &str) -> Value {
    json!({ "id": id, "at": at, "from": from, "name": name, "text": text })
}

/// A Bot with `role` subagent, or `hideFromSidebar`, cannot take a seat in a group.
fn refuse_subagent(slugs: &[String], bots: &[Value]) -> ApiResult<()> {
    for slug in slugs {
        let Some(bot) = bots
            .iter()
            .find(|bot| bot["slug"].as_str() == Some(slug.as_str()))
        else {
            continue;
        };
        if crate::bots::is_subagent(bot) {
            return Err(ApiError::new(409, "A subagent can't join a group."));
        }
    }
    Ok(())
}

fn member_slugs(room: &Value) -> Vec<String> {
    room["members"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn has_message(room: &Value, id: &str) -> bool {
    room["messages"].as_array().is_some_and(|list| {
        list.iter()
            .any(|message| message["id"].as_str() == Some(id))
    })
}

fn push_message(room: &mut Value, message: &Value) -> ApiResult<()> {
    let list = room
        .as_object_mut()
        .and_then(|room| room.get_mut("messages"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ApiError::new(500, "groups.json is not valid"))?;
    list.push(message.clone());
    Ok(())
}

fn rooms_of(doc: &Value) -> Vec<Value> {
    doc["rooms"].as_array().cloned().unwrap_or_default()
}

fn push_room(doc: &mut Value, room: Value) -> ApiResult<()> {
    let list = doc
        .as_object_mut()
        .and_then(|doc| doc.get_mut("rooms"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ApiError::new(500, "groups.json is not valid"))?;
    list.push(room);
    Ok(())
}

fn room_mut<'a>(doc: &'a mut Value, room_id: &str) -> ApiResult<&'a mut Value> {
    doc.get_mut("rooms")
        .and_then(Value::as_array_mut)
        .and_then(|rooms| {
            rooms
                .iter_mut()
                .find(|room| room["id"].as_str() == Some(room_id))
        })
        .ok_or_else(|| ApiError::not_found("No such room"))
}

async fn read_doc(path: &std::path::Path) -> ApiResult<Value> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => {
            #[allow(
                clippy::map_err_ignore,
                reason = "serde quotes groups.json, which holds room messages"
            )]
            let doc: Value = serde_json::from_str(&text)
                .map_err(|_| ApiError::new(500, "groups.json is not valid"))?;
            if !doc["rooms"].is_array() {
                return Err(ApiError::new(500, "groups.json is not valid"));
            }
            Ok(doc)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(json!({ "version": 1, "rooms": [] }))
        }
        Err(err) => Err(err.into()),
    }
}

async fn write_doc(path: &std::path::Path, doc: &Value) -> ApiResult<()> {
    crate::store::write_json(path, doc).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_main_thread_is_missing() {
        assert!(Rooms::group_thread_missing("", "main-1"));
        assert!(Rooms::group_thread_missing("main-1", "main-1"));
        assert!(!Rooms::group_thread_missing("group-1", "main-1"));
    }

    fn directory() -> Vec<Member> {
        vec![
            Member {
                slug: "ada".into(),
                title: "Ada".into(),
                thread_id: "thread-ada".into(),
                working: true,
                provisioned: true,
            },
            Member {
                slug: "bea".into(),
                title: "Bea".into(),
                thread_id: "thread-bea".into(),
                working: false,
                provisioned: true,
            },
        ]
    }

    #[tokio::test]
    async fn a_room_send_stores_one_message_and_fans_out_to_each_main_thread() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let created = rooms
            .create(
                "Design",
                "",
                "",
                "",
                &["ada".into(), "bea".into()],
                &[],
                None,
                None,
            )
            .await
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        assert!(!rooms.list().await.unwrap().is_empty());

        let (stored, dispatches) = rooms
            .send(
                &id,
                &directory(),
                "hello both",
                "m1",
                "t1",
                AnswerPolicy::Last,
            )
            .await
            .unwrap();
        assert_eq!(stored["messages"].as_array().unwrap().len(), 1);
        assert_eq!(stored["messages"][0]["text"], "hello both");
        assert_eq!(stored["messages"][0]["from"], "user");
        assert_eq!(dispatches.len(), 2);
        assert_eq!(dispatches[0].thread_id, "thread-ada");
        assert_eq!(dispatches[1].thread_id, "thread-bea");

        let commands = dispatch_commands(&dispatches);
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0]["threadId"], "thread-ada");
        assert_eq!(commands[1]["threadId"], "thread-bea");
        assert_eq!(commands[0]["type"], "message.dispatch");
        assert!(commands[0].get("quote").is_none());
        assert_eq!(commands[0]["messageId"], commands[0]["commandId"]);
        assert_ne!(commands[0]["messageId"], commands[1]["messageId"]);

        let again = Rooms::new(dir.path().join("groups.json"));
        let loaded = again.get(&id).await.unwrap();
        assert_eq!(loaded["messages"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn naming_one_member_by_slug_or_title_dispatches_only_to_that_member() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = rooms
            .create(
                "Design",
                "",
                "",
                "",
                &["ada".into(), "bea".into()],
                &[],
                None,
                None,
            )
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let (_, by_slug) = rooms
            .send(
                &id,
                &directory(),
                "ada please look",
                "m2",
                "t2",
                AnswerPolicy::Last,
            )
            .await
            .unwrap();
        assert_eq!(by_slug.len(), 1);
        assert_eq!(by_slug[0].slug, "ada");
        assert_eq!(by_slug[0].thread_id, "thread-ada");

        let (stored, by_title) = rooms
            .send(
                &id,
                &directory(),
                "ask Bea about it",
                "m3",
                "t3",
                AnswerPolicy::Last,
            )
            .await
            .unwrap();
        assert_eq!(by_title.len(), 1);
        assert_eq!(by_title[0].slug, "bea");
        assert_eq!(stored["messages"].as_array().unwrap().len(), 2);

        let (_, both_named) = rooms
            .send(
                &id,
                &directory(),
                "Ada and Bea",
                "m4",
                "t4",
                AnswerPolicy::Last,
            )
            .await
            .unwrap();
        assert_eq!(both_named.len(), 2);
    }

    #[tokio::test]
    async fn a_member_without_a_thread_stays_quiet_and_replies_keep_order() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = rooms
            .create(
                "Design",
                "",
                "",
                "",
                &["ada".into(), "bea".into()],
                &[],
                None,
                None,
            )
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let members = vec![
            Member {
                slug: "ada".into(),
                title: "Ada".into(),
                thread_id: "thread-ada".into(),
                working: true,
                provisioned: true,
            },
            Member {
                slug: "bea".into(),
                title: "Bea".into(),
                thread_id: String::new(),
                working: false,
                provisioned: false,
            },
        ];
        let (_, dispatches) = rooms
            .send(&id, &members, "hello", "m1", "t1", AnswerPolicy::Last)
            .await
            .unwrap();
        assert_eq!(dispatches.len(), 1);
        assert_eq!(dispatches[0].slug, "ada");

        rooms
            .reply(&id, "ada", "Ada", "On it", "r1", "t2")
            .await
            .unwrap();
        rooms
            .reply(&id, "ada", "Ada", "On it", "r1", "t3")
            .await
            .unwrap();
        let stored = rooms
            .reply(&id, "bea", "Bea", "I am still starting", "r2", "t4")
            .await
            .unwrap();
        let messages = stored["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["name"], "Ada");
        assert_eq!(messages[1]["from"], "ada");
        assert_eq!(messages[2]["name"], "Bea");
        assert_eq!(messages[2]["text"], "I am still starting");

        let view = present(&stored, &members, AnswerPolicy::Last);
        assert_eq!(view["members"][0]["name"], "Ada");
        assert_eq!(view["members"][0]["line"], "Working");
        assert_eq!(view["members"][0]["state"], "working");
        assert_eq!(view["members"][1]["line"], "Getting set up…");
        assert_eq!(view["members"][1]["ready"], false);
        assert_eq!(view["members"][1]["state"], "asleep");
        assert_eq!(view["messages"].as_array().unwrap().len(), 3);
    }

    fn trio() -> Vec<Member> {
        let mut m = directory();
        m.push(Member {
            slug: "cy".into(),
            title: "Cy Park".into(),
            thread_id: "thread-cy".into(),
            working: false,
            provisioned: true,
        });
        m
    }

    async fn trio_room(rooms: &Rooms) -> String {
        rooms
            .create(
                "Team",
                "Plans the week",
                "blue",
                "hex",
                &["ada".into(), "bea".into(), "cy".into()],
                &[],
                None,
                None,
            )
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn slugs(dispatches: &[Dispatch]) -> Vec<&str> {
        dispatches.iter().map(|d| d.slug.as_str()).collect()
    }

    #[tokio::test]
    async fn a_group_keeps_its_description_and_avatar() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = trio_room(&rooms).await;
        let room = rooms.get(&id).await.unwrap();
        assert_eq!(room["description"], "Plans the week");
        assert_eq!(room["color"], "blue");
        assert_eq!(room["shape"], "hex");

        let odd = rooms
            .create(
                "Odd",
                "",
                "not-a-colour",
                "not-a-shape",
                &["ada".into(), "bea".into()],
                &[],
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(odd["color"], hbots_design::tokens().avatar.default.as_str());
        assert_eq!(
            odd["shape"], "ball",
            "an unknown shape falls back to the ball"
        );
        let app = rooms
            .create(
                "App",
                "",
                "brand.violet",
                "egg",
                &["ada".into(), "bea".into()],
                &[],
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            app["color"], "brand.violet",
            "the app's own colour names are kept"
        );

        let view = present(&room, &trio(), AnswerPolicy::Last);
        assert_eq!(view["description"], "Plans the week");
        assert_eq!(view["shape"], "hex");
        let row = summary(&room);
        assert!(
            row.get("messages").is_none(),
            "the list carries no transcript"
        );
        assert_eq!(row["last"], Value::Null);
    }

    #[tokio::test]
    async fn create_needs_a_name_and_two_distinct_bots() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let ada = vec!["ada".to_string()];
        assert!(rooms
            .create(
                " ",
                "",
                "",
                "",
                &["ada".into(), "bea".into()],
                &[],
                None,
                None
            )
            .await
            .is_err());
        assert!(rooms
            .create("Solo", "", "", "", &ada, &[], None, None)
            .await
            .is_err());
        let twice = vec!["ada".to_string(), "ada".to_string()];
        let err = rooms
            .create("Twin", "", "", "", &twice, &[], None, None)
            .await
            .unwrap_err();
        assert_eq!(
            err.message, "A group needs two Bots.",
            "a duplicate is not a second Bot"
        );
    }

    #[tokio::test]
    async fn update_changes_fields_keeps_messages_and_refuses_one_member() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = trio_room(&rooms).await;
        rooms
            .send(&id, &trio(), "hello", "m1", "t1", AnswerPolicy::Last)
            .await
            .unwrap();
        let updated = rooms
            .update(&id, &json!({ "title": "Crew", "description": "", "color": "red", "members": ["ada", "cy"] }), &[])
            .await
            .unwrap();
        assert_eq!(updated["title"], "Crew");
        assert_eq!(updated["description"], "");
        assert_eq!(updated["color"], "red");
        assert_eq!(updated["shape"], "hex", "an omitted field stays");
        assert_eq!(updated["members"], json!(["ada", "cy"]));
        assert_eq!(updated["messages"].as_array().unwrap().len(), 1);
        assert!(rooms
            .update(&id, &json!({ "members": ["ada"] }), &[])
            .await
            .is_err());
        assert!(rooms
            .update(&id, &json!({ "title": "  " }), &[])
            .await
            .is_err());
        assert!(rooms
            .update("missing", &json!({ "title": "x" }), &[])
            .await
            .is_err());
    }

    #[tokio::test]
    async fn a_room_without_last_seen_is_stamped_and_only_a_later_bot_message_is_unread() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("groups.json");
        let old = json!({
            "version": 1,
            "rooms": [{
                "id": "room-1",
                "title": "Team",
                "description": "",
                "color": "blue",
                "shape": "ball",
                "members": ["ada", "bea"],
                "messages": [{
                    "id": "m0", "at": "2020-01-01T00:00:00.000Z",
                    "from": "ada", "name": "Ada", "text": "old"
                }],
                "anchors": {},
                "threads": {},
                "hops": 0
            }]
        });
        tokio::fs::write(&path, serde_json::to_vec(&old).unwrap())
            .await
            .unwrap();
        let rooms = Rooms::new(path);
        let listed = rooms.list().await.unwrap();
        let stamp = listed[0]["lastSeenAt"].as_str().unwrap().to_string();
        assert!(!stamp.is_empty());
        let row = summary(&listed[0]);
        assert_eq!(row["unread"], false, "an old room is not unread");
        assert_eq!(row["notifications"], true, "a missing key stays on");
        assert_eq!(row["last"]["from"], "ada");
        assert_eq!(
            present(&listed[0], &[], AnswerPolicy::Last)["notifications"],
            true
        );
        let again = rooms.list().await.unwrap();
        assert_eq!(
            again[0]["lastSeenAt"],
            json!(stamp),
            "the stamp is written once"
        );

        let created = rooms
            .create(
                "Design",
                "",
                "",
                "",
                &["ada".into(), "bea".into()],
                &[],
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(created["notifications"], true);

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let bot_at = crate::vault::now_iso();
        assert!(bot_at.as_str() > stamp.as_str());
        rooms
            .reply("room-1", "ada", "Ada", "new", "m1", &bot_at)
            .await
            .unwrap();
        let after_bot = rooms.get("room-1").await.unwrap();
        assert_eq!(summary(&after_bot)["unread"], true);
        assert_eq!(summary(&after_bot)["last"]["from"], "ada");

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let user_at = crate::vault::now_iso();
        rooms
            .send(
                "room-1",
                &directory(),
                "mine",
                "m2",
                &user_at,
                AnswerPolicy::Last,
            )
            .await
            .unwrap();
        let after_user = rooms.get("room-1").await.unwrap();
        assert_eq!(summary(&after_user)["unread"], false);
        assert_eq!(summary(&after_user)["last"]["from"], "user");

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let later = crate::vault::now_iso();
        rooms
            .reply("room-1", "bea", "Bea", "again", "m3", &later)
            .await
            .unwrap();
        let muted = rooms
            .update("room-1", &json!({ "notifications": false }), &[])
            .await
            .unwrap();
        assert_eq!(muted["notifications"], false);
        let muted_row = summary(&muted);
        assert_eq!(muted_row["unread"], true, "mute does not clear the dot");
        assert_eq!(muted_row["notifications"], false);
        assert_eq!(
            present(&muted, &[], AnswerPolicy::Last)["notifications"],
            false
        );
        let err = rooms
            .update("room-1", &json!({ "notifications": "no" }), &[])
            .await
            .unwrap_err();
        assert_eq!(err.message, "notifications must be a boolean");
        assert_eq!(
            rooms.get("room-1").await.unwrap()["notifications"],
            false,
            "a bad value is not stored"
        );

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let seen = rooms.seen("room-1").await.unwrap();
        assert!(seen["lastSeenAt"].as_str().unwrap() > later.as_str());
        assert_eq!(summary(&seen)["unread"], false);
    }

    async fn patch_bot(root: &std::path::Path, slug: &str, patch: Value) {
        let path = root.join(slug).join(".bot").join("bot.json");
        let text = tokio::fs::read_to_string(&path).await.unwrap();
        let mut meta: Value = serde_json::from_str(&text).unwrap();
        for (key, value) in patch.as_object().unwrap() {
            meta[key] = value.clone();
        }
        tokio::fs::write(&path, serde_json::to_vec(&meta).unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_subagent_cannot_take_a_seat_in_a_group() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("bots");
        let bots = crate::bots::Bots::new(root.clone(), String::new(), String::new());
        let ada = bots.create(&json!({ "name": "Ada" })).await.unwrap();
        let bea = bots.create(&json!({ "name": "Bea" })).await.unwrap();
        let ada_slug = ada["slug"].as_str().unwrap().to_string();
        let bea_slug = bea["slug"].as_str().unwrap().to_string();
        let kid = bots
            .create(&json!({ "name": "Kid", "parentSlug": ada_slug }))
            .await
            .unwrap();
        let kid_slug = kid["slug"].as_str().unwrap().to_string();
        let rooms = Rooms::new(dir.path().join("groups.json"));

        let loaded = bots.list().await.unwrap();
        let err = rooms
            .create(
                "Design",
                "",
                "",
                "",
                &[ada_slug.clone(), kid_slug.clone()],
                &loaded,
                None,
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(err.status, 409);
        assert!(rooms.list().await.unwrap().is_empty());

        patch_bot(&root, &bea_slug, json!({ "hideFromSidebar": true })).await;
        let loaded = bots.list().await.unwrap();
        let err = rooms
            .create(
                "Design",
                "",
                "",
                "",
                &[ada_slug.clone(), bea_slug.clone()],
                &loaded,
                None,
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(err.status, 409);

        patch_bot(
            &root,
            &bea_slug,
            json!({ "hideFromSidebar": false, "role": "bot" }),
        )
        .await;
        let loaded = bots.list().await.unwrap();
        let created = rooms
            .create(
                "Design",
                "",
                "",
                "",
                &[ada_slug.clone(), bea_slug.clone()],
                &loaded,
                None,
                None,
            )
            .await
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();

        // A seat taken while Bea was a normal Bot stays until a member list is saved.
        patch_bot(&root, &bea_slug, json!({ "role": "subagent" })).await;
        let before = rooms.get(&id).await.unwrap();
        let loaded = bots.list().await.unwrap();
        let err = rooms
            .update(
                &id,
                &json!({ "title": "Nope", "members": [ada_slug, kid_slug] }),
                &loaded,
            )
            .await
            .unwrap_err();
        assert_eq!(err.status, 409);
        let after = rooms.get(&id).await.unwrap();
        assert_eq!(after["title"], before["title"]);
        assert_eq!(after["members"], before["members"]);

        let cy = bots.create(&json!({ "name": "Cy" })).await.unwrap();
        let cy_slug = cy["slug"].as_str().unwrap().to_string();
        let loaded = bots.list().await.unwrap();
        let updated = rooms
            .update(&id, &json!({ "members": [ada_slug, cy_slug] }), &loaded)
            .await
            .unwrap();
        assert_eq!(updated["members"], json!([ada_slug, cy_slug]));
    }

    #[tokio::test]
    async fn delete_removes_one_group_and_leaves_the_other() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let keep = trio_room(&rooms).await;
        let drop = trio_room(&rooms).await;
        rooms.delete(&drop).await.unwrap();
        let left = rooms.list().await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0]["id"], json!(keep));
        assert!(
            rooms.delete(&drop).await.is_err(),
            "a second delete finds nothing"
        );
    }

    #[test]
    fn mentions_pick_members_by_slug_or_name_and_all_wakes_everyone() {
        let m = trio();
        assert_eq!(
            plan_dispatches(
                &m,
                "@ada and @bea please look",
                None,
                AnswerPolicy::Last,
                None
            ),
            vec![0, 1]
        );
        assert_eq!(
            plan_dispatches(&m, "@cypark can you check", None, AnswerPolicy::Last, None),
            vec![2],
            "a name without spaces"
        );
        assert_eq!(
            plan_dispatches(&m, "@Cy hi", None, AnswerPolicy::Last, None),
            vec![2],
            "case does not matter"
        );
        assert_eq!(
            plan_dispatches(&m, "@all stand up", None, AnswerPolicy::Last, None),
            vec![0, 1, 2]
        );
        assert_eq!(
            plan_dispatches(&m, "@everyone", None, AnswerPolicy::Last, None),
            vec![0, 1, 2]
        );
        assert_eq!(
            plan_dispatches(
                &m,
                "mail me at joe@example.com",
                None,
                AnswerPolicy::Last,
                None
            ),
            vec![0, 1, 2],
            "an address is not a mention"
        );
        assert_eq!(
            plan_dispatches(&m, "hello", None, AnswerPolicy::Last, None),
            vec![0, 1, 2],
            "nobody has spoken yet"
        );
    }

    #[test]
    fn without_a_mention_the_last_speaker_answers() {
        let m = trio();
        assert_eq!(
            plan_dispatches(&m, "and then?", Some("bea"), AnswerPolicy::Last, None),
            vec![1]
        );
        assert_eq!(
            plan_dispatches(&m, "@ada and then?", Some("bea"), AnswerPolicy::Last, None),
            vec![0],
            "a mention beats the last speaker"
        );
        assert_eq!(
            plan_dispatches(&m, "and then?", Some("gone"), AnswerPolicy::Last, None),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn everyone_wakes_the_whole_group_and_a_member_slug_wins() {
        let m = trio();
        let all = vec![0, 1, 2];
        assert_eq!(
            plan_dispatches(&m, "@everyone", Some("bea"), AnswerPolicy::Last, None),
            all,
            "an earlier speaker does not narrow @everyone"
        );
        assert_eq!(
            plan_dispatches(&m, "@ada @everyone", Some("bea"), AnswerPolicy::Last, None),
            all,
            "a named member beside @everyone still asks the group"
        );
        assert_eq!(
            plan_dispatches(
                &m,
                "please weigh in, @everyone, before lunch",
                Some("cy"),
                AnswerPolicy::Last,
                None
            ),
            all,
            "@everyone counts anywhere in the text"
        );
        assert_eq!(
            plan_dispatches(&m, "@team status", Some("bea"), AnswerPolicy::Last, None),
            vec![1],
            "@team is not a whole-group wake"
        );
        assert_eq!(
            plan_dispatches(&m, "@group status", Some("bea"), AnswerPolicy::Last, None),
            vec![1],
            "@group is not a whole-group wake"
        );

        let mut named_everyone = m.clone();
        named_everyone[1].slug = "everyone".into();
        assert_eq!(
            plan_dispatches(
                &named_everyone,
                "@everyone",
                Some("ada"),
                AnswerPolicy::Last,
                None
            ),
            vec![1],
            "a member whose slug is everyone is that member"
        );
        let mut named_all = trio();
        named_all[2].slug = "all".into();
        assert_eq!(
            plan_dispatches(&named_all, "@all", Some("bea"), AnswerPolicy::Last, None),
            vec![2],
            "a member whose slug is all is that member"
        );
    }

    #[tokio::test]
    async fn everyone_send_frames_the_whole_group() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = trio_room(&rooms).await;
        rooms
            .send(&id, &trio(), "@bea first", "m0", "t0", AnswerPolicy::Last)
            .await
            .unwrap();
        let mut threads = HashMap::new();
        let room = rooms.get(&id).await.unwrap();
        let anchor = room["anchors"]["bea"].as_str().unwrap().to_string();
        threads.insert("bea".into(), thread(&anchor, &[("b1", "I started", false)]));
        rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();

        let (_, woken) = rooms
            .send(
                &id,
                &trio(),
                "thoughts @everyone?",
                "m1",
                "t1",
                AnswerPolicy::Last,
            )
            .await
            .unwrap();
        assert_eq!(slugs(&woken), vec!["ada", "bea", "cy"]);
        let text = &woken[0].text;
        assert!(text.contains("You wrote to everyone:"));
        assert!(text.contains("Everyone is being asked, so answer for yourself."));
    }

    #[tokio::test]
    async fn a_mention_wakes_only_that_member_and_frames_the_message() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = trio_room(&rooms).await;
        let (_, woken) = rooms
            .send(
                &id,
                &trio(),
                "@bea draft the plan",
                "m1",
                "t1",
                AnswerPolicy::Last,
            )
            .await
            .unwrap();
        assert_eq!(slugs(&woken), vec!["bea"]);
        let text = &woken[0].text;
        assert!(text.contains("Group \"Team\""));
        assert!(
            text.contains("Ada, Bea, Cy Park"),
            "the roster lists names, not @handles"
        );
        assert!(
            !text.contains("Ada (@ada)"),
            "echoing the header must not wake everyone"
        );
        assert!(text.contains("You wrote:") && text.contains("draft the plan"));
        let commands = dispatch_commands(&woken);
        assert_eq!(commands[0]["messageId"], json!(woken[0].message_id));
        assert_eq!(commands[0]["threadId"], "thread-bea");
    }

    fn thread(anchor: &str, replies: &[(&str, &str, bool)]) -> Vec<Value> {
        let mut out = vec![
            json!({ "id": anchor, "role": "user", "text": "framed", "streaming": false, "at": "a0" }),
        ];
        for (id, text, streaming) in replies {
            out.push(json!({ "id": id, "role": "bot", "text": text, "streaming": streaming, "at": "a1" }));
        }
        out
    }

    #[tokio::test]
    async fn replies_after_the_anchor_join_the_room_once_and_streaming_ones_wait() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = trio_room(&rooms).await;
        let (_, woken) = rooms
            .send(&id, &trio(), "@ada hello", "m1", "t1", AnswerPolicy::Last)
            .await
            .unwrap();
        let anchor = woken[0].message_id.clone();
        let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
        threads.insert(
            "ada".into(),
            thread(
                &anchor,
                &[("a1", "Hi there", false), ("a2", "Typing", true)],
            ),
        );

        let (room, wake) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        assert!(wake.is_empty());
        let messages = room["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1]["from"], "ada");
        assert_eq!(messages[1]["text"], "Hi there");

        let (again, _) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        assert_eq!(
            again["messages"].as_array().unwrap().len(),
            2,
            "a second sync adds nothing"
        );

        threads.insert(
            "ada".into(),
            thread(
                &anchor,
                &[("a1", "Hi there", false), ("a2", "Done typing", false)],
            ),
        );
        let (done, _) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        assert_eq!(done["messages"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn a_thread_without_the_anchor_or_with_old_replies_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = trio_room(&rooms).await;
        rooms
            .send(&id, &trio(), "@ada hello", "m1", "t1", AnswerPolicy::Last)
            .await
            .unwrap();
        let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
        threads.insert(
            "ada".into(),
            vec![
                json!({ "id": "old", "role": "bot", "text": "From before the group", "streaming": false }),
                json!({ "id": "other", "role": "user", "text": "Not our message", "streaming": false }),
            ],
        );
        let (room, wake) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        assert_eq!(
            room["messages"].as_array().unwrap().len(),
            1,
            "only the person's message"
        );
        assert!(wake.is_empty());
    }

    #[tokio::test]
    async fn the_anchor_matches_the_item_id_t3_gives_a_dispatched_message() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = trio_room(&rooms).await;
        let (_, woken) = rooms
            .send(&id, &trio(), "@ada hello", "m1", "t1", AnswerPolicy::Last)
            .await
            .unwrap();
        let anchor = format!("turn-item:message:{}", woken[0].message_id);
        let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
        threads.insert(
            "ada".into(),
            vec![
                json!({ "id": "turn-item:message:older", "role": "user", "text": "earlier", "streaming": false }),
                json!({ "id": "turn-item:provider:x:old", "role": "bot", "text": "old reply", "streaming": false }),
                json!({ "id": anchor, "role": "user", "text": "framed", "streaming": false }),
                json!({ "id": "turn-item:provider:x:new", "role": "bot", "text": "fresh reply", "streaming": false, "at": "a2" }),
            ],
        );
        let (room, _) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        let texts: Vec<&str> = room["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap())
            .collect();
        assert_eq!(
            texts,
            vec!["@ada hello", "fresh reply"],
            "only what follows our message is imported"
        );
    }

    #[tokio::test]
    async fn a_reply_that_mentions_another_member_wakes_only_that_member() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = trio_room(&rooms).await;
        let (_, woken) = rooms
            .send(
                &id,
                &trio(),
                "@ada ask Cy for numbers",
                "m1",
                "t1",
                AnswerPolicy::Last,
            )
            .await
            .unwrap();
        let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
        threads.insert(
            "ada".into(),
            thread(
                &woken[0].message_id,
                &[("a1", "@cy what are this week's numbers?", false)],
            ),
        );
        let (room, wake) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        assert_eq!(slugs(&wake), vec!["cy"], "Bea stays asleep");
        assert!(wake[0].text.contains("Ada wrote:"));
        assert_eq!(room["hops"], 1);

        // Cy's answer is kept in the room. The hop budget is spent, so Ada is not woken again.
        threads.insert(
            "cy".into(),
            thread(&wake[0].message_id, &[("c1", "@ada 42 units", false)]),
        );
        let (room, back) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        assert!(back.is_empty(), "an answer does not start another turn");
        assert!(
            room["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["text"].as_str() == Some("@ada 42 units")),
            "Cy's answer still lands in the room"
        );
    }

    #[tokio::test]
    async fn a_bot_reply_containing_everyone_wakes_nobody() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = trio_room(&rooms).await;
        let (_, woken) = rooms
            .send(&id, &trio(), "@ada start", "m1", "t1", AnswerPolicy::Last)
            .await
            .unwrap();
        let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
        threads.insert(
            "ada".into(),
            thread(
                &woken[0].message_id,
                &[("a1", "I am asking @everyone to look at this", false)],
            ),
        );
        let (room, wake) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        assert!(wake.is_empty(), "a Bot's own @everyone is not a wake-up");
        assert_eq!(room["hops"], 0);
        assert!(room["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| { m["text"].as_str() == Some("I am asking @everyone to look at this") }));
    }

    #[tokio::test]
    async fn a_courtesy_reply_does_not_wake_anyone() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = trio_room(&rooms).await;
        let (_, woken) = rooms
            .send(&id, &trio(), "@ada start", "m1", "t1", AnswerPolicy::Last)
            .await
            .unwrap();
        let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
        threads.insert(
            "ada".into(),
            thread(&woken[0].message_id, &[("a1", "thanks @bea, on it", false)]),
        );
        let (room, wake) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        assert!(wake.is_empty(), "a thank-you must not start another turn");
        assert_eq!(room["hops"], 0, "courtesy does not spend the hop budget");
        assert!(room["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["text"].as_str() == Some("thanks @bea, on it")));
    }

    #[tokio::test]
    async fn bots_cannot_wake_each_other_forever() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = rooms
            .create(
                "Pair",
                "",
                "",
                "",
                &["ada".into(), "bea".into()],
                &[],
                None,
                None,
            )
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let (_, mut woken) = rooms
            .send(
                &id,
                &directory(),
                "@ada start",
                "m1",
                "t1",
                AnswerPolicy::Last,
            )
            .await
            .unwrap();
        let mut turns = 0;
        let mut counter = 0;
        while let Some(next) = woken.pop() {
            counter += 1;
            turns += 1;
            assert!(turns < 20, "the ping-pong never ends");
            let other = if next.slug == "ada" {
                "@bea your turn"
            } else {
                "@ada your turn"
            };
            let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
            threads.insert(
                next.slug.clone(),
                thread(&next.message_id, &[(&format!("r{counter}"), other, false)]),
            );
            let (_, wake) = rooms
                .sync(&id, &directory(), &threads, AnswerPolicy::Last)
                .await
                .unwrap();
            woken.extend(wake);
        }
        assert_eq!(
            u64::try_from(turns).unwrap_or(0),
            MAX_HOPS + 1,
            "the person's first wake-up, then at most {MAX_HOPS} more"
        );
        let room = rooms.get(&id).await.unwrap();
        // A new message from the person resets the budget.
        rooms
            .send(
                &id,
                &directory(),
                "@bea again",
                "m2",
                "t9",
                AnswerPolicy::Last,
            )
            .await
            .unwrap();
        assert_eq!(rooms.get(&id).await.unwrap()["hops"], 0);
        assert!(room["messages"].as_array().unwrap().len() > 1);
    }

    #[test]
    fn each_policy_picks_who_answers_and_an_explicit_name_still_wins() {
        let m = trio();
        assert_eq!(
            plan_dispatches(&m, "hello", None, AnswerPolicy::Everyone, Some("bea")),
            vec![0, 1, 2]
        );
        assert_eq!(
            plan_dispatches(&m, "hello", Some("cy"), AnswerPolicy::Last, Some("bea")),
            vec![2]
        );
        assert_eq!(
            plan_dispatches(&m, "hello", Some("cy"), AnswerPolicy::Chief, Some("bea")),
            vec![1],
            "the chief answers when nobody is named"
        );
        assert_eq!(
            plan_dispatches(
                &m,
                "@ada hello",
                Some("cy"),
                AnswerPolicy::Chief,
                Some("bea")
            ),
            vec![0],
            "an @mention beats the policy"
        );
        assert_eq!(
            plan_dispatches(&m, "@all hello", None, AnswerPolicy::Chief, Some("bea")),
            vec![0, 1, 2]
        );
        assert_eq!(
            plan_dispatches(&m, "ask Ada", None, AnswerPolicy::Everyone, Some("bea")),
            vec![0],
            "a single plain name beats the policy"
        );
    }

    #[test]
    fn a_chief_who_cannot_answer_falls_back_to_the_last_speaker_then_everyone() {
        let mut quiet = trio();
        quiet[1].thread_id.clear();
        quiet[1].provisioned = false;
        assert_eq!(
            plan_dispatches(
                &quiet,
                "hello",
                Some("cy"),
                AnswerPolicy::Chief,
                Some("bea")
            ),
            vec![2],
            "the last speaker answers when the chief has no thread"
        );
        assert_eq!(
            plan_dispatches(&quiet, "hello", None, AnswerPolicy::Chief, Some("bea")),
            vec![0, 1, 2],
            "everyone answers when the chief cannot and nobody has spoken"
        );
        assert_eq!(max_hops(AnswerPolicy::Last, 3), MAX_HOPS);
        assert_eq!(max_hops(AnswerPolicy::Everyone, 3), MAX_HOPS);
        assert_eq!(max_hops(AnswerPolicy::Chief, 3), 2);
    }

    #[test]
    fn the_effective_chief_is_the_stored_member_or_the_first() {
        let room = json!({ "members": ["ada", "bea", "cy"], "chief": "cy" });
        assert_eq!(effective_chief(&room, &trio()).as_deref(), Some("cy"));
        let dropped = json!({ "members": ["ada", "bea"], "chief": "gone" });
        assert_eq!(
            effective_chief(&dropped, &directory()).as_deref(),
            Some("ada")
        );
        let missing = json!({ "members": ["ada", "bea"] });
        assert_eq!(
            effective_chief(&missing, &directory()).as_deref(),
            Some("ada")
        );
    }

    #[tokio::test]
    async fn chief_mode_frames_only_the_chief_and_allows_a_longer_handoff() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = rooms
            .create(
                "Team",
                "",
                "",
                "",
                &["ada".into(), "bea".into(), "cy".into()],
                &[],
                Some("chief"),
                Some("bea"),
            )
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let (_, woken) = rooms
            .send(&id, &trio(), "hello", "m1", "t1", AnswerPolicy::Last)
            .await
            .unwrap();
        assert_eq!(slugs(&woken), vec!["bea"]);
        assert!(
            woken[0].text.contains(
                "You are the group chief. Answer first, and bring in members with @slug only when you need them."
            )
        );

        let mut threads: HashMap<String, Vec<Value>> = HashMap::new();
        threads.insert(
            "bea".into(),
            thread(&woken[0].message_id, &[("b1", "@ada take this", false)]),
        );
        let (_, to_ada) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        assert_eq!(slugs(&to_ada), vec!["ada"]);
        assert!(
            !to_ada[0].text.contains("You are the group chief"),
            "a member the chief brings in is not the chief"
        );
        threads.insert(
            "ada".into(),
            thread(&to_ada[0].message_id, &[("a1", "@cy and this", false)]),
        );
        let (_, to_cy) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        assert_eq!(
            slugs(&to_cy),
            vec!["cy"],
            "the second handoff is still inside the budget"
        );
        threads.insert(
            "cy".into(),
            thread(&to_cy[0].message_id, &[("c1", "@bea done", false)]),
        );
        let (room, back) = rooms
            .sync(&id, &trio(), &threads, AnswerPolicy::Last)
            .await
            .unwrap();
        assert!(back.is_empty(), "the budget is members_len - 1");
        assert_eq!(room["hops"], 2);
    }

    #[tokio::test]
    async fn a_room_without_the_new_fields_stays_on_last_and_a_group_override_beats_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("groups.json");
        let rooms = Rooms::new(path);
        let id = trio_room(&rooms).await;
        let stored = rooms.get(&id).await.unwrap();
        assert!(stored.get("answerPolicy").is_none());
        assert!(stored.get("chief").is_none());
        let view = present(&stored, &trio(), AnswerPolicy::Everyone);
        assert_eq!(view["answerPolicy"], "default");
        assert_eq!(view["answerPolicyEffective"], "everyone");
        assert_eq!(view["chief"], Value::Null);

        let (_, woken) = rooms
            .send(&id, &trio(), "hello", "m1", "t1", AnswerPolicy::Last)
            .await
            .unwrap();
        assert_eq!(slugs(&woken), vec!["ada", "bea", "cy"]);
        rooms
            .reply(&id, "ada", "Ada", "noted", "r1", "t1b")
            .await
            .unwrap();

        let updated = rooms
            .update(
                &id,
                &json!({ "answerPolicy": "everyone", "chief": "cy" }),
                &[],
            )
            .await
            .unwrap();
        let view = present(&updated, &trio(), AnswerPolicy::Last);
        assert_eq!(view["answerPolicy"], "everyone");
        assert_eq!(view["answerPolicyEffective"], "everyone");
        assert_eq!(view["chief"], "cy");
        let (_, all) = rooms
            .send(&id, &trio(), "again", "m2", "t2", AnswerPolicy::Last)
            .await
            .unwrap();
        assert_eq!(
            slugs(&all),
            vec!["ada", "bea", "cy"],
            "everyone overrides a settings default of last, even after Ada spoke"
        );

        rooms
            .update(&id, &json!({ "answerPolicy": "last" }), &[])
            .await
            .unwrap();
        rooms
            .reply(&id, "cy", "Cy", "on it", "r2", "t2b")
            .await
            .unwrap();
        let (_, only_last) = rooms
            .send(
                &id,
                &trio(),
                "and then?",
                "m3",
                "t3",
                AnswerPolicy::Everyone,
            )
            .await
            .unwrap();
        assert_eq!(
            slugs(&only_last),
            vec!["cy"],
            "the room's last policy beats a settings default of everyone"
        );
    }

    #[tokio::test]
    async fn answer_policy_and_chief_are_checked_and_a_dropped_member_clears_the_chief() {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::new(dir.path().join("groups.json"));
        let id = trio_room(&rooms).await;
        let err = rooms
            .update(&id, &json!({ "answerPolicy": "round" }), &[])
            .await
            .unwrap_err();
        assert_eq!(err.status, 400);
        let err = rooms
            .update(&id, &json!({ "chief": "nope" }), &[])
            .await
            .unwrap_err();
        assert_eq!(err.status, 400);
        let err = rooms
            .create(
                "Bad",
                "",
                "",
                "",
                &["ada".into(), "bea".into()],
                &[],
                Some("sideways"),
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(err.status, 400);

        let set = rooms
            .update(&id, &json!({ "chief": "bea" }), &[])
            .await
            .unwrap();
        assert_eq!(set["chief"], "bea");
        let cleared = rooms
            .update(&id, &json!({ "chief": "" }), &[])
            .await
            .unwrap();
        assert!(cleared.get("chief").is_none());
        rooms
            .update(&id, &json!({ "chief": "cy" }), &[])
            .await
            .unwrap();
        let dropped = rooms
            .update(&id, &json!({ "members": ["ada", "bea"] }), &[])
            .await
            .unwrap();
        assert!(dropped.get("chief").is_none());
        let view = present(&dropped, &directory(), AnswerPolicy::Chief);
        assert_eq!(view["answerPolicy"], "default");
        assert_eq!(view["answerPolicyEffective"], "chief");
    }
}
