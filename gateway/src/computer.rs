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

pub async fn send_control(t3: &T3, thread: &str, tab_id: &str, action: &str) -> Result<(), String> {
    let message = control_message(action).map_err(str::to_string)?;
    let mut ws = connect(t3, thread, tab_id, true).await?;
    use futures_util::SinkExt;
    ws.send(tokio_tungstenite::tungstenite::Message::text(
        message.to_string(),
    ))
    .await
    .map_err(|_| "The browser closed the connection.".to_string())?;
    let _ = tokio::time::timeout(Duration::from_millis(800), read_one_jpeg(&mut ws)).await;
    Ok(())
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
