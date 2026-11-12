#![allow(
    clippy::map_err_ignore,
    reason = "a socket error can include the stream address; the person sees a fixed sentence"
)]

use super::T3;
#[cfg(test)]
use crate::bots::computer::Sessions;
use crate::bots::computer::{control_message, edit_message, watch_message};
use crate::vault::vault_tools::{server_tabs, Tab};
use serde_json::{json, Value};
use std::time::Duration;

/// Tabs the watch screen may show. Other runtimes are left out.
pub(crate) fn tabs_for_screen(list: &Value) -> Vec<Value> {
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

/// The inline card. Lighter than a frame the person is looking at up close.
pub(crate) const CARD_MAX_WIDTH: &str = "960";

pub(crate) const CARD_MAX_HEIGHT: &str = "540";

pub(crate) const CARD_QUALITY: &str = "60";

/// Interactive control and the large view.
pub(crate) const VIEW_MAX_WIDTH: &str = "1920";

pub(crate) const VIEW_MAX_HEIGHT: &str = "1200";

pub(crate) const VIEW_QUALITY: &str = "80";

/// Which preview size a watch should ask for. A held page does not open a second socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WatchSize {
    Card,
    Large,
}

/// One JPEG from the preview stream, without taking control.
pub(crate) async fn one_frame(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    size: WatchSize,
) -> Result<Vec<u8>, String> {
    let mut ws = connect(t3, thread, tab_id, false, size).await?;
    read_one_jpeg(&mut ws).await
}

pub(crate) async fn send_control(
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
pub(crate) struct Driven {
    pub image: Vec<u8>,
    pub clipboard: String,
}

/// Input on the held page. The messages are the click, scroll, text, or key, not a new take.
/// Take runs on this same socket first, so the page stays owned for the turn.
pub(crate) async fn send_input(
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
pub(crate) async fn send_edit(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    action: &str,
) -> Result<Driven, String> {
    let message = edit_message(action).map_err(str::to_string)?;
    drive(t3, thread, tab_id, &[message], true).await
}

/// Take or release only. Does not prepend another take.
pub(crate) async fn deliver(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    messages: &[Value],
) -> Result<Vec<u8>, String> {
    use futures_util::SinkExt;
    let mut ws = connect(t3, thread, tab_id, true, WatchSize::Large).await?;
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
pub(crate) async fn drive(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    messages: &[Value],
    want_clipboard: bool,
) -> Result<Driven, String> {
    use futures_util::SinkExt;
    let mut ws = connect(t3, thread, tab_id, true, WatchSize::Large).await?;
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

/// Preview query for one socket. Interactive and the large view share the sharper cap.
/// The inline card keeps the lighter cap. This does not send a resize message.
pub(crate) fn preview_stream_url(
    ws_base: &str,
    thread: &str,
    tab_id: &str,
    interactive: bool,
    size: WatchSize,
) -> Result<url::Url, &'static str> {
    let (max_width, max_height, quality) = preview_limits(interactive, size);
    let mut url = url::Url::parse(&format!("{ws_base}/api/preview-stream/ws"))
        .map_err(|_| "Couldn't open the browser stream.")?;
    url.query_pairs_mut()
        .append_pair("threadId", thread)
        .append_pair("tabId", tab_id)
        .append_pair("interactive", if interactive { "true" } else { "false" })
        .append_pair("maxWidth", max_width)
        .append_pair("maxHeight", max_height)
        .append_pair("quality", quality);
    Ok(url)
}

pub(crate) fn preview_limits(
    interactive: bool,
    size: WatchSize,
) -> (&'static str, &'static str, &'static str) {
    if interactive {
        return (VIEW_MAX_WIDTH, VIEW_MAX_HEIGHT, VIEW_QUALITY);
    }
    match size {
        WatchSize::Card => (CARD_MAX_WIDTH, CARD_MAX_HEIGHT, CARD_QUALITY),
        WatchSize::Large => (VIEW_MAX_WIDTH, VIEW_MAX_HEIGHT, VIEW_QUALITY),
    }
}

pub(crate) async fn connect(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    interactive: bool,
    size: WatchSize,
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
    let url =
        preview_stream_url(&ws_base, thread, tab_id, interactive, size).map_err(str::to_string)?;
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

pub(crate) type PreviewWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub(crate) async fn read_one_jpeg(ws: &mut PreviewWs) -> Result<Vec<u8>, String> {
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

pub(crate) const CANNOT_OPERATE: &str =
    "hbots's T3 token can't operate the browser. Run hbots up --refresh-token.";

/// What one `control` frame means. `Ready` is the only outcome that may mark a take held.
pub(crate) enum ControlFrame {
    Ignore,
    Ready,
    Denied(&'static str),
}

pub(crate) fn control_frame(msg: &Value) -> ControlFrame {
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
pub(crate) fn hold_if_ready(
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
pub(crate) async fn wait_for_control(ws: &mut PreviewWs) -> Result<(), String> {
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

pub(crate) async fn wait_for_clipboard(ws: &mut PreviewWs) -> Result<String, String> {
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
