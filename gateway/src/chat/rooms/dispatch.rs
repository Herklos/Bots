use super::*;

/// One message to send into a member's main thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dispatch {
    pub slug: String,
    pub thread_id: String,
    /// The T3 message id the dispatch creates; replies in the thread after it belong to the room.
    pub message_id: String,
    pub text: String,
}

/// The commands a room send dispatches. No quote field is added.
pub(crate) fn dispatch_commands(dispatches: &[Dispatch]) -> Vec<Value> {
    dispatches
        .iter()
        .map(|item| {
            let mut command =
                crate::harness::t3::transcript::send_command(&item.thread_id, &item.text, &[]);
            command["commandId"] = json!(item.message_id);
            command["messageId"] = json!(item.message_id);
            command
        })
        .collect()
}

/// Whoever spoke last in the room, if a Bot has.
pub(crate) fn last_responder(room: &Value) -> Option<String> {
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

pub(crate) fn last_or_everyone(members: &[Member], last: Option<&str>) -> Vec<usize> {
    if let Some(index) = last.and_then(|slug| members.iter().position(|m| m.slug == slug)) {
        return vec![index];
    }
    (0..members.len()).collect()
}

/// Slug and thread of each chosen member that has a thread to wake.
pub(crate) fn targets(members: &[Member], indexes: &[usize]) -> Vec<(String, String)> {
    indexes
        .iter()
        .filter_map(|i| members.get(*i))
        .filter(|m| !m.thread_id.is_empty())
        .map(|m| (m.slug.clone(), m.thread_id.clone()))
        .collect()
}

/// Wraps `text` for each target so a Bot knows it is in a group and how to bring someone in.
/// The roster lists names without `@`, so a Bot that echoes the header does not wake everyone.
pub(crate) fn frame(
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
pub(crate) fn is_courtesy(text: &str) -> bool {
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

pub(crate) fn anchor_all(room: &mut Value, dispatches: &[Dispatch]) {
    if !room["anchors"].is_object() {
        room["anchors"] = json!({});
    }
    for item in dispatches {
        room["anchors"][&item.slug] = json!(item.message_id);
    }
}

/// `@everyone` and `@all` ask the whole group. `@team` and `@group` do not.
/// A token that [`mentioned`] would claim as a member is that member, not this wake.
pub(crate) fn wants_everyone(members: &[Member], text: &str) -> bool {
    mention_words(text).iter().any(|word| {
        matches!(word.as_str(), "all" | "everyone")
            && !members.iter().any(|member| mention_matches(member, word))
    })
}

pub(crate) fn mention_matches(member: &Member, word: &str) -> bool {
    let compact: String = member
        .title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    word == member.slug.to_lowercase() || word == compact
}

/// The lowercase words that follow an `@`.
pub(crate) fn mention_words(text: &str) -> Vec<String> {
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

/// Directory rows for the slugs stored on the room, in that order.
pub(crate) fn resolve(slugs: Vec<String>, directory: &[Member]) -> Vec<Member> {
    slugs
        .into_iter()
        .filter_map(|slug| directory.iter().find(|member| member.slug == slug).cloned())
        .collect()
}

pub(crate) fn named_one(members: &[Member], text: &str) -> Option<usize> {
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

pub(crate) fn word_hit(text: &str, needle: &str) -> bool {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == needle)
}

impl Rooms {
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
}
