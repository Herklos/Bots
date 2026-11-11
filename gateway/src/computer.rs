//! The Bot's computer: T3's headless browser. Watching is read-only.
//! Take control is a separate action and is released when the person leaves it.
use crate::t3::T3;
use crate::vault_tools::{server_tabs, Tab};
use serde_json::{json, Value};
use std::time::Duration;

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

fn steer_command(thread: &str, text: &str) -> Value {
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

fn frame_px(fraction: f64, span: u32) -> Result<i64, &'static str> {
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

/// A navigation or editing key for T3's server browser stream (CDP-shaped).
/// Escape is the phone's back gesture, so it is not sent.
pub fn key_message(key: &str) -> Result<Value, &'static str> {
    let (key, code) = match key {
        "ArrowUp" => ("ArrowUp", "ArrowUp"),
        "ArrowDown" => ("ArrowDown", "ArrowDown"),
        "ArrowLeft" => ("ArrowLeft", "ArrowLeft"),
        "ArrowRight" => ("ArrowRight", "ArrowRight"),
        "Tab" => ("Tab", "Tab"),
        "Enter" => ("Enter", "Enter"),
        "Backspace" => ("Backspace", "Backspace"),
        "Delete" => ("Delete", "Delete"),
        "Home" => ("Home", "Home"),
        "End" => ("End", "End"),
        "PageUp" => ("PageUp", "PageUp"),
        "PageDown" => ("PageDown", "PageDown"),
        _ => return Err("That key is not sent."),
    };
    Ok(json!({ "type": "key", "key": key, "code": code, "modifiers": 0 }))
}

/// Meta (4) matches T3's editingCommand for copy, cut and select-all on the server browser.
pub const META: i64 = 4;

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

/// One held browser per Bot. Release drops it. A click does not open one.
/// `image` is the last JPEG from that interactive session, so a later watch
/// can show it without opening a second socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hold {
    pub tab: String,
    pub width: u32,
    pub height: u32,
    pub image: String,
}

#[derive(Default)]
pub struct Sessions {
    holds: std::collections::HashMap<String, Hold>,
}

impl Sessions {
    pub fn take(&mut self, slug: &str, tab: &str, width: u32, height: u32) {
        let previous = self
            .holds
            .get(slug)
            .filter(|hold| hold.tab == tab)
            .map(|hold| (hold.image.clone(), hold.width, hold.height));
        let (image, kept_w, kept_h) = previous.unwrap_or_default();
        self.holds.insert(
            slug.to_string(),
            Hold {
                tab: tab.to_string(),
                width: if width > 0 { width } else { kept_w },
                height: if height > 0 { height } else { kept_h },
                image,
            },
        );
    }

    /// The JPEG's pixel size, once a frame has arrived. A later take of the same
    /// page keeps this when the caller has no new size.
    pub fn remember_size(&mut self, slug: &str, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if let Some(hold) = self.holds.get_mut(slug) {
            hold.width = width;
            hold.height = height;
        }
    }

    pub fn release(&mut self, slug: &str) {
        self.holds.remove(slug);
    }

    pub fn get(&self, slug: &str) -> Option<&Hold> {
        self.holds.get(slug)
    }

    /// The picture from the held session, when `tab` is the page being driven.
    /// `Some("")` means the page is held and no frame has arrived yet.
    pub fn picture(&self, slug: &str, tab: &str) -> Option<String> {
        let hold = self.holds.get(slug)?;
        if hold.tab != tab {
            return None;
        }
        Some(hold.image.clone())
    }

    pub fn note_image(&mut self, slug: &str, image: &str) {
        if image.is_empty() {
            return;
        }
        if let Some(hold) = self.holds.get_mut(slug) {
            hold.image = image.to_string();
        }
    }
}

pub fn sessions() -> &'static std::sync::Mutex<Sessions> {
    static SESSIONS: std::sync::OnceLock<std::sync::Mutex<Sessions>> = std::sync::OnceLock::new();
    SESSIONS.get_or_init(|| std::sync::Mutex::new(Sessions::default()))
}

/// Tabs the watch screen may show. Other runtimes are left out.
pub fn tabs_for_screen(list: &Value) -> Vec<Value> {
    server_tabs(list)
        .into_iter()
        .map(|Tab { tab_id, url }| {
            let title = url::Url::parse(&url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .filter(|h| !h.is_empty())
                .unwrap_or_else(|| {
                    if url.is_empty() {
                        "Page".into()
                    } else {
                        url.clone()
                    }
                });
            json!({ "id": tab_id, "title": title, "url": url })
        })
        .collect()
}

/// One JPEG from the preview stream, without taking control.
pub async fn one_frame(t3: &T3, thread: &str, tab_id: &str) -> Result<Vec<u8>, String> {
    let mut ws = connect(t3, thread, tab_id, false).await?;
    read_one_jpeg(&mut ws).await
}

pub async fn send_control(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    action: &str,
) -> Result<Vec<u8>, String> {
    let message = control_message(action).map_err(str::to_string)?;
    deliver(t3, thread, tab_id, &[message]).await
}

/// What one interactive turn returns: the next JPEG, and clipboard text after copy/cut.
#[derive(Default)]
pub struct Driven {
    pub image: Vec<u8>,
    pub clipboard: String,
}

/// Input on the held page. The messages are the click, scroll, text, or key, not a new take.
/// Take runs on this same socket first, so the page stays owned for the turn.
pub async fn send_input(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    messages: &[Value],
) -> Result<Driven, String> {
    if messages
        .iter()
        .any(|message| message.get("type").and_then(Value::as_str) == Some("takeControl"))
    {
        return Err("A click does not take control.".into());
    }
    drive(t3, thread, tab_id, messages, false).await
}

/// Copy or cut on the held page. Waits for T3's clipboard event on the same socket.
pub async fn send_edit(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    action: &str,
) -> Result<Driven, String> {
    let message = edit_message(action).map_err(str::to_string)?;
    drive(t3, thread, tab_id, &[message], true).await
}

/// Take or release only. Does not prepend another take.
async fn deliver(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    messages: &[Value],
) -> Result<Vec<u8>, String> {
    use futures_util::SinkExt;
    let mut ws = connect(t3, thread, tab_id, true).await?;
    for message in messages {
        ws.send(tokio_tungstenite::tungstenite::Message::text(
            message.to_string(),
        ))
        .await
        .map_err(|_| "The browser closed the connection.".to_string())?;
    }
    // A take is held only after the stream says this token can operate.
    if messages
        .iter()
        .any(|message| message.get("type").and_then(Value::as_str) == Some("takeControl"))
    {
        wait_for_control(&mut ws).await?;
    }
    match tokio::time::timeout(Duration::from_millis(800), read_one_jpeg(&mut ws)).await {
        Ok(Ok(bytes)) => Ok(bytes),
        _ => Ok(Vec::new()),
    }
}

/// One interactive turn: take on this socket, send the messages, then a frame and optional clipboard.
async fn drive(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    messages: &[Value],
    want_clipboard: bool,
) -> Result<Driven, String> {
    use futures_util::SinkExt;
    let mut ws = connect(t3, thread, tab_id, true).await?;
    ws.send(tokio_tungstenite::tungstenite::Message::text(
        control_message("take").map_err(str::to_string)?.to_string(),
    ))
    .await
    .map_err(|_| "The browser closed the connection.".to_string())?;
    wait_for_control(&mut ws).await?;
    for message in messages {
        ws.send(tokio_tungstenite::tungstenite::Message::text(
            message.to_string(),
        ))
        .await
        .map_err(|_| "The browser closed the connection.".to_string())?;
    }
    if want_clipboard {
        let clipboard = wait_for_clipboard(&mut ws).await?;
        let image =
            match tokio::time::timeout(Duration::from_millis(400), read_one_jpeg(&mut ws)).await {
                Ok(Ok(bytes)) => bytes,
                _ => Vec::new(),
            };
        return Ok(Driven { image, clipboard });
    }
    let image = match tokio::time::timeout(Duration::from_millis(800), read_one_jpeg(&mut ws)).await
    {
        Ok(Ok(bytes)) => bytes,
        _ => Vec::new(),
    };
    Ok(Driven {
        image,
        clipboard: String::new(),
    })
}

async fn connect(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    interactive: bool,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    String,
> {
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue};
    let base = t3.base_url.trim_end_matches('/');
    let ws_base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_string()
    };
    let mut url = url::Url::parse(&format!("{ws_base}/api/preview-stream/ws"))
        .map_err(|_| "Couldn't open the browser stream.".to_string())?;
    url.query_pairs_mut()
        .append_pair("threadId", thread)
        .append_pair("tabId", tab_id)
        .append_pair("interactive", if interactive { "true" } else { "false" })
        .append_pair("maxWidth", "960")
        .append_pair("maxHeight", "540")
        .append_pair("quality", "60");
    let mut req = url
        .as_str()
        .into_client_request()
        .map_err(|_| "Couldn't open the browser stream.".to_string())?;
    let auth = HeaderValue::from_str(&format!("Bearer {}", t3.token))
        .map_err(|_| "Couldn't open the browser stream.".to_string())?;
    req.headers_mut().insert("authorization", auth);
    match tokio_tungstenite::connect_async(req).await {
        Ok((ws, _)) => Ok(ws),
        Err(_) => Err("This Bot has not opened a page.".into()),
    }
}

type PreviewWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn read_one_jpeg(ws: &mut PreviewWs) -> Result<Vec<u8>, String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let frame = tokio::time::timeout_at(deadline, ws.next())
            .await
            .map_err(|_| "The picture didn't arrive.".to_string())?
            .ok_or_else(|| "The browser closed the connection.".to_string())?
            .map_err(|_| "The browser closed the connection.".to_string())?;
        match frame {
            Message::Binary(bytes) => {
                let _ = ws.send(Message::text(watch_message().to_string())).await;
                return Ok(bytes.to_vec());
            }
            Message::Text(_) => continue,
            _ => continue,
        }
    }
    Err("The picture didn't arrive.".into())
}

const CANNOT_OPERATE: &str =
    "hbots's T3 token can't operate the browser. Run hbots up --refresh-token.";

/// What one `control` frame means. `Ready` is the only outcome that may mark a take held.
enum ControlFrame {
    Ignore,
    Ready,
    Denied(&'static str),
}

fn control_frame(msg: &Value) -> ControlFrame {
    if msg.get("type").and_then(Value::as_str) != Some("control") {
        return ControlFrame::Ignore;
    }
    if msg["canOperate"] == false {
        return ControlFrame::Denied(CANNOT_OPERATE);
    }
    let controller = msg.get("controller").and_then(Value::as_str).unwrap_or("");
    if controller == "another-viewer" {
        return ControlFrame::Denied("Someone else has this page.");
    }
    if controller == "you" || controller == "unclaimed" || controller == "none" {
        return ControlFrame::Ready;
    }
    ControlFrame::Ignore
}

/// Record a take only when the control frame says this viewer may operate.
#[cfg(test)]
fn hold_if_ready(
    sessions: &mut Sessions,
    slug: &str,
    tab: &str,
    msg: &Value,
) -> Result<(), &'static str> {
    match control_frame(msg) {
        ControlFrame::Ready => {
            sessions.take(slug, tab, 0, 0);
            Ok(())
        }
        ControlFrame::Denied(reason) => Err(reason),
        ControlFrame::Ignore => Err("The page didn't confirm control."),
    }
}

/// The page is ready for input once this viewer owns it (or nobody else does).
async fn wait_for_control(ws: &mut PreviewWs) -> Result<(), String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let frame = match tokio::time::timeout_at(deadline, ws.next()).await {
            Ok(Some(Ok(frame))) => frame,
            Ok(Some(Err(_))) | Ok(None) => return Err("The browser closed the connection.".into()),
            Err(_) => break,
        };
        match frame {
            Message::Binary(_) => {
                let _ = ws.send(Message::text(watch_message().to_string())).await;
            }
            Message::Text(text) => {
                let Ok(msg) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                match control_frame(&msg) {
                    ControlFrame::Ignore => continue,
                    ControlFrame::Ready => return Ok(()),
                    ControlFrame::Denied(reason) => return Err(reason.to_string()),
                }
            }
            _ => {}
        }
    }
    Err("The page didn't confirm control.".into())
}

async fn wait_for_clipboard(ws: &mut PreviewWs) -> Result<String, String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let frame = match tokio::time::timeout_at(deadline, ws.next()).await {
            Ok(Some(Ok(frame))) => frame,
            Ok(Some(Err(_))) | Ok(None) => return Err("The browser closed the connection.".into()),
            Err(_) => break,
        };
        match frame {
            Message::Binary(_) => {
                let _ = ws.send(Message::text(watch_message().to_string())).await;
            }
            Message::Text(text) => {
                let Ok(msg) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                if msg.get("type").and_then(Value::as_str) == Some("clipboard") {
                    let clip = msg
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    if clip.is_empty() {
                        return Err("Nothing was copied.".into());
                    }
                    return Ok(clip);
                }
            }
            _ => {}
        }
    }
    Err("Nothing was copied.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_can_operate_false_is_that_error_and_a_take_is_not_held() {
        let denied = json!({ "type": "control", "controller": "you", "canOperate": false });
        let mut sessions = Sessions::default();
        let err = hold_if_ready(&mut sessions, "ada", "tab-1", &denied).unwrap_err();
        assert_eq!(err, CANNOT_OPERATE);
        assert!(sessions.get("ada").is_none());

        let allowed = json!({ "type": "control", "controller": "you", "canOperate": true });
        hold_if_ready(&mut sessions, "ada", "tab-1", &allowed).unwrap();
        assert_eq!(
            sessions.get("ada").map(|hold| hold.tab.as_str()),
            Some("tab-1")
        );
    }

    #[test]
    fn watch_acks_and_does_not_take_control() {
        assert_eq!(watch_message()["type"], "ack");
        assert_ne!(watch_message()["type"], "takeControl");
        assert_eq!(control_message("take").unwrap()["type"], "takeControl");
        assert_eq!(
            control_message("release").unwrap()["type"],
            "releaseControl"
        );
        assert!(control_message("steal").is_err());
    }

    #[test]
    fn open_command_steers_with_the_computer_frame() {
        let command = open_command("thread-ada");
        assert_eq!(command["type"], "message.dispatch");
        assert_eq!(command["threadId"], "thread-ada");
        assert_eq!(command["createdBy"], "user");
        assert_eq!(command["creationSource"], "web");
        assert_eq!(command["deliveryIntent"], "steer");
        assert_eq!(command["dispatchMode"]["type"], "start_immediately");
        assert_eq!(command["attachments"], json!([]));
        let text = command["text"].as_str().unwrap();
        assert!(text.contains("[Computer]"));
        assert!(text.contains("https://www.google.com"));
        assert!(text.contains("where you left off") || text.contains("left off"));
        assert_eq!(text, OPEN_PROMPT);
        assert_eq!(command["commandId"].as_str(), command["messageId"].as_str());
        assert!(!command["messageId"].as_str().unwrap_or("").is_empty());
    }

    #[test]
    fn close_command_steers_with_the_computer_frame() {
        let command = close_command("thread-ada");
        assert_eq!(command["type"], "message.dispatch");
        assert_eq!(command["threadId"], "thread-ada");
        assert_eq!(command["deliveryIntent"], "steer");
        assert_eq!(command["attachments"], json!([]));
        let text = command["text"].as_str().unwrap();
        assert!(text.contains("[Computer]"));
        assert!(text.contains("Close your browser"));
        assert_eq!(text, CLOSE_PROMPT);
        assert_ne!(text, OPEN_PROMPT);
    }

    #[test]
    fn only_server_tabs_are_listed() {
        let listed = tabs_for_screen(&json!({
            "sessions": [
                { "runtime": "server", "tabId": "t1", "navStatus": { "url": "https://example.com/a" } },
                { "runtime": "local", "tabId": "no", "navStatus": { "url": "https://hidden.example" } }
            ]
        }));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["id"], "t1");
        assert_eq!(listed[0]["title"], "example.com");
    }

    #[test]
    fn an_empty_list_and_a_bare_address_stay_honest() {
        assert!(tabs_for_screen(&json!({ "sessions": [] })).is_empty());
        assert!(tabs_for_screen(&json!({})).is_empty());
        let listed = tabs_for_screen(&json!({
            "sessions": [
                { "runtime": "server", "tabId": "blank", "navStatus": { "url": "" } },
                { "runtime": "server", "tabId": "raw", "navStatus": { "url": "not a url" } }
            ]
        }));
        assert_eq!(listed[0]["title"], "Page");
        assert_eq!(listed[1]["title"], "not a url");
        assert_eq!(listed[1]["url"], "not a url");
    }

    #[test]
    fn a_click_fraction_maps_into_the_frame_and_does_not_take_control() {
        let messages = click_messages(0.5, 0.25, 800, 400).unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["action"], "move");
        assert_eq!(messages[1]["action"], "down");
        assert_eq!(messages[2]["action"], "up");
        assert_eq!(messages[0]["x"], 400);
        assert_eq!(messages[0]["y"], 100);
        assert_eq!(messages[1]["x"], 400);
        assert_eq!(messages[2]["y"], 100);
        assert!(messages
            .iter()
            .all(|message| message["type"] != "takeControl"));
        assert!(click_messages(1.5, 0.0, 800, 400).is_err());
    }

    #[test]
    fn an_unknown_tab_falls_back_and_a_non_server_tab_is_omitted() {
        let listed = tabs_for_screen(&json!({
            "sessions": [
                { "runtime": "server", "tabId": "first", "navStatus": { "url": "https://one.example" } },
                { "runtime": "local", "tabId": "hidden", "navStatus": { "url": "https://hidden.example" } },
                { "runtime": "server", "tabId": "second", "navStatus": { "url": "https://two.example" } }
            ]
        }));
        assert!(listed.iter().all(|tab| tab["id"] != "hidden"));
        assert_eq!(frame_tab(&listed, Some("missing")).unwrap()["id"], "first");
        assert_eq!(frame_tab(&listed, Some("second")).unwrap()["id"], "second");
        assert_eq!(frame_tab(&listed, None).unwrap()["id"], "first");
    }

    #[test]
    fn keys_use_the_browser_stream_shape_and_escape_is_not_back() {
        let enter = key_message("Enter").unwrap();
        assert_eq!(enter["type"], "key");
        assert_eq!(enter["key"], "Enter");
        assert_eq!(enter["code"], "Enter");
        assert_eq!(enter["modifiers"], 0);
        assert!(enter.get("keycode").is_none());
        assert_eq!(key_message("Backspace").unwrap()["key"], "Backspace");
        assert_eq!(key_message("Tab").unwrap()["code"], "Tab");
        assert_eq!(key_message("ArrowLeft").unwrap()["key"], "ArrowLeft");
        assert_eq!(key_message("Delete").unwrap()["key"], "Delete");
        assert_eq!(key_message("PageDown").unwrap()["code"], "PageDown");
        let escape = key_message("Escape");
        assert!(escape.is_err());
        assert_ne!(escape.unwrap_or(json!({ "type": "nope" }))["type"], "back");
        let copy = edit_message("copy").unwrap();
        assert_eq!(copy["key"], "c");
        assert_eq!(copy["code"], "KeyC");
        assert_eq!(copy["modifiers"], META);
        assert!(edit_message("paste").is_err());
    }

    #[test]
    fn text_is_one_message_and_a_long_clip_is_refused_without_echoing_it() {
        let message = text_message("hello").unwrap();
        assert_eq!(message["type"], "text");
        assert_eq!(message["text"], "hello");
        let secret = "s".repeat(TEXT_CAP + 1);
        let refused = text_message(&secret).unwrap_err();
        assert_eq!(refused, "That text is too long.");
        assert!(!refused.contains(secret.as_str()));
    }

    #[test]
    fn a_scroll_stays_in_frame_pixels() {
        let message = wheel_message(0.0, 1.0, 800, 400, -120.0).unwrap();
        assert_eq!(message["type"], "wheel");
        assert_eq!(message["x"], 0);
        assert_eq!(message["y"], 400);
        assert_eq!(message["deltaY"], -120.0);
        assert_ne!(message["type"], "takeControl");
    }

    #[test]
    fn release_drops_the_held_page() {
        let mut sessions = Sessions::default();
        sessions.take("ada", "first", 800, 400);
        assert_eq!(
            sessions.get("ada").map(|hold| hold.tab.as_str()),
            Some("first")
        );
        sessions.release("ada");
        assert!(sessions.get("ada").is_none());
        let click = click_messages(0.5, 0.25, 800, 400).unwrap();
        assert!(click.iter().all(|message| message["type"] != "takeControl"));
        assert!(sessions.get("ada").is_none());
    }

    #[test]
    fn a_letterboxed_point_maps_into_the_jpeg_and_the_margin_is_not_a_click() {
        let inside = contain_fractions(400.0, 300.0, 800.0, 800.0, 800, 400)
            .unwrap()
            .unwrap();
        assert_eq!(inside.0, 0.5);
        assert_eq!(inside.1, 0.25);
        assert!(contain_fractions(400.0, 10.0, 800.0, 800.0, 800, 400)
            .unwrap()
            .is_none());
        assert!(contain_fractions(400.0, 10.0, 0.0, 800.0, 800, 400).is_err());
    }

    #[test]
    fn a_jpeg_sof_gives_the_frame_size_a_hold_keeps() {
        let bytes = [
            0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x01, 0x90, 0x02, 0x80, 0x03, 0x01, 0x11,
            0x00, 0x02, 0x11, 0x00, 0x03, 0x11, 0x00, 0xff, 0xd9,
        ];
        assert_eq!(jpeg_size(&bytes), Some((640, 400)));
        assert!(jpeg_size(&[0xff, 0xd8, 0xff, 0xd9]).is_none());
        let mut sessions = Sessions::default();
        sessions.take("ada", "first", 0, 0);
        sessions.remember_size("ada", 640, 400);
        sessions.take("ada", "first", 0, 0);
        let hold = sessions.get("ada").unwrap();
        assert_eq!((hold.width, hold.height), (640, 400));
    }

    #[test]
    fn the_picture_stays_with_the_held_session_until_release() {
        let mut sessions = Sessions::default();
        sessions.take("ada", "first", 800, 400);
        sessions.note_image("ada", "jpeg");
        assert_eq!(sessions.picture("ada", "first").as_deref(), Some("jpeg"));
        assert!(sessions.picture("ada", "second").is_none());
        sessions.take("ada", "first", 800, 400);
        assert_eq!(sessions.picture("ada", "first").as_deref(), Some("jpeg"));
        sessions.release("ada");
        assert!(sessions.picture("ada", "first").is_none());
    }
}

#[cfg(test)]
mod etag_tests {
    use super::etag;

    #[test]
    fn the_same_frame_has_the_same_etag_and_another_frame_does_not() {
        assert_eq!(etag("AAAA"), etag("AAAA"));
        assert_ne!(etag("AAAA"), etag("AAAB"));
        assert!(!etag("").is_empty());
    }
}
