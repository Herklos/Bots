//! The Bot's computer: T3's headless browser. Watching is read-only.
//! Take control is a separate action and is released when the person leaves it.
use crate::t3::T3;
use crate::vault_tools::{server_tabs, Tab};
use serde_json::{json, Value};
use std::time::Duration;

pub fn watch_message() -> Value {
    json!({ "type": "ack" })
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
    Ok((fraction * f64::from(span)).round() as i64)
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

/// T3's preview key map. Escape is the phone's back gesture, so it is not sent.
pub fn key_message(key: &str) -> Result<Value, &'static str> {
    let code = match key {
        "ArrowUp" => 19,
        "ArrowDown" => 20,
        "ArrowLeft" => 21,
        "ArrowRight" => 22,
        "Tab" => 61,
        "Enter" => 66,
        "Backspace" => 67,
        _ => return Err("That key is not sent."),
    };
    Ok(json!({ "type": "key", "keycode": code }))
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
        let image = self
            .holds
            .get(slug)
            .filter(|hold| hold.tab == tab)
            .map(|hold| hold.image.clone())
            .unwrap_or_default();
        self.holds.insert(
            slug.to_string(),
            Hold {
                tab: tab.to_string(),
                width,
                height,
                image,
            },
        );
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

/// Input on the held page. The messages are the click, scroll, text, or key, not a new take.
/// The bytes are the next JPEG from that same interactive session.
pub async fn send_input(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    messages: &[Value],
) -> Result<Vec<u8>, String> {
    if messages
        .iter()
        .any(|message| message.get("type").and_then(Value::as_str) == Some("takeControl"))
    {
        return Err("A click does not take control.".into());
    }
    deliver(t3, thread, tab_id, messages).await
}

async fn deliver(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    messages: &[Value],
) -> Result<Vec<u8>, String> {
    let mut ws = connect(t3, thread, tab_id, true).await?;
    use futures_util::SinkExt;
    for message in messages {
        ws.send(tokio_tungstenite::tungstenite::Message::text(
            message.to_string(),
        ))
        .await
        .map_err(|_| "The browser closed the connection.".to_string())?;
    }
    match tokio::time::timeout(Duration::from_millis(800), read_one_jpeg(&mut ws)).await {
        Ok(Ok(bytes)) => Ok(bytes),
        _ => Ok(Vec::new()),
    }
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

async fn read_one_jpeg(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Result<Vec<u8>, String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let frame = tokio::time::timeout_at(deadline, ws.next())
            .await
            .map_err(|_| "The picture didn't arrive.".to_string())?
            .ok_or_else(|| "The browser closed the connection.".to_string())?
            .map_err(|_| "The browser closed the connection.".to_string())?;
        if let Message::Binary(bytes) = frame {
            let _ = ws.send(Message::text(watch_message().to_string())).await;
            return Ok(bytes.to_vec());
        }
    }
    Err("The picture didn't arrive.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn keys_use_the_preview_map_and_escape_is_not_back() {
        assert_eq!(key_message("Enter").unwrap()["keycode"], 66);
        assert_eq!(key_message("Backspace").unwrap()["keycode"], 67);
        assert_eq!(key_message("Tab").unwrap()["keycode"], 61);
        assert_eq!(key_message("ArrowLeft").unwrap()["keycode"], 21);
        assert_eq!(key_message("ArrowUp").unwrap()["keycode"], 19);
        let escape = key_message("Escape");
        assert!(escape.is_err());
        assert_ne!(escape.unwrap_or(json!({ "type": "nope" }))["type"], "back");
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
