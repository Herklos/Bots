use super::*;

pub(crate) async fn computer_get(
    State(s): S,
    Path(slug): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let thread = main_thread(&s, &slug).await?;
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    let listed = rpc
        .call("preview.list", json!({ "threadId": thread }))
        .await
        .map_err(|e| ApiError::new(502, format!("T3 didn't list pages: {e}")))?;
    let tabs = computer::tabs_for_screen(&listed);
    let asked = q.get("tab").map(String::as_str);
    let selected = computer::frame_tab(&tabs, asked)
        .and_then(|tab| tab.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut image = String::new();
    let mut error = String::new();
    // A held page keeps the JPEG from its interactive session. Watching that
    // same tab does not open a second socket.
    let held = {
        let sessions = computer::sessions()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        sessions.picture(&slug, &selected)
    };
    match held {
        Some(frame) => image = frame,
        None if !selected.is_empty() => {
            let size = if q.get("view").is_some_and(|view| view == "large") {
                computer::WatchSize::Large
            } else {
                computer::WatchSize::Card
            };
            match computer::one_frame(&s.t3, &thread, &selected, size).await {
                Ok(bytes) => image = library::encode_file(&bytes),
                Err(reason) => error = reason,
            }
        }
        None => {}
    }
    // A frame the app already has is not sent again.
    let tag = computer::etag(&image);
    let unchanged = !image.is_empty() && q.get("since").is_some_and(|s| *s == tag);
    Ok(Json(json!({
        "ok": true,
        "tabs": tabs,
        "image": if unchanged { String::new() } else { image },
        "etag": tag,
        "unchanged": unchanged,
        "selected": selected,
        "error": error,
    })))
}

/// Steers the Bot to open a server browser tab. Does not wait for the tab to appear.
pub(crate) async fn computer_open(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    computer_steer(&s, &slug, computer::open_command).await
}

/// Steers the Bot to close its server browser tabs. Does not wait for them to go.
pub(crate) async fn computer_close(
    State(s): S,
    Path(slug): Path<String>,
) -> ApiResult<Json<Value>> {
    computer_steer(&s, &slug, computer::close_command).await
}

pub(crate) async fn computer_steer(
    s: &AppState,
    slug: &str,
    command: fn(&str) -> Value,
) -> ApiResult<Json<Value>> {
    let thread = main_thread(s, slug).await?;
    let command = command(&thread);
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    rpc.call("orchestration.dispatchCommand", command.clone())
        .await
        .map_err(|e| ApiError::new(502, e.to_string()))?;
    Ok(Json(
        json!({ "ok": true, "messageId": command["messageId"] }),
    ))
}

pub(crate) async fn computer_control(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let thread = main_thread(&s, &slug).await?;
    let body = parse(&body)?;
    let tab = body["tabId"].as_str().unwrap_or("");
    let action = body["action"].as_str().unwrap_or("");
    if tab.is_empty() {
        return Err(ApiError::bad_request("Pick a page first."));
    }
    let frame = match action {
        "take" => hold::take(&s.t3, &thread, &slug, tab).await,
        "release" if hold::release(&slug).await => Ok(Vec::new()),
        _ => computer::send_control(&s.t3, &thread, tab, action).await,
    }
    .map_err(ApiError::bad_request)?;
    let mut sessions = computer::sessions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    match action {
        "take" => {
            let width = u32::try_from(body["width"].as_u64().unwrap_or(0)).unwrap_or(0);
            let height = u32::try_from(body["height"].as_u64().unwrap_or(0)).unwrap_or(0);
            sessions.take(&slug, tab, width, height);
            if !frame.is_empty() {
                sessions.note_image(&slug, &library::encode_file(&frame));
                if let Some((w, h)) = computer::jpeg_size(&frame) {
                    sessions.remember_size(&slug, w, h);
                }
            }
        }
        "release" => sessions.release(&slug),
        _ => {}
    }
    Ok(Json(json!({ "ok": true })))
}

/// Input on the held socket; a fresh socket only when none is live.
pub(crate) async fn held_input(
    s: &AppState,
    slug: &str,
    thread: &str,
    tab: &str,
    messages: &[Value],
) -> Result<computer::Driven, String> {
    match hold::input(slug, tab, messages).await {
        Some(image) => Ok(computer::Driven {
            image,
            clipboard: String::new(),
        }),
        None => computer::send_input(&s.t3, thread, tab, messages).await,
    }
}

pub(crate) async fn computer_input(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let thread = main_thread(&s, &slug).await?;
    let body = parse(&body)?;
    let hold = {
        let sessions = computer::sessions()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        sessions.get(&slug).cloned()
    };
    let Some(hold) = hold else {
        return Err(ApiError::bad_request("Take control first."));
    };
    let width = body["width"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0)
        .unwrap_or(hold.width);
    let height = body["height"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0)
        .unwrap_or(hold.height);
    let box_w = num(&body, "boxW");
    let box_h = num(&body, "boxH");
    let (x, y) = if box_w > 0.0 && box_h > 0.0 {
        match computer::contain_fractions(
            num(&body, "x"),
            num(&body, "y"),
            box_w,
            box_h,
            width,
            height,
        ) {
            Ok(Some(point)) => point,
            Ok(None) => return Ok(Json(json!({ "ok": true }))),
            Err(reason) => return Err(ApiError::bad_request(reason)),
        }
    } else {
        (num(&body, "x"), num(&body, "y"))
    };
    let kind = body["kind"].as_str().unwrap_or("");
    let driven = match kind {
        "click" => {
            let messages =
                computer::click_messages(x, y, width, height).map_err(ApiError::bad_request)?;
            held_input(&s, &slug, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        "wheel" => {
            let messages = vec![
                computer::wheel_message(x, y, width, height, num(&body, "deltaY"))
                    .map_err(ApiError::bad_request)?,
            ];
            held_input(&s, &slug, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        "text" => {
            let messages = vec![computer::text_message(body["text"].as_str().unwrap_or(""))
                .map_err(ApiError::bad_request)?];
            held_input(&s, &slug, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        "key" => {
            let modifiers = match body.get("modifiers") {
                None | Some(serde_json::Value::Null) => 0,
                Some(value) => value
                    .as_i64()
                    .ok_or_else(|| ApiError::bad_request("Those modifiers are not sent."))?,
            };
            let message = computer::key_message(body["key"].as_str().unwrap_or(""), modifiers)
                .map_err(ApiError::bad_request)?;
            if let Some(sent) = body.get("keyCode").filter(|value| !value.is_null()) {
                let sent = sent
                    .as_i64()
                    .ok_or_else(|| ApiError::bad_request("That key is not sent."))?;
                if sent != message["keyCode"].as_i64().unwrap_or(sent) {
                    return Err(ApiError::bad_request("That key is not sent."));
                }
            }
            let messages = vec![message];
            held_input(&s, &slug, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        "copy" | "cut" => computer::send_edit(&s.t3, &thread, &hold.tab, kind)
            .await
            .map_err(ApiError::bad_request)?,
        "selectAll" => {
            let messages =
                vec![computer::edit_message("selectAll").map_err(ApiError::bad_request)?];
            held_input(&s, &slug, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        _ => {
            return Err(ApiError::bad_request(
                "Say click, wheel, text, key, copy, cut, or selectAll.",
            ))
        }
    };
    let mut sessions = computer::sessions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if sessions.get(&slug).is_some() {
        if width > 0 && height > 0 {
            sessions.take(&slug, &hold.tab, width, height);
        }
        if !driven.image.is_empty() {
            sessions.note_image(&slug, &library::encode_file(&driven.image));
            if let Some((w, h)) = computer::jpeg_size(&driven.image) {
                sessions.remember_size(&slug, w, h);
            }
        }
    }
    Ok(Json(json!({ "ok": true, "text": driven.clipboard })))
}

pub(crate) fn num(body: &Value, key: &str) -> f64 {
    body.get(key).and_then(Value::as_f64).unwrap_or(f64::NAN)
}
