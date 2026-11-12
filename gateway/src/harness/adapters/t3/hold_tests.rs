//! A local preview-stream peer. The hold under test is the real reader, from connect onward.
use super::{input, latest, release, take};
use crate::bots::computer::sessions;
use crate::harness::t3::T3;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

async fn peer(stream: tokio::net::TcpStream, jpeg_a: Vec<u8>, jpeg_b: Vec<u8>, jpeg_c: Vec<u8>) {
    let mut ws = tokio_tungstenite::accept_async(stream)
        .await
        .expect("preview handshake");
    let mut acked = false;
    while let Some(message) = ws.next().await {
        let message = message.expect("preview frame");
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(body) = serde_json::from_str::<Value>(text.as_str()) else {
            continue;
        };
        match body.get("type").and_then(Value::as_str) {
            Some("takeControl") => {
                ws.send(Message::text(
                    r#"{"type":"control","controller":"you","canOperate":true}"#,
                ))
                .await
                .expect("control");
                ws.send(Message::binary(jpeg_a.clone()))
                    .await
                    .expect("jpeg A");
            }
            Some("ack") if !acked => {
                acked = true;
                ws.send(Message::binary(jpeg_b.clone()))
                    .await
                    .expect("jpeg B");
            }
            Some("text") => {
                tokio::time::sleep(Duration::from_millis(200)).await;
                ws.send(Message::binary(jpeg_c.clone()))
                    .await
                    .expect("jpeg C");
            }
            Some("releaseControl") => break,
            _ => {}
        }
    }
}

async fn until_picture(slug: &str, want: &[u8]) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if latest(slug).await.as_deref() == Some(want) {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            let got = latest(slug).await.unwrap_or_default();
            panic!("held picture was {got:?}, wanted {want:?}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_held_page_shows_frames_that_arrive_after_take_and_after_input() {
    let jpeg_a = vec![0xff, 0xd8, 0x0a, 0xff, 0xd9];
    let jpeg_b = vec![0xff, 0xd8, 0x0b, 0xff, 0xd9];
    let jpeg_c = vec![0xff, 0xd8, 0x0c, 0xff, 0xd9];
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let (a, b, c) = (jpeg_a.clone(), jpeg_b.clone(), jpeg_c.clone());
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        peer(stream, a, b, c).await;
    });
    tokio::task::yield_now().await;

    let slug = format!("hold-preview-{}", std::process::id());
    let _ = release(&slug).await;
    {
        let mut sessions = sessions()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        sessions.release(&slug);
        sessions.take(&slug, "tab", 0, 0);
    }

    let t3 = T3::new(format!("http://{addr}"), "token".to_string());
    take(&t3, "thread", &slug, "tab").await.expect("take");
    until_picture(&slug, &jpeg_b).await;

    let started = std::time::Instant::now();
    let accepted = input(
        &slug,
        "tab",
        &[serde_json::json!({"type": "text", "text": "hi"})],
    )
    .await;
    let elapsed = started.elapsed();
    assert!(
        accepted.is_some(),
        "input was not accepted on the held socket"
    );
    assert!(
        elapsed < Duration::from_millis(150),
        "input waited for the late frame: {elapsed:?}"
    );
    until_picture(&slug, &jpeg_c).await;

    assert!(release(&slug).await, "release should close the hold");
    assert!(
        latest(&slug).await.is_none(),
        "released hold still supplies a picture"
    );
    assert!(
        !release(&slug).await,
        "second release should find nothing held"
    );
    sessions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .release(&slug);
}
