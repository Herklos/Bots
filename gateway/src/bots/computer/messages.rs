use super::*;

pub fn watch_message() -> Value {
    json!({ "type": "ack" })
}

/// Framed prompt the Computer empty-state button dispatches. Chat hides this turn.
pub const OPEN_PROMPT: &str = "\
[Computer]
Open your browser where you left off if you still have that page. Otherwise open https://www.google.com with your browser tools. Do this without asking, and do not reply in chat.";

/// Framed prompt the small-screen × dispatches. Chat hides this turn.
pub const CLOSE_PROMPT: &str = "\
[Computer]
Close your browser tabs with your browser tools. Do this without asking, and do not reply in chat.";

pub(crate) fn steer_command(thread: &str, text: &str) -> Value {
    let id = uuid::Uuid::new_v4().to_string();
    json!({
        "commandId": id,
        "type": "message.dispatch",
        "threadId": thread,
        "messageId": id,
        "text": text,
        "attachments": [],
        "createdBy": "user",
        "creationSource": "web",
        "deliveryIntent": "steer",
        "dispatchMode": { "type": "start_immediately" },
    })
}

/// `message.dispatch` that steers the Bot to open a server browser tab.
pub fn open_command(thread: &str) -> Value {
    steer_command(thread, OPEN_PROMPT)
}

/// `message.dispatch` that steers the Bot to close its server browser tabs.
pub fn close_command(thread: &str) -> Value {
    steer_command(thread, CLOSE_PROMPT)
}

pub fn control_message(action: &str) -> Result<Value, &'static str> {
    match action {
        "take" => Ok(json!({ "type": "takeControl" })),
        "release" => Ok(json!({ "type": "releaseControl" })),
        _ => Err("Say take or release."),
    }
}

/// A short fingerprint of a frame, so a poll that finds the same page need not carry it again.
pub fn etag(image: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    image.hash(&mut h);
    format!("{:x}", h.finish())
}

/// Which listed server tab a watch frames. An unknown id falls back to the first.
pub fn frame_tab<'a>(tabs: &'a [Value], asked: Option<&str>) -> Option<&'a Value> {
    if let Some(id) = asked.map(str::trim).filter(|id| !id.is_empty()) {
        if let Some(hit) = tabs
            .iter()
            .find(|tab| tab.get("id").and_then(Value::as_str) == Some(id))
        {
            return Some(hit);
        }
    }
    tabs.first()
}

pub(crate) fn frame_px(fraction: f64, span: u32) -> Result<i64, &'static str> {
    if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) || span == 0 {
        return Err("That point is off the page.");
    }
    let px = (fraction * f64::from(span)).round();
    // `fraction` is in 0..=1 and `span` fits in `i64`, so the rounded pixel does too.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "rounded pixel is finite and at most the frame span, which fits in i64"
    )]
    Ok(px as i64)
}

/// A click in the streamed frame. Fractions are of that JPEG, and the click does not take control.
pub fn click_messages(x: f64, y: f64, width: u32, height: u32) -> Result<Vec<Value>, &'static str> {
    let x = frame_px(x, width)?;
    let y = frame_px(y, height)?;
    let at = |action: &str, button: &str, buttons: i64, clicks: i64| {
        json!({
            "type": "mouse",
            "action": action,
            "x": x,
            "y": y,
            "button": button,
            "buttons": buttons,
            "clickCount": clicks,
            "modifiers": 0,
        })
    };
    Ok(vec![
        at("move", "none", 0, 0),
        at("down", "left", 1, 1),
        at("up", "left", 0, 1),
    ])
}

/// Where a CSS point lands in a picture drawn with `object-fit: contain`.
/// `Ok(None)` is the letterbox around that picture: nothing is sent.
/// Fractions are of the JPEG, which is what the preview stream addresses.
pub fn contain_fractions(
    x: f64,
    y: f64,
    box_w: f64,
    box_h: f64,
    image_w: u32,
    image_h: u32,
) -> Result<Option<(f64, f64)>, &'static str> {
    if !x.is_finite() || !y.is_finite() || !box_w.is_finite() || !box_h.is_finite() {
        return Err("That point is off the page.");
    }
    if box_w <= 0.0 || box_h <= 0.0 || image_w == 0 || image_h == 0 {
        return Err("The picture has no size yet.");
    }
    let image_w = f64::from(image_w);
    let image_h = f64::from(image_h);
    let scale = (box_w / image_w).min(box_h / image_h);
    if !scale.is_finite() || scale <= 0.0 {
        return Err("The picture has no size yet.");
    }
    let drawn_w = image_w * scale;
    let drawn_h = image_h * scale;
    let origin_x = (box_w - drawn_w) / 2.0;
    let origin_y = (box_h - drawn_h) / 2.0;
    let fx = (x - origin_x) / drawn_w;
    let fy = (y - origin_y) / drawn_h;
    if !(0.0..=1.0).contains(&fx) || !(0.0..=1.0).contains(&fy) {
        return Ok(None);
    }
    Ok(Some((fx, fy)))
}

/// Width and height from a JPEG SOF marker, in pixels.
pub fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut i = 0;
    while i + 9 < bytes.len() {
        if bytes[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        if marker == 0xd8 || marker == 0xd9 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            i += 2;
            continue;
        }
        if i + 4 >= bytes.len() {
            return None;
        }
        let len = usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]));
        if marker == 0xc0 || marker == 0xc1 || marker == 0xc2 {
            let height = u32::from(u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]));
            let width = u32::from(u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]));
            if width == 0 || height == 0 {
                return None;
            }
            return Some((width, height));
        }
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

/// A scroll in the same frame. `delta_y` is the wheel distance, not a fraction.
pub fn wheel_message(
    x: f64,
    y: f64,
    width: u32,
    height: u32,
    delta_y: f64,
) -> Result<Value, &'static str> {
    if !delta_y.is_finite() {
        return Err("That scroll is not a distance.");
    }
    Ok(json!({
        "type": "wheel",
        "x": frame_px(x, width)?,
        "y": frame_px(y, height)?,
        "deltaX": 0,
        "deltaY": delta_y,
        "modifiers": 0,
    }))
}

pub const TEXT_CAP: usize = 8192;

/// Typed text for the held page. The value is the message, and it is not logged here.
pub fn text_message(text: &str) -> Result<Value, &'static str> {
    if text.is_empty() {
        return Err("Nothing to type.");
    }
    if text.len() > TEXT_CAP {
        return Err("That text is too long.");
    }
    Ok(json!({ "type": "text", "text": text }))
}

/// CDP modifier bits. Meta (4) matches T3's editingCommand for copy, cut and select-all.
pub const ALT: i64 = 1;

pub const CTRL: i64 = 2;

pub const META: i64 = 4;

pub const SHIFT: i64 = 8;

/// Shift, Alt and Ctrl. Meta stays on the copy, cut and select-all path.
pub(crate) const KEY_MODIFIERS: i64 = ALT | CTRL | SHIFT;

/// A navigation or editing key for T3's server browser stream (CDP-shaped).
/// Escape is the phone's back gesture, so it is not sent.
/// `modifiers` is the CDP bit field: Alt 1, Ctrl 2, Shift 8.
pub fn key_message(key: &str, modifiers: i64) -> Result<Value, &'static str> {
    if modifiers < 0 || (modifiers & !KEY_MODIFIERS) != 0 {
        return Err("Those modifiers are not sent.");
    }
    let (key, code, key_code) = match key {
        "ArrowUp" => ("ArrowUp", "ArrowUp", 38),
        "ArrowDown" => ("ArrowDown", "ArrowDown", 40),
        "ArrowLeft" => ("ArrowLeft", "ArrowLeft", 37),
        "ArrowRight" => ("ArrowRight", "ArrowRight", 39),
        "Tab" => ("Tab", "Tab", 9),
        "Enter" => ("Enter", "Enter", 13),
        "Backspace" => ("Backspace", "Backspace", 8),
        "Delete" => ("Delete", "Delete", 46),
        "Home" => ("Home", "Home", 36),
        "End" => ("End", "End", 35),
        "PageUp" => ("PageUp", "PageUp", 33),
        "PageDown" => ("PageDown", "PageDown", 34),
        _ => return Err("That key is not sent."),
    };
    Ok(json!({
        "type": "key",
        "key": key,
        "code": code,
        "keyCode": key_code,
        "modifiers": modifiers,
    }))
}

/// Copy, cut or select-all on the held page. The clipboard text comes back on the same socket.
pub fn edit_message(action: &str) -> Result<Value, &'static str> {
    let (key, code) = match action {
        "copy" => ("c", "KeyC"),
        "cut" => ("x", "KeyX"),
        "selectAll" => ("a", "KeyA"),
        _ => return Err("Say copy, cut, or selectAll."),
    };
    Ok(json!({
        "type": "key",
        "key": key,
        "code": code,
        "modifiers": META,
    }))
}
