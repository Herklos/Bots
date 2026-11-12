//! One preview socket for the whole hold. Take opens it, input reuses it, release closes it.
//! A held page then pays the connect and take once, not on every key.
//!
//! That socket is the only picture of the page. A reader acks each JPEG into [`Live::latest`]
//! for the whole hold. Input only writes. Without the reader, frames sit unread and the
//! picture stays on the take frame until release opens a fresh watch socket.
use crate::bots::computer::{control_message, watch_message};
use crate::harness::t3::T3;
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

type Write = SplitSink<Ws, Message>;

/// How long to wait after a frame before asking for the next. A peer that emits one
/// JPEG per ack would otherwise spin. A still page sends nothing, so this is idle.
const ACK_PACE: Duration = Duration::from_millis(80);

struct Live {
    tab: String,
    write: Arc<tokio::sync::Mutex<Write>>,
    /// Newest JPEG. Empty until the first one arrives.
    latest: Arc<std::sync::Mutex<Vec<u8>>>,
    stopped: Arc<AtomicBool>,
    stop: Arc<Notify>,
}

fn slots() -> &'static tokio::sync::Mutex<HashMap<String, Arc<Live>>> {
    static SLOTS: std::sync::OnceLock<tokio::sync::Mutex<HashMap<String, Arc<Live>>>> =
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
        .map_err(|_e| "Couldn't open the browser stream.".to_string())?;
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
        .map_err(|_e| "Couldn't open the browser stream.".to_string())?;
    let auth = HeaderValue::from_str(&format!("Bearer {}", t3.token))
        .map_err(|_e| "Couldn't open the browser stream.".to_string())?;
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

fn remember(slug: &str, live: &Live, bytes: &[u8]) {
    if let Ok(mut latest) = live.latest.lock() {
        *latest = bytes.to_vec();
    }
    let mut sessions = crate::bots::computer::sessions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    sessions.note_image(slug, &crate::bots::library::encode_file(bytes));
    if let Some((width, height)) = crate::bots::computer::jpeg_size(bytes) {
        sessions.remember_size(slug, width, height);
    }
}

fn stop(live: &Live) {
    live.stopped.store(true, Ordering::Release);
    live.stop.notify_waiters();
}

fn retire(old: Arc<Live>) {
    stop(&old);
    tokio::spawn(async move {
        let mut write = old.write.lock().await;
        let _ = write.close().await;
    });
}

/// Read frames for as long as the hold lasts. Each one is acked, or the stream stalls.
async fn pump(slug: String, live: Arc<Live>, mut read: futures_util::stream::SplitStream<Ws>) {
    loop {
        let notified = live.stop.notified();
        tokio::pin!(notified);
        if live.stopped.load(Ordering::Acquire) {
            break;
        }
        let message = tokio::select! {
            _ = notified => break,
            message = read.next() => message,
        };
        match message {
            Some(Ok(Message::Binary(bytes))) => {
                remember(&slug, &live, &bytes);
                let pause = live.stop.notified();
                tokio::pin!(pause);
                tokio::select! {
                    _ = pause => break,
                    _ = tokio::time::sleep(ACK_PACE) => {}
                }
                if live.stopped.load(Ordering::Acquire) {
                    break;
                }
                let mut write = live.write.lock().await;
                if write
                    .send(Message::text(watch_message().to_string()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Some(Ok(_)) => {}
            _ => break,
        }
    }
    if !live.stopped.load(Ordering::Acquire) {
        let mut all = slots().lock().await;
        if all.get(&slug).is_some_and(|now| Arc::ptr_eq(now, &live)) {
            all.remove(&slug);
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
    .map_err(|_e| CLOSED.to_string())?;
    let mut frame = wait_ready(&mut ws).await?;
    if frame.is_empty() {
        // The first frame gives the page size.
        frame = next_jpeg(&mut ws, Duration::from_secs(2))
            .await
            .unwrap_or_default();
    }
    let (write, read) = ws.split();
    let live = Arc::new(Live {
        tab: tab.to_string(),
        write: Arc::new(tokio::sync::Mutex::new(write)),
        latest: Arc::new(std::sync::Mutex::new(frame.clone())),
        stopped: Arc::new(AtomicBool::new(false)),
        stop: Arc::new(Notify::new()),
    });
    if !frame.is_empty() {
        remember(slug, &live, &frame);
    }
    let replaced = {
        let mut all = slots().lock().await;
        all.insert(slug.to_string(), live.clone())
    };
    if let Some(old) = replaced {
        retire(old);
    }
    tokio::spawn(pump(slug.to_string(), live, read));
    Ok(frame)
}

/// Send input on the held socket. `None` means there is no live one for this tab,
/// so the caller falls back to a socket of its own. The picture arrives on the reader.
pub(crate) async fn input(slug: &str, tab: &str, messages: &[Value]) -> Option<Vec<u8>> {
    let live = {
        let all = slots().lock().await;
        all.get(slug).cloned()
    }?;
    if live.tab != tab || live.stopped.load(Ordering::Acquire) {
        return None;
    }
    let sent = async {
        let mut write = live.write.lock().await;
        for message in messages {
            write
                .send(Message::text(message.to_string()))
                .await
                .map_err(|_e| CLOSED.to_string())?;
        }
        Ok::<(), String>(())
    }
    .await;
    if sent.is_err() {
        stop(&live);
        let mut all = slots().lock().await;
        if all.get(slug).is_some_and(|now| Arc::ptr_eq(now, &live)) {
            all.remove(slug);
        }
        return None;
    }
    Some(Vec::new())
}

/// The newest JPEG on the held socket. `None` when this viewer is not holding one.
pub(crate) async fn latest(slug: &str) -> Option<Vec<u8>> {
    let live = {
        let all = slots().lock().await;
        all.get(slug).cloned()
    }?;
    let bytes = live
        .latest
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    if bytes.is_empty() {
        None
    } else {
        Some(bytes)
    }
}

/// Release on the held socket and close it. False when none was held.
pub(crate) async fn release(slug: &str) -> bool {
    let Some(live) = slots().lock().await.remove(slug) else {
        return false;
    };
    stop(&live);
    let mut write = live.write.lock().await;
    if let Ok(message) = control_message("release") {
        let _ = write.send(Message::text(message.to_string())).await;
    }
    let _ = write.close().await;
    true
}

#[cfg(test)]
#[path = "hold_tests.rs"]
mod tests;
