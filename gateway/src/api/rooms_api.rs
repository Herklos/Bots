use super::*;

pub(crate) async fn rooms_list(State(s): S) -> ApiResult<Json<Value>> {
    let listed = s.rooms.list().await?;
    let bots = s.bots.list().await?;
    let mut prepared = Vec::with_capacity(listed.len());
    for room in &listed {
        prepared.push(room_members(&s, room, &bots, false).await?);
    }
    let threads = room_thread_ids(prepared.iter().flatten());
    let reads = read_room_threads(&s, threads).await;
    let now = Instant::now();
    let mut book = s
        .room_working
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rooms: Vec<Value> = listed
        .iter()
        .zip(&prepared)
        .map(|(room, members)| {
            let working = working_slugs(members, &reads, &mut book, now);
            crate::chat::rooms::summary(room, &working)
        })
        .collect();
    Ok(Json(json!({ "rooms": rooms })))
}

pub(crate) fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Refuses a slug that is not a Bot on this machine. The list is the one `Bots::list` loaded.
pub(crate) async fn known_bots(s: &AppState, slugs: &[String]) -> ApiResult<Vec<Value>> {
    let known = s.bots.list().await?;
    if slugs.iter().any(|slug| {
        !known
            .iter()
            .any(|bot| bot["slug"].as_str() == Some(slug.as_str()))
    }) {
        return Err(ApiError::bad_request("Pick Bots that already exist."));
    }
    Ok(known)
}

pub(crate) async fn rooms_create(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let kickoff = match body.get("kickoff") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => {
            let text = text.trim();
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        }
        Some(_) => return Err(ApiError::bad_request("kickoff must be a string")),
    };
    let slugs = string_list(&body["members"]);
    let known = known_bots(&s, &slugs).await?;
    let room = s
        .rooms
        .create(
            body["title"].as_str().unwrap_or(""),
            body["description"].as_str().unwrap_or(""),
            body["color"].as_str().unwrap_or(""),
            body["shape"].as_str().unwrap_or(""),
            &slugs,
            &known,
            body["answerPolicy"].as_str(),
            body["chief"].as_str(),
        )
        .await?;
    let policy = group_answer_policy(&s.settings().await);
    let (members, _) = room_snapshot(&s, &room, true).await?;
    let room = if let Some(text) = kickoff {
        let id = room["id"].as_str().unwrap_or("");
        let (stored, dispatches) = s
            .rooms
            .send(
                id,
                &members,
                &text,
                &uuid::Uuid::new_v4().to_string(),
                &crate::chat::routines::stamped(),
                policy,
            )
            .await?;
        dispatch_room(&s, &dispatches).await?;
        stored
    } else {
        room
    };
    Ok(Json(crate::chat::rooms::present(&room, &members, policy)))
}

pub(crate) async fn room_document(s: &AppState, id: &str) -> ApiResult<Value> {
    let room = s.rooms.get(id).await?;
    let (members, threads) = room_snapshot(s, &room, false).await?;
    // Replies the members have finished since the last read join the room here, and a reply that
    // mentions another member wakes that member.
    let policy = group_answer_policy(&s.settings().await);
    let (room, wake) = s.rooms.sync(id, &members, &threads, policy).await?;
    if let Err(err) = dispatch_room(s, &wake).await {
        tracing::warn!("group wake-up failed: {}", err.message);
    }
    Ok(crate::chat::rooms::present(&room, &members, policy))
}

pub(crate) async fn room_get(
    State(s): S,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let view = room_document(&s, &id).await?;
    let tag = chat::room_tag(&view);
    let seen = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok());
    if !chat::none_match(seen, &tag) {
        return Ok(with_etag(&tag, view, true));
    }
    let key = format!("room\n{id}");
    let outcome = s
        .holds
        .hold_until_change(&key, &tag, chat::LOOK_EVERY, chat::HOLD_FOR, || {
            let s = s.clone();
            let id = id.clone();
            async move {
                match room_document(&s, &id).await {
                    Ok(next) => {
                        let next_tag = chat::room_tag(&next);
                        Some((next, next_tag))
                    }
                    Err(_) => None,
                }
            }
        })
        .await;
    match outcome {
        chat::Hold::Changed(next) => Ok(with_etag(&chat::room_tag(&next), next, true)),
        chat::Hold::Same | chat::Hold::Superseded | chat::Hold::Stopped => {
            Ok(with_etag(&tag, Value::Null, false))
        }
    }
}

pub(crate) async fn room_update(
    State(s): S,
    Path(id): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let known = if body.get("members").is_some() {
        known_bots(&s, &string_list(&body["members"])).await?
    } else {
        Vec::new()
    };
    let room = s.rooms.update(&id, &body, &known).await?;
    let policy = group_answer_policy(&s.settings().await);
    let (members, _) = room_snapshot(&s, &room, false).await?;
    Ok(Json(crate::chat::rooms::present(&room, &members, policy)))
}

pub(crate) async fn room_seen(State(s): S, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let room = s.rooms.seen(&id).await?;
    let policy = group_answer_policy(&s.settings().await);
    let (members, _) = room_snapshot(&s, &room, false).await?;
    Ok(Json(crate::chat::rooms::present(&room, &members, policy)))
}

pub(crate) async fn room_delete(State(s): S, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    s.rooms.delete(&id).await?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn room_send(
    State(s): S,
    Path(id): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let text = body["text"].as_str().unwrap_or("").trim().to_string();
    let message_id = crate::harness::message_id(body["messageId"].as_str());
    let room = s.rooms.get(&id).await?;
    let (members, _) = room_snapshot(&s, &room, true).await?;
    let (stored, dispatches) = s
        .rooms
        .send(
            &id,
            &members,
            &text,
            &message_id,
            &crate::chat::routines::stamped(),
            group_answer_policy(&s.settings().await),
        )
        .await?;
    let dispatched = dispatch_room(&s, &dispatches).await?;
    let policy = group_answer_policy(&s.settings().await);
    Ok(Json(json!({
        "ok": true,
        "messageId": message_id,
        "room": crate::chat::rooms::present(&stored, &members, policy),
        "dispatched": dispatched,
    })))
}

/// Sends each dispatch to its member's main thread and counts it as that Bot's message.
pub(crate) async fn dispatch_room(
    s: &AppState,
    dispatches: &[crate::chat::rooms::Dispatch],
) -> ApiResult<Vec<String>> {
    let mut sent = Vec::new();
    if dispatches.is_empty() {
        return Ok(sent);
    }
    let Ok(turns) = s.harness.turns() else {
        return Ok(sent);
    };
    let commands = crate::chat::rooms::dispatch_commands(dispatches);
    for (item, command) in dispatches.iter().zip(commands) {
        turns
            .send_user(&crate::harness::UserMessage::new(
                command["threadId"].as_str().unwrap_or(&item.thread_id),
                command["messageId"].as_str().unwrap_or(&item.message_id),
                command["text"].as_str().unwrap_or(&item.text),
                command["attachments"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            ))
            .await
            .map_err(|err| err.api())?;
        s.usage
            .note_for(&item.slug, crate::bots::usage::Event::Message)
            .await;
        {
            let mut book = s
                .room_working
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            book.note(&item.thread_id, Instant::now());
        }
        sent.push(item.slug.clone());
    }
    Ok(sent)
}

pub(crate) fn room_thread_ids<'a>(
    members: impl Iterator<Item = &'a crate::chat::rooms::Member>,
) -> Vec<String> {
    members
        .filter(|member| !member.thread_id.is_empty())
        .map(|member| member.thread_id.clone())
        .collect()
}

pub(crate) fn working_slugs(
    members: &[crate::chat::rooms::Member],
    reads: &HashMap<String, (crate::chat::rooms::MemberProbe, Vec<Value>)>,
    book: &mut crate::chat::rooms::WorkingBook,
    now: Instant,
) -> Vec<String> {
    members
        .iter()
        .filter(|member| {
            let probe = reads
                .get(&member.thread_id)
                .map(|(probe, _)| *probe)
                .unwrap_or(crate::chat::rooms::MemberProbe::Failed);
            !member.thread_id.is_empty() && book.decide(&member.thread_id, probe, now)
        })
        .map(|member| member.slug.clone())
        .collect()
}

pub(crate) async fn read_room_threads(
    s: &AppState,
    thread_ids: Vec<String>,
) -> HashMap<String, (crate::chat::rooms::MemberProbe, Vec<Value>)> {
    let harness = s.harness.clone();
    crate::chat::rooms::read_group_threads(thread_ids, |thread| {
        let harness = harness.clone();
        async move {
            let snapshot = harness
                .transcript()
                .map_err(|err| anyhow::anyhow!(err.to_string()))?
                .read(&thread)
                .await
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
            Ok(snapshot.group())
        }
    })
    .await
}

/// Member rows for a room, with each member's working flag, and each member's group thread messages.
/// With `ensure`, a member who is set up but has no thread for this group gets one now. An empty
/// id or the Bot's one-to-one thread counts as none: the read path drops it from the member and
/// passes no messages, and the next ensure launches. A failed launch stays empty. A group talks
/// in its own threads, so nothing it says lands in a Bot's one-to-one conversation.
pub(crate) async fn room_snapshot(
    s: &AppState,
    room: &Value,
    ensure: bool,
) -> ApiResult<(
    Vec<crate::chat::rooms::Member>,
    std::collections::HashMap<String, Vec<Value>>,
)> {
    let bots = s.bots.list().await?;
    let mut members = room_members(s, room, &bots, ensure).await?;
    let reads = read_room_threads(s, room_thread_ids(members.iter())).await;
    let now = Instant::now();
    let mut book = s
        .room_working
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut threads = std::collections::HashMap::new();
    for member in &mut members {
        if member.thread_id.is_empty() {
            member.working = false;
            continue;
        }
        if let Some((crate::chat::rooms::MemberProbe::Read { .. }, messages)) =
            reads.get(&member.thread_id)
        {
            // This thread is the group, so the one-to-one filter would drop the anchor and the reply.
            threads.insert(member.slug.clone(), messages.clone());
        }
        let probe = reads
            .get(&member.thread_id)
            .map(|(probe, _)| *probe)
            .unwrap_or(crate::chat::rooms::MemberProbe::Failed);
        member.working = book.decide(&member.thread_id, probe, now);
    }
    Ok((members, threads))
}

/// Seats for a room, before their projections are read. With `ensure`, a member who is set up
/// but has no thread for this group gets one now. An empty id or the Bot's one-to-one thread
/// counts as none. A failed launch stays empty.
pub(crate) async fn room_members(
    s: &AppState,
    room: &Value,
    bots: &[Value],
    ensure: bool,
) -> ApiResult<Vec<crate::chat::rooms::Member>> {
    let provisioning = s.harness.provisioning().ok();
    let room_id = room["id"].as_str().unwrap_or("").to_string();
    let settings = if ensure {
        s.settings().await
    } else {
        Value::Null
    };
    let mut members = Vec::new();
    for slug in string_list(&room["members"]) {
        let Some(bot) = bots
            .iter()
            .find(|bot| bot["slug"].as_str() == Some(slug.as_str()))
        else {
            continue;
        };
        let main = bot["mainThreadId"].as_str().unwrap_or("");
        let provisioned = !main.is_empty();
        let mut thread = room["threads"][&slug].as_str().unwrap_or("").to_string();
        if crate::chat::rooms::Rooms::group_thread_missing(&thread, main) {
            thread.clear();
        }
        if thread.is_empty() && ensure && provisioned {
            if let (Some(provisioning), Some(project)) = (&provisioning, bot["projectId"].as_str())
            {
                let model = [&bot["model"], &settings["defaultModel"]]
                    .into_iter()
                    .find(|m| m.is_object())
                    .cloned()
                    .unwrap_or_else(crate::bots::provisioner::default_model);
                let id = uuid::Uuid::new_v4().to_string();
                let launched = provisioning
                    .launch(&crate::harness::LaunchThread {
                        thread_id: id.clone(),
                        project_id: project.to_string(),
                        title: room["title"].as_str().unwrap_or("").to_string(),
                        model,
                        runtime_mode: crate::bots::provisioner::launch_runtime_mode(bot, &settings)
                            .to_string(),
                    })
                    .await;
                match launched {
                    Ok(_) => {
                        s.rooms.set_thread(&room_id, &slug, &id).await?;
                        thread = id;
                    }
                    Err(err) => tracing::warn!("group thread for {slug} failed: {err}"),
                }
            }
        }
        members.push(crate::chat::rooms::Member {
            slug,
            title: bot["name"].as_str().unwrap_or("Bot").to_string(),
            thread_id: thread,
            working: false,
            provisioned,
        });
    }
    Ok(members)
}
