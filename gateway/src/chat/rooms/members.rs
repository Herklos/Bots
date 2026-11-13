use super::*;

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

pub(crate) fn distinct(slugs: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for slug in slugs {
        if !out.contains(slug) {
            out.push(slug.clone());
        }
    }
    out
}

pub(crate) fn clean_color(raw: &str) -> String {
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
/// `working_slugs` is the members whose group thread is working, in member order.
/// Both fields are for the response. Nothing here is stored.
pub(crate) fn summary(room: &Value, working_slugs: &[String]) -> Value {
    let last = room["messages"]
        .as_array()
        .and_then(|l| l.last())
        .map(|m| json!({ "name": m["name"], "text": m["text"], "at": m["at"], "from": m["from"] }));
    let members = member_slugs(room);
    let working_slugs: Vec<&String> = members
        .iter()
        .filter(|slug| working_slugs.iter().any(|working| working == *slug))
        .collect();
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
        "working": !working_slugs.is_empty(),
        "workingSlugs": working_slugs,
    })
}

/// Missing means on. `false` mutes the room and leaves the unread dot as it is.
pub(crate) fn notifications_on(room: &Value) -> bool {
    room["notifications"].as_bool().unwrap_or(true)
}

/// The last message is new when a Bot wrote it after the person last opened the room.
pub(crate) fn is_unread(room: &Value) -> bool {
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

/// A Bot with `role` subagent, or `hideFromSidebar`, cannot take a seat in a group.
pub(crate) fn refuse_subagent(slugs: &[String], bots: &[Value]) -> ApiResult<()> {
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

pub(crate) fn member_slugs(room: &Value) -> Vec<String> {
    room["members"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

impl Rooms {
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
        let _disk_guard = self.store.acquire()
        .await?;
        let mut doc = self.read_doc().await?;
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
            self.write_doc(&doc).await?;
        }
        Ok((stored, wake))
    }
}
