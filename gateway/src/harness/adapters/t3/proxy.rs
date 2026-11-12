//! `/t3/*` passthrough. The bearer stays here, so a browser never sees the T3 token.
use super::client::{bearer_request, T3};
use axum::extract::ws::{Message as AxMsg, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message as WsMsg;

/// `/t3/api/...` → T3's `/api/...`, with the bearer added and the client's cookies dropped.
pub(crate) async fn proxy_http(
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
pub(crate) async fn proxy_ws(
    client: WebSocket,
    upstream_url: String,
    token: String,
    close_sockets: std::sync::Arc<tokio::sync::Notify>,
) {
    // Registered before the upstream connects, so a password change during that wait still closes this socket.
    let closed = close_sockets.notified();
    tokio::pin!(closed);
    closed.as_mut().enable();
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
        _ = closed.as_mut() => {},
    }
}
