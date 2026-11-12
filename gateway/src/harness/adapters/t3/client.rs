//! T3 Code, one harness backend (https://github.com/pingdotgg/t3code).
//! Other backends may be added beside this module.
//!
//! - [`T3Client`]: a minimal client for T3's Effect RPC over WebSocket (orchestration protocol 2,
//!   JSON, one message per frame).
//!
//! The `/t3/*` passthrough lives in [`super::proxy`].
use anyhow::{anyhow, Context};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message as WsMsg};

/// One stream item, or the Exit that ended the request.
#[derive(Debug)]
enum Note {
    Chunk(Value),
    Ended(anyhow::Result<Value>),
}

#[derive(Debug)]
enum Waiter {
    /// First chunk, or the Exit when a call has no chunks.
    Once(oneshot::Sender<anyhow::Result<Value>>),
    /// Every chunk until the caller interrupts the request.
    Many(mpsc::UnboundedSender<Note>),
}

type Pending = Arc<Mutex<HashMap<String, Waiter>>>;

#[derive(Debug)]
pub(crate) struct T3Client {
    out: mpsc::UnboundedSender<WsMsg>,
    pending: Pending,
    next: AtomicU64,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

pub(crate) fn ws_base(http_base: &str) -> String {
    if let Some(rest) = http_base.strip_prefix("https") {
        format!("wss{rest}")
    } else if let Some(rest) = http_base.strip_prefix("http") {
        format!("ws{rest}")
    } else {
        http_base.to_string()
    }
}

pub(crate) fn bearer_request(
    url: &str,
    token: &str,
) -> anyhow::Result<tokio_tungstenite::tungstenite::handshake::client::Request> {
    let mut req = url.into_client_request()?;
    req.headers_mut()
        .insert("authorization", format!("Bearer {token}").parse()?);
    Ok(req)
}

/// Effect v4 encodes a failure Cause as an array of reasons.
fn describe_cause(cause: &Value) -> String {
    match cause {
        Value::Array(list) => list
            .iter()
            .map(describe_cause)
            .collect::<Vec<_>>()
            .join("; "),
        Value::Object(o) => match o.get("_tag").and_then(Value::as_str) {
            Some("Interrupt") => "interrupted".into(),
            Some("Fail") => o.get("error").map(Value::to_string).unwrap_or_default(),
            Some("Die") => format!("defect: {}", o.get("defect").cloned().unwrap_or_default()),
            _ => cause.to_string(),
        },
        Value::Null => "unknown failure".into(),
        other => other.to_string(),
    }
}

impl T3Client {
    pub(crate) async fn connect(base_url: &str, token: &str) -> anyhow::Result<Self> {
        let url = format!(
            "{}/ws?orchestrationProtocol=2&clientSurface=web&clientAppVersion=bots-rs-{}",
            ws_base(base_url),
            env!("CARGO_PKG_VERSION")
        );
        let (socket, _) = tokio::time::timeout(
            Duration::from_secs(10),
            tokio_tungstenite::connect_async(bearer_request(&url, token)?),
        )
        .await
        .context("T3 timed out")?
        .context("couldn't reach T3 Code")?;
        let (mut sink, mut stream) = socket.split();
        let (out, mut rx) = mpsc::unbounded_channel::<WsMsg>();
        let pending: Pending = Arc::default();
        let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));

        tokio::spawn(async move {
            while let Some(m) = rx.recv().await {
                if sink.send(m).await.is_err() {
                    break;
                }
            }
        });
        let ping = out.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                if ping.send(WsMsg::text(r#"{"_tag":"Ping"}"#)).is_err() {
                    break;
                }
            }
        });
        let (p, acks, c) = (pending.clone(), out.clone(), closed.clone());
        tokio::spawn(async move {
            while let Some(Ok(frame)) = stream.next().await {
                let text = match frame {
                    WsMsg::Text(t) => t.to_string(),
                    WsMsg::Binary(b) => String::from_utf8_lossy(&b).into_owned(),
                    WsMsg::Close(_) => break,
                    _ => continue,
                };
                let Ok(data) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let messages = match data {
                    Value::Array(list) => list,
                    one => vec![one],
                };
                for msg in messages {
                    let id = match &msg["requestId"] {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        _ => continue,
                    };
                    match msg["_tag"].as_str() {
                        // A stream's items; unary calls get none. T3 waits for the Ack.
                        Some("Chunk") => {
                            let _ = acks.send(WsMsg::text(
                                json!({ "_tag": "Ack", "requestId": id }).to_string(),
                            ));
                            let value = msg.get("value").cloned().or_else(|| {
                                msg.get("values")
                                    .and_then(Value::as_array)
                                    .and_then(|items| items.first().cloned())
                            });
                            let Some(value) = value else { continue };
                            let mut guard = p.lock().await;
                            let Some(waiter) = guard.remove(&id) else {
                                continue;
                            };
                            match waiter {
                                Waiter::Once(tx) => {
                                    let _ = tx.send(Ok(value));
                                }
                                Waiter::Many(tx) => {
                                    if tx.send(Note::Chunk(value)).is_ok() {
                                        guard.insert(id.clone(), Waiter::Many(tx));
                                    }
                                }
                            }
                        }
                        Some("Exit") => {
                            let Some(waiter) = p.lock().await.remove(&id) else {
                                continue;
                            };
                            let exit = &msg["exit"];
                            let result = if exit["_tag"] == "Success" {
                                Ok(exit["value"].clone())
                            } else {
                                Err(anyhow!(describe_cause(&exit["cause"])))
                            };
                            match waiter {
                                Waiter::Once(tx) => {
                                    let _ = tx.send(result);
                                }
                                Waiter::Many(tx) => {
                                    let _ = tx.send(Note::Ended(result));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            c.store(true, Ordering::SeqCst);
            for (_, waiter) in p.lock().await.drain() {
                let err = Err(anyhow!("T3 closed the connection"));
                match waiter {
                    Waiter::Once(tx) => {
                        let _ = tx.send(err);
                    }
                    Waiter::Many(tx) => {
                        let _ = tx.send(Note::Ended(err));
                    }
                }
            }
        });
        Ok(Self {
            out,
            pending,
            next: AtomicU64::new(1),
            closed,
        })
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// A unary RPC, e.g. `orchestration.getThreadProjection`.
    pub(crate) async fn call(&self, tag: &str, payload: Value) -> anyhow::Result<Value> {
        let id = self.next.fetch_add(1, Ordering::SeqCst).to_string();
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .await
            .insert(id.clone(), Waiter::Once(tx));
        #[allow(
            clippy::map_err_ignore,
            reason = "a socket error can include the T3 address; callers only need that the connection closed"
        )]
        let sent = self.out
            .send(WsMsg::text(json!({ "_tag": "Request", "id": id, "tag": tag, "payload": payload, "headers": [] }).to_string()))
            .map_err(|_| anyhow!("T3 closed the connection"));
        sent?;
        #[allow(
            clippy::map_err_ignore,
            reason = "the dropped reply is an empty oneshot error; callers only need that T3 closed"
        )]
        let reply = tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .context("T3 timed out")?
            .map_err(|_| anyhow!("T3 closed the connection"));
        reply?
    }

    /// Read a stream until `keep` accepts one item, then interrupt it.
    /// A staying-open subscribe (`orchestration.subscribeShell`,
    /// `scheduledTasks.subscribe`) must not be left running: the next read
    /// would add another one.
    pub(crate) async fn read_until<F>(
        &self,
        tag: &str,
        payload: Value,
        mut keep: F,
    ) -> anyhow::Result<Value>
    where
        F: FnMut(&Value) -> bool,
    {
        let id = self.next.fetch_add(1, Ordering::SeqCst).to_string();
        let (tx, mut rx) = mpsc::unbounded_channel();
        self.pending
            .lock()
            .await
            .insert(id.clone(), Waiter::Many(tx));
        #[allow(
            clippy::map_err_ignore,
            reason = "a socket error can include the T3 address; callers only need that the connection closed"
        )]
        let sent = self.out
            .send(WsMsg::text(
                json!({ "_tag": "Request", "id": id, "tag": tag, "payload": payload, "headers": [] }).to_string(),
            ))
            .map_err(|_| anyhow!("T3 closed the connection"));
        sent?;
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(note) = rx.recv().await {
                match note {
                    Note::Chunk(value) if keep(&value) => return Ok(value),
                    Note::Chunk(_) => {}
                    Note::Ended(Ok(_)) => {
                        return Err(anyhow!("T3 ended the stream before a matching item"));
                    }
                    Note::Ended(Err(error)) => return Err(error),
                }
            }
            Err(anyhow!("T3 closed the connection"))
        })
        .await;
        // Interrupt even when the read timed out or failed, so the server drops it.
        let _ = self.out.send(WsMsg::text(
            json!({ "_tag": "Interrupt", "requestId": id, "interruptors": [] }).to_string(),
        ));
        self.pending.lock().await.remove(&id);
        result.context("T3 timed out")?
    }

    /// `orchestration.dispatchCommand` with a fresh commandId unless the command has one.
    pub(crate) async fn dispatch(&self, mut command: Value) -> anyhow::Result<Value> {
        if command.get("commandId").is_none() {
            command["commandId"] = json!(uuid::Uuid::new_v4().to_string());
        }
        self.call("orchestration.dispatchCommand", command).await
    }
}

/// One shared connection, reopened after T3 restarts.
pub(crate) struct T3 {
    pub base_url: String,
    pub token: String,
    pub http: reqwest::Client,
    client: Mutex<Option<Arc<T3Client>>>,
}

impl std::fmt::Debug for T3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("T3")
            .field("base_url", &self.base_url)
            .field("token", &"…")
            .field("http", &self.http)
            .field("client", &self.client)
            .finish()
    }
}

impl T3 {
    pub(crate) fn new(base_url: String, token: String) -> Self {
        Self {
            base_url,
            token,
            http: reqwest::Client::new(),
            client: Mutex::new(None),
        }
    }

    pub(crate) async fn rpc(&self) -> anyhow::Result<Arc<T3Client>> {
        let mut g = self.client.lock().await;
        if let Some(c) = g.as_ref().filter(|c| !c.is_closed()) {
            return Ok(c.clone());
        }
        let c = Arc::new(T3Client::connect(&self.base_url, &self.token).await?);
        *g = Some(c.clone());
        Ok(c)
    }

    pub(crate) async fn get_json(&self, path: &str) -> anyhow::Result<Value> {
        let res = self
            .http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await?;
        let status = res.status();
        let body = res.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() {
            anyhow::bail!("T3 HTTP {status} {path}");
        }
        Ok(body)
    }
}

/// One staying-open stream. Dropping it interrupts the request so a second
/// subscribe does not pile on.
pub(crate) struct LiveStream {
    rx: mpsc::UnboundedReceiver<anyhow::Result<Value>>,
    stop: Option<StreamStop>,
}

struct StreamStop {
    id: String,
    out: mpsc::UnboundedSender<WsMsg>,
    pending: Pending,
}

impl StreamStop {
    fn interrupt(self) {
        let _ = self.out.send(WsMsg::text(
            json!({ "_tag": "Interrupt", "requestId": self.id, "interruptors": [] }).to_string(),
        ));
        let pending = self.pending.clone();
        let id = self.id.clone();
        tokio::spawn(async move {
            pending.lock().await.remove(&id);
        });
    }
}

impl Drop for LiveStream {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.interrupt();
        }
    }
}

impl LiveStream {
    pub(crate) async fn recv(&mut self) -> Option<anyhow::Result<Value>> {
        self.rx.recv().await
    }
}

impl T3Client {
    /// Keep a stream open until it ends or this value is dropped.
    /// `read_until` still takes one item and interrupts. This is the other shape:
    /// the routine watcher holds one `scheduledTasks.subscribe` and drops it before
    /// opening the next.
    pub(crate) async fn open_stream(
        &self,
        tag: &str,
        payload: Value,
    ) -> anyhow::Result<LiveStream> {
        let id = self.next.fetch_add(1, Ordering::SeqCst).to_string();
        let (notes_tx, mut notes_rx) = mpsc::unbounded_channel();
        self.pending
            .lock()
            .await
            .insert(id.clone(), Waiter::Many(notes_tx));
        let request =
            json!({ "_tag": "Request", "id": id, "tag": tag, "payload": payload, "headers": [] });
        if self.out.send(WsMsg::text(request.to_string())).is_err() {
            self.pending.lock().await.remove(&id);
            return Err(anyhow!("T3 closed the connection"));
        }
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(note) = notes_rx.recv().await {
                match note {
                    Note::Chunk(value) => {
                        if out_tx.send(Ok(value)).is_err() {
                            break;
                        }
                    }
                    Note::Ended(Ok(_)) => break,
                    Note::Ended(Err(err)) => {
                        let _ = out_tx.send(Err(err));
                        break;
                    }
                }
            }
        });
        Ok(LiveStream {
            rx: out_rx,
            stop: Some(StreamStop {
                id,
                out: self.out.clone(),
                pending: Arc::clone(&self.pending),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_scheme() {
        assert_eq!(ws_base("http://127.0.0.1:3773"), "ws://127.0.0.1:3773");
        assert_eq!(ws_base("https://t3.example"), "wss://t3.example");
    }

    #[test]
    fn causes_read_like_hbots() {
        assert_eq!(
            describe_cause(&json!([{ "_tag": "Interrupt" }])),
            "interrupted"
        );
        assert_eq!(
            describe_cause(&json!([{ "_tag": "Fail", "error": "nope" }])),
            "\"nope\""
        );
    }
}
