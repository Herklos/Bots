use super::{ws_base, T3};
use crate::vault::vault_tools::{
    server_tabs, Tab, NOT_A_TAB_STREAM, PROBE_DELAY, RELEASE_TIMEOUT, TAB_LIST_TIMEOUT,
    TYPE_TIMEOUT,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest, http::HeaderValue, Error as WsError, Message,
};

pub(crate) async fn list_tabs(t3: &T3, thread: &str) -> Result<Vec<Tab>, String> {
    let rpc = async {
        let client = t3.rpc().await?;
        client
            .call("preview.list", json!({ "threadId": thread }))
            .await
    };
    let result: anyhow::Result<Value> = match tokio::time::timeout(TAB_LIST_TIMEOUT, rpc).await {
        Ok(r) => r,
        Err(_) => Err(anyhow::anyhow!("timed out")),
    };
    #[allow(
        clippy::map_err_ignore,
        reason = "the T3 error can include the harness address; the Bot gets a fixed retry message"
    )]
    let result = result.map_err(|_| {
        "Couldn't reach your browser on T3 right now. Try again in a moment.".to_string()
    })?;
    Ok(server_tabs(&result))
}

pub(crate) type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Stage {
    /// Waiting for T3 to say we control the tab.
    Taking,
    /// The text is sent; waiting for the probe's answer.
    Typing,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    Ignore,
    /// We have control: send the text, then the probe.
    SendText,
    /// The probe came back: release control.
    Release,
    Fail(&'static str),
}

/// What one frame from T3's preview stream means at this stage.
pub(crate) fn step(stage: Stage, msg: &Value) -> Step {
    match msg["type"].as_str() {
        Some("control") => {
            if msg["canOperate"] == false {
                return Step::Fail("The gateway's T3 token can't operate the browser (it needs the preview:operate scope).");
            }
            match (stage, msg["controller"].as_str()) {
                (Stage::Taking, Some("you")) => Step::SendText,
                (Stage::Taking, Some("another-viewer")) => Step::Fail(
                    "The user is controlling this tab right now (Take over). Try again when they're done.",
                ),
                _ => Step::Ignore,
            }
        }
        Some("probe") if stage == Stage::Typing => Step::Release,
        _ => Step::Ignore,
    }
}

#[allow(
    clippy::map_err_ignore,
    reason = "the URL error can echo the stream address and the header error can echo the bearer token"
)]
pub(crate) async fn type_into_tab(
    t3: &T3,
    thread: &str,
    tab_id: &str,
    text: &str,
) -> Result<(), String> {
    let mut url = url::Url::parse(&format!("{}/api/preview-stream/ws", ws_base(&t3.base_url)))
        .map_err(|_| NOT_A_TAB_STREAM.to_string())?;
    url.query_pairs_mut()
        .append_pair("threadId", thread)
        .append_pair("tabId", tab_id)
        .append_pair("interactive", "true")
        .append_pair("maxWidth", "320")
        .append_pair("maxHeight", "240")
        .append_pair("quality", "10");
    let mut req = url
        .as_str()
        .into_client_request()
        .map_err(|_| NOT_A_TAB_STREAM.to_string())?;
    let auth = HeaderValue::from_str(&format!("Bearer {}", t3.token))
        .map_err(|_| NOT_A_TAB_STREAM.to_string())?;
    req.headers_mut().insert("authorization", auth);
    let (ws, _) = match tokio_tungstenite::connect_async(req).await {
        Ok(ok) => ok,
        Err(WsError::Http(res)) if res.status() == 404 => {
            return Err("That browser tab isn't open.".into())
        }
        Err(WsError::Http(res)) => {
            return Err(format!(
                "The browser stream refused the connection ({}).",
                res.status()
            ))
        }
        Err(_) => return Err(NOT_A_TAB_STREAM.into()),
    };
    tokio::time::timeout(TYPE_TIMEOUT, drive(ws, text))
        .await
        .unwrap_or_else(|_| Err("The browser didn't respond in time.".into()))
}

/// Takes control, types, probes and releases. Errors never include the text.
#[allow(
    clippy::map_err_ignore,
    reason = "a socket error can include the typed text or the stream address; the Bot only hears that the browser closed"
)]
pub(crate) async fn drive(mut ws: Ws, text: &str) -> Result<(), String> {
    const CLOSED: &str = "The browser closed the connection before the text was typed.";
    let send = |ws_msg: String| Message::text(ws_msg);
    ws.send(send(json!({ "type": "takeControl" }).to_string()))
        .await
        .map_err(|_| CLOSED.to_string())?;
    let mut stage = Stage::Taking;
    while let Some(frame) = ws.next().await {
        let frame = frame.map_err(|_| CLOSED.to_string())?;
        match frame {
            // Frames are paced by acks.
            Message::Binary(_) => {
                ws.send(send(json!({ "type": "ack" }).to_string()))
                    .await
                    .map_err(|_| CLOSED.to_string())?;
            }
            Message::Text(t) => {
                let Ok(msg) = serde_json::from_str::<Value>(t.as_str()) else {
                    continue;
                };
                match step(stage, &msg) {
                    Step::Ignore => {}
                    Step::Fail(reason) => return Err(reason.into()),
                    Step::SendText => {
                        stage = Stage::Typing;
                        ws.send(send(json!({ "type": "text", "text": text }).to_string()))
                            .await
                            .map_err(|_| CLOSED.to_string())?;
                        tokio::time::sleep(PROBE_DELAY).await;
                        ws.send(send(json!({ "type": "probe", "x": 1, "y": 1 }).to_string()))
                            .await
                            .map_err(|_| CLOSED.to_string())?;
                    }
                    Step::Release => {
                        ws.send(send(json!({ "type": "releaseControl" }).to_string()))
                            .await
                            .map_err(|_| CLOSED.to_string())?;
                        // Done when T3 says someone else has control, or after a second.
                        let _ = tokio::time::timeout(RELEASE_TIMEOUT, wait_released(&mut ws)).await;
                        return Ok(());
                    }
                }
            }
            Message::Close(_) => return Err(CLOSED.into()),
            _ => {}
        }
    }
    Err(CLOSED.into())
}

pub(crate) async fn wait_released(ws: &mut Ws) {
    while let Some(Ok(Message::Text(t))) = ws.next().await {
        if let Ok(msg) = serde_json::from_str::<Value>(t.as_str()) {
            if msg["type"] == "control" && msg["controller"] != "you" {
                return;
            }
        }
    }
}
