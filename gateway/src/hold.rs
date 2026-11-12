//! One preview socket for the whole hold. Take opens it, input reuses it, release closes it.
//! A held page then pays the connect and take once, not on every key.
use crate::computer::{control_message, watch_message};
use crate::t3::T3;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct Held {
    tab: String,
    ws: Ws,
}

type Slot = Arc<tokio::sync::Mutex<Held>>;

fn slots() -> &'static tokio::sync::Mutex<HashMap<String, Slot>> {
    static SLOTS: std::sync::OnceLock<tokio::sync::Mutex<HashMap<String, Slot>>> =
        std::sync::OnceLock::new();
    SLOTS.get_or_init(Default::default)
}

const CLOSED: &str = "The browser closed the connection.";

async fn connect(t3: &T3, thread: &str, tab: &str) -> Result<Ws, String> {
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
        .append_pair("tabId", tab)
        .append_pair("interactive", "true")
        .append_pair("maxWidth", "1920")
        .append_pair("maxHeight", "1200")
        .append_pair("quality", "80");
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

/// Wait until this viewer owns the page, acking frames meanwhile. Returns the last one seen.
async fn wait_ready(ws: &mut Ws) -> Result<Vec<u8>, String> {
    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let frame = match tokio::time::timeout_at(deadline, ws.next()).await {
            Ok(Some(Ok(frame))) => frame,
            Ok(_) => return Err(CLOSED.into()),
            Err(_) => return Err("The page didn't confirm control.".into()),
        };
        match frame {
            Message::Binary(bytes) => {
                seen = bytes.to_vec();
                let _ = ws.send(Message::text(watch_message().to_string())).await;
            }
            Message::Text(text) => {
                let Ok(msg) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                if msg.get("type").and_then(Value::as_str) != Some("control") {
                    continue;
                }
                if msg["canOperate"] == false {
                    return Err(
                        "hbots's T3 token can't operate the browser. Run hbots up --refresh-token."
                            .into(),
                    );
                }
                match msg.get("controller").and_then(Value::as_str).unwrap_or("") {
                    "another-viewer" => return Err("Someone else has this page.".into()),
                    "you" | "unclaimed" | "none" => return Ok(seen),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// The next JPEG within `wait`, or empty. Acks it so the stream sends another.
async fn next_jpeg(ws: &mut Ws, wait: Duration) -> Result<Vec<u8>, String> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        match tokio::time::timeout_at(deadline, ws.next()).await {
            Err(_) => return Ok(Vec::new()),
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                let _ = ws.send(Message::text(watch_message().to_string())).await;
                return Ok(bytes.to_vec());
            }
            Ok(Some(Ok(_))) => {}
            Ok(_) => return Err(CLOSED.into()),
        }
    }
}

/// Take the page on a new socket and keep it. Returns the first frame, possibly empty.
pub(crate) async fn take(t3: &T3, thread: &str, slug: &str, tab: &str) -> Result<Vec<u8>, String> {
    let mut ws = connect(t3, thread, tab).await?;
    ws.send(Message::text(
        control_message("take").map_err(str::to_string)?.to_string(),
    ))
    .await
    .map_err(|_| CLOSED.to_string())?;
    let mut frame = wait_ready(&mut ws).await?;
    if frame.is_empty() {
        // The first frame gives the page size.
        frame = next_jpeg(&mut ws, Duration::from_secs(2))
            .await
            .unwrap_or_default();
    }
    let held = Held {
        tab: tab.to_string(),
        ws,
    };
    slots()
        .lock()
        .await
        .insert(slug.to_string(), Arc::new(tokio::sync::Mutex::new(held)));
    Ok(frame)
}

/// Send input on the held socket. `None` means there is no live one for this tab,
/// so the caller falls back to a socket of its own.
pub(crate) async fn input(slug: &str, tab: &str, messages: &[Value]) -> Option<Vec<u8>> {
    let slot = slots().lock().await.get(slug).cloned()?;
    let mut held = slot.lock().await;
    if held.tab != tab {
        return None;
    }
    let sent = async {
        for message in messages {
            held.ws
                .send(Message::text(message.to_string()))
                .await
                .map_err(|_| CLOSED.to_string())?;
        }
        // A still page sends nothing new, so don't wait long for a picture.
        next_jpeg(&mut held.ws, Duration::from_millis(100)).await
    }
    .await;
    match sent {
        Ok(frame) => Some(frame),
        Err(_) => {
            drop(held);
            let mut all = slots().lock().await;
            if all.get(slug).is_some_and(|now| Arc::ptr_eq(now, &slot)) {
                all.remove(slug);
            }
            None
        }
    }
}

/// Release on the held socket and close it. False when none was held.
pub(crate) async fn release(slug: &str) -> bool {
    let Some(slot) = slots().lock().await.remove(slug) else {
        return false;
    };
    let mut held = slot.lock().await;
    if let Ok(message) = control_message("release") {
        let _ = held.ws.send(Message::text(message.to_string())).await;
    }
    let _ = held.ws.close(None).await;
    true
}
