//! T3 Code, one harness backend (https://github.com/pingdotgg/t3code).
//! Other backends may be added beside this module.
//!
//! - [`T3Client`]: a minimal client for T3's Effect RPC over WebSocket (orchestration protocol 2,
//!   JSON, one message per frame).
//! - [`proxy_http`] and [`proxy_ws`]: the `/t3/*` passthrough the browser uses, which adds the
//!   bearer so T3's token never reaches a client.
use anyhow::{anyhow, Context};
use axum::extract::ws::{Message as AxMsg, WebSocket};
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

type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<anyhow::Result<Value>>>>>;

pub struct T3Client {
    out: mpsc::UnboundedSender<WsMsg>,
    pending: Pending,
    next: AtomicU64,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

pub fn ws_base(http_base: &str) -> String {
    if let Some(rest) = http_base.strip_prefix("https") {
        format!("wss{rest}")
    } else if let Some(rest) = http_base.strip_prefix("http") {
        format!("ws{rest}")
    } else {
        http_base.to_string()
    }
}

fn bearer_request(
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
    pub async fn connect(base_url: &str, token: &str) -> anyhow::Result<Self> {
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
                            // A stream's first item answers a one-shot read (scheduledTasks.subscribe).
                            let value = msg.get("value").cloned().or_else(|| {
                                msg.get("values")
                                    .and_then(Value::as_array)
                                    .and_then(|items| items.first().cloned())
                            });
                            if let Some(value) = value {
                                if let Some(tx) = p.lock().await.remove(&id) {
                                    let _ = tx.send(Ok(value));
                                }
                            }
                        }
                        Some("Exit") => {
                            if let Some(tx) = p.lock().await.remove(&id) {
                                let exit = &msg["exit"];
                                let result = if exit["_tag"] == "Success" {
                                    Ok(exit["value"].clone())
                                } else {
                                    Err(anyhow!(describe_cause(&exit["cause"])))
                                };
                                let _ = tx.send(result);
                            }
                        }
                        _ => {}
                    }
                }
            }
            c.store(true, Ordering::SeqCst);
            for (_, tx) in p.lock().await.drain() {
                let _ = tx.send(Err(anyhow!("T3 closed the connection")));
            }
        });
        Ok(Self {
            out,
            pending,
            next: AtomicU64::new(1),
            closed,
        })
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// A unary RPC, e.g. `orchestration.getThreadProjection`.
    pub async fn call(&self, tag: &str, payload: Value) -> anyhow::Result<Value> {
        let id = self.next.fetch_add(1, Ordering::SeqCst).to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        self.out
            .send(WsMsg::text(json!({ "_tag": "Request", "id": id, "tag": tag, "payload": payload, "headers": [] }).to_string()))
            .map_err(|_| anyhow!("T3 closed the connection"))?;
        tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .context("T3 timed out")?
            .map_err(|_| anyhow!("T3 closed the connection"))?
    }

    /// `orchestration.dispatchCommand` with a fresh commandId unless the command has one.
    pub async fn dispatch(&self, mut command: Value) -> anyhow::Result<Value> {
        if command.get("commandId").is_none() {
            command["commandId"] = json!(uuid::Uuid::new_v4().to_string());
        }
        self.call("orchestration.dispatchCommand", command).await
    }
}

/// One shared connection, reopened after T3 restarts.
pub struct T3 {
    pub base_url: String,
    pub token: String,
    pub http: reqwest::Client,
    client: Mutex<Option<Arc<T3Client>>>,
}

impl T3 {
    pub fn new(base_url: String, token: String) -> Self {
        Self {
            base_url,
            token,
            http: reqwest::Client::new(),
            client: Mutex::new(None),
        }
    }

    pub async fn rpc(&self) -> anyhow::Result<Arc<T3Client>> {
        let mut g = self.client.lock().await;
        if let Some(c) = g.as_ref().filter(|c| !c.is_closed()) {
            return Ok(c.clone());
        }
        let c = Arc::new(T3Client::connect(&self.base_url, &self.token).await?);
        *g = Some(c.clone());
        Ok(c)
    }

    pub async fn get_json(&self, path: &str) -> anyhow::Result<Value> {
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

/// `/t3/api/...` → T3's `/api/...`, with the bearer added and the client's cookies dropped.
pub async fn proxy_http(
    t3: &T3,
    method: axum::http::Method,
    path_and_query: &str,
    headers: &axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let url = format!("{}{path_and_query}", t3.base_url);
    let mut req = t3
        .http
        .request(method, url)
        .bearer_auth(&t3.token)
        .body(body);
    for name in ["content-type", "accept", "range", "if-none-match"] {
        if let Some(v) = headers.get(name) {
            req = req.header(name, v);
        }
    }
    match req.send().await {
        Ok(res) => {
            let status = res.status();
            let mut out = axum::http::HeaderMap::new();
            for name in [
                "content-type",
                "cache-control",
                "etag",
                "content-range",
                "accept-ranges",
                "content-disposition",
            ] {
                if let Some(v) = res.headers().get(name) {
                    out.insert(name, v.clone());
                }
            }
            let bytes = res.bytes().await.unwrap_or_default();
            (status, out, bytes).into_response()
        }
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            axum::Json(json!({ "error": format!("T3 unavailable: {e}") })),
        )
            .into_response(),
    }
}

/// Pipes a client WebSocket to T3 (`/ws` for RPC, `/api/preview-stream/ws` for a Bot's screen).
pub async fn proxy_ws(
    client: WebSocket,
    upstream_url: String,
    token: String,
    close_sockets: std::sync::Arc<tokio::sync::Notify>,
) {
    let upstream = match bearer_request(&upstream_url, &token) {
        Ok(req) => tokio_tungstenite::connect_async(req).await,
        Err(e) => {
            tracing::error!("bad upstream url: {e}");
            return;
        }
    };
    let Ok((upstream, _)) = upstream.map_err(|e| tracing::error!("upstream ws error: {e}")) else {
        let mut client = client;
        let _ = client.send(AxMsg::Close(None)).await;
        return;
    };
    let (mut up_tx, mut up_rx) = upstream.split();
    let (mut cl_tx, mut cl_rx) = client.split();
    let to_upstream = async {
        while let Some(Ok(m)) = cl_rx.next().await {
            let m = match m {
                AxMsg::Text(t) => WsMsg::text(t.to_string()),
                AxMsg::Binary(b) => WsMsg::binary(b),
                AxMsg::Ping(p) => WsMsg::Ping(p),
                AxMsg::Pong(p) => WsMsg::Pong(p),
                AxMsg::Close(_) => break,
            };
            if up_tx.send(m).await.is_err() {
                break;
            }
        }
        let _ = up_tx.close().await;
    };
    let to_client = async {
        while let Some(Ok(m)) = up_rx.next().await {
            let m = match m {
                WsMsg::Text(t) => AxMsg::text(t.to_string()),
                WsMsg::Binary(b) => AxMsg::Binary(b),
                WsMsg::Ping(p) => AxMsg::Ping(p),
                WsMsg::Pong(p) => AxMsg::Pong(p),
                WsMsg::Close(_) | WsMsg::Frame(_) => break,
            };
            if cl_tx.send(m).await.is_err() {
                break;
            }
        }
        let _ = cl_tx.close().await;
    };
    tokio::select! {
        _ = to_upstream => {},
        _ = to_client => {},
        // A new remote password signs everyone out, including sockets already open.
        _ = close_sockets.notified() => {},
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
