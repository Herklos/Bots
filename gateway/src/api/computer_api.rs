use super::*;

pub(crate) async fn computer_get(
    State(s): S,
    Path(slug): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let thread = main_thread(&s, &slug).await?;
    let browser = s.harness.browser().map_err(|err| err.api())?;
    let tabs = browser
        .pages(&thread)
        .await
        .map_err(|err| err.api_note(|msg| format!("T3 didn't list pages: {msg}")))?;
    let asked = q.get("tab").map(String::as_str);
    let selected = crate::bots::computer::frame_tab(&tabs, asked)
        .and_then(|tab| tab.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut image = String::new();
    let mut error = String::new();
    // A held page keeps the JPEG from its interactive session. Watching that
    // same tab does not open a second socket. The reader on that socket is
    // newer than the frame stored at take, so the poll shows the live page.
    let held = {
        let sessions = crate::bots::computer::sessions()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        sessions.picture(&slug, &selected)
    };
    match held {
        Some(frame) => {
            image = match browser.held_frame(&slug).await {
                Some(bytes) if !bytes.is_empty() => crate::bots::library::encode_file(&bytes),
                _ => frame,
            };
        }
        None if !selected.is_empty() => {
            let large = q.get("view").is_some_and(|view| view == "large");
            match browser
                .frame(&thread, &selected, large)
                .await
                .map_err(|err| err.to_string())
            {
                Ok(bytes) => image = crate::bots::library::encode_file(&bytes),
                Err(reason) => error = reason,
            }
        }
        None => {}
    }
    // A frame the app already has is not sent again.
    let tag = crate::bots::computer::etag(&image);
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
    let thread = main_thread(&s, &slug).await?;
    let text = crate::bots::computer::open_command(&thread)["text"]
        .as_str()
        .unwrap_or(crate::bots::computer::OPEN_PROMPT)
        .to_string();
    steer_text(&s, &slug, &thread, &text).await
}

/// Steers the Bot to close its server browser tabs. Does not wait for them to go.
pub(crate) async fn computer_close(
    State(s): S,
    Path(slug): Path<String>,
) -> ApiResult<Json<Value>> {
    let thread = main_thread(&s, &slug).await?;
    let text = crate::bots::computer::close_command(&thread)["text"]
        .as_str()
        .unwrap_or(crate::bots::computer::CLOSE_PROMPT)
        .to_string();
    steer_text(&s, &slug, &thread, &text).await
}

async fn steer_text(s: &AppState, slug: &str, thread: &str, text: &str) -> ApiResult<Json<Value>> {
    let bot = require_bot(s, slug).await?;
    crate::bots::provisioner::limit_existing_thread(s, &bot, thread)
        .await
        .map_err(|err| err.api())?;
    let message_id = s
        .harness
        .turns()
        .map_err(|err| err.api())?
        .steer(thread, text)
        .await
        .map_err(|err| err.api())?;
    Ok(Json(json!({ "ok": true, "messageId": message_id })))
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
    let width = u32::try_from(body["width"].as_u64().unwrap_or(0)).unwrap_or(0);
    let height = u32::try_from(body["height"].as_u64().unwrap_or(0)).unwrap_or(0);
    // The reader stores frames during take. The session has to exist first,
    // or those frames are dropped and the take-time JPEG overwrites them.
    if action == "take" {
        crate::bots::computer::sessions()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take(&slug, tab, width, height);
    }
    let browser = s.harness.browser().map_err(|err| err.api())?;
    let frame = match action {
        "take" => match browser.take(&thread, &slug, tab).await {
            Ok(bytes) => Ok(bytes),
            Err(err) => {
                // A failed first take leaves no socket. A failed re-take leaves the old one.
                if browser.held_frame(&slug).await.is_none() {
                    crate::bots::computer::sessions()
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .release(&slug);
                }
                Err(err.to_string())
            }
        },
        "release" if browser.release(&slug).await => Ok(Vec::new()),
        _ => browser
            .control(&thread, tab, action)
            .await
            .map_err(|err| err.to_string()),
    }
    .map_err(ApiError::bad_request)?;
    let live = if action == "take" {
        browser.held_frame(&slug).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut sessions = crate::bots::computer::sessions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    match action {
        "take" => {
            sessions.take(&slug, tab, width, height);
            // The reader may already have stored a newer frame. Do not put the
            // take-time JPEG back on top of it.
            if sessions
                .picture(&slug, tab)
                .is_some_and(|image| image.is_empty())
            {
                let bytes = if live.is_empty() { &frame } else { &live };
                if !bytes.is_empty() {
                    sessions.note_image(&slug, &crate::bots::library::encode_file(bytes));
                    if let Some((w, h)) = crate::bots::computer::jpeg_size(bytes) {
                        sessions.remember_size(&slug, w, h);
                    }
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
) -> Result<crate::harness::Frame, String> {
    let browser = s.harness.browser().map_err(|err| err.to_string())?;
    match browser.held_input(slug, tab, messages).await {
        Some(image) => Ok(crate::harness::Frame {
            bytes: image,
            clipboard: String::new(),
        }),
        None => browser
            .input(thread, tab, messages)
            .await
            .map_err(|err| err.to_string()),
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
        let sessions = crate::bots::computer::sessions()
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
        match crate::bots::computer::contain_fractions(
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
            let messages = crate::bots::computer::click_messages(x, y, width, height)
                .map_err(ApiError::bad_request)?;
            held_input(&s, &slug, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        "wheel" => {
            let messages = vec![crate::bots::computer::wheel_message(
                x,
                y,
                width,
                height,
                num(&body, "deltaY"),
            )
            .map_err(ApiError::bad_request)?];
            held_input(&s, &slug, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        "text" => {
            let messages =
                vec![
                    crate::bots::computer::text_message(body["text"].as_str().unwrap_or(""))
                        .map_err(ApiError::bad_request)?,
                ];
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
            let message =
                crate::bots::computer::key_message(body["key"].as_str().unwrap_or(""), modifiers)
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
        "copy" | "cut" => {
            let browser = s.harness.browser().map_err(|err| err.api())?;
            browser
                .edit(&thread, &hold.tab, kind)
                .await
                .map_err(|err| ApiError::bad_request(err.to_string()))?
        }
        "selectAll" => {
            let messages =
                vec![crate::bots::computer::edit_message("selectAll")
                    .map_err(ApiError::bad_request)?];
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
    let mut sessions = crate::bots::computer::sessions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if sessions.get(&slug).is_some() {
        if width > 0 && height > 0 {
            sessions.take(&slug, &hold.tab, width, height);
        }
        if !driven.bytes.is_empty() {
            sessions.note_image(&slug, &crate::bots::library::encode_file(&driven.bytes));
            if let Some((w, h)) = crate::bots::computer::jpeg_size(&driven.bytes) {
                sessions.remember_size(&slug, w, h);
            }
        }
    }
    Ok(Json(json!({ "ok": true, "text": driven.clipboard })))
}

pub(crate) fn num(body: &Value, key: &str) -> f64 {
    body.get(key).and_then(Value::as_f64).unwrap_or(f64::NAN)
}
