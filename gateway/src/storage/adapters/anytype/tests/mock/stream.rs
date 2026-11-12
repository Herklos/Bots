//! `POST /v2/spaces/{id}/search/stream`. SSE: one `object_added` per match, then
//! `snapshot_complete`, then live `object_added` / `object_updated` / `object_removed`,
//! with a `: keepalive` comment on the heartbeat interval.
//!
//! A full channel or a lagged subscriber ends the response. The client treats that as
//! a dropped stream and opens a fresh snapshot. `DropAll` does the same on purpose.

use super::search::{reject_search_body, search};
use super::state::{Live, MockState};
use super::Shared;
use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Deserialize)]
pub(super) struct StreamQuery {
    heartbeat: Option<u64>,
}

struct Slot(Shared);

impl Drop for Slot {
    fn drop(&mut self) {
        let mut mock = self.0.lock().expect("mock");
        mock.stream_open = mock.stream_open.saturating_sub(1);
    }
}

pub(super) async fn search_stream(
    State(state): State<Shared>,
    Query(query): Query<StreamQuery>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    if let Some(response) = super::routes::prelude(&state).await {
        return response;
    }
    let parsed = super::routes::parse_json(&body);
    if parsed.get("query").is_some() {
        return json_reply(
            StatusCode::BAD_REQUEST,
            json!({"message": "A search stream does not take a query."}),
        );
    }
    if let Some(error) = reject_search_body(&parsed) {
        return json_reply(StatusCode::BAD_REQUEST, error);
    }
    let heartbeat = Duration::from_secs(query.heartbeat.unwrap_or(15).clamp(1, 120));
    let live = {
        let mut mock = state.lock().expect("mock");
        if mock.stream_open >= mock.stream_cap {
            return json_reply(
                StatusCode::TOO_MANY_REQUESTS,
                json!({
                    "object": "error",
                    "status": 429,
                    "code": "rate_limit_exceeded",
                    "message": "Too many search streams.",
                }),
            );
        }
        mock.stream_open += 1;
        mock.last_stream_body = parsed.clone();
        mock.live.clone()
    };
    let mut rx = live.subscribe();
    let (rows, gen) = {
        let mock = state.lock().expect("mock");
        (snapshot_rows(&mock, &parsed), mock.gen)
    };
    let (tx, out) = mpsc::channel(32);
    let slot = Slot(state.clone());
    tokio::spawn(async move {
        let _slot = slot;
        pump(&state, &parsed, rows, gen, heartbeat, &mut rx, &tx).await;
    });
    let stream =
        ReceiverStream::new(out).map(|chunk| Ok::<Bytes, std::io::Error>(Bytes::from(chunk)));
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}

async fn pump(
    state: &Shared,
    body: &Value,
    rows: Vec<Value>,
    mut seen: u64,
    heartbeat: Duration,
    rx: &mut tokio::sync::broadcast::Receiver<Live>,
    tx: &mpsc::Sender<String>,
) {
    let mut announced = BTreeSet::new();
    for row in rows {
        let Some(id) = row["id"].as_str() else {
            continue;
        };
        announced.insert(id.to_string());
        if tx.send(frame("object_added", &row)).await.is_err() {
            return;
        }
    }
    if tx
        .send(frame("snapshot_complete", &json!({})))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            _ = tokio::time::sleep(heartbeat) => {
                if tx.send(": keepalive\n\n".to_string()).await.is_err() {
                    return;
                }
            }
            event = rx.recv() => {
                match event {
                    Ok(Live::DropAll) | Err(_) => return,
                    Ok(Live::Changed { id, gen }) => {
                        if gen <= seen {
                            continue;
                        }
                        seen = gen;
                        if !emit_change(state, body, &id, &mut announced, tx).await {
                            return;
                        }
                    }
                }
            }
        }
    }
}

async fn emit_change(
    state: &Shared,
    body: &Value,
    id: &str,
    announced: &mut BTreeSet<String>,
    tx: &mpsc::Sender<String>,
) -> bool {
    let row = {
        let mock = state.lock().expect("mock");
        if mock.stream_omit.contains(id) {
            None
        } else {
            mock.objects
                .get(id)
                .and_then(|object| one_row(object, body))
        }
    };
    match row {
        Some(row) => {
            let name = if announced.insert(id.to_string()) {
                "object_added"
            } else {
                "object_updated"
            };
            tx.send(frame(name, &row)).await.is_ok()
        }
        None if announced.remove(id) => tx
            .send(frame("object_removed", &json!({"id": id})))
            .await
            .is_ok(),
        None => true,
    }
}

fn snapshot_rows(mock: &MockState, body: &Value) -> Vec<Value> {
    let objects: Vec<_> = mock
        .objects
        .values()
        .filter(|object| !mock.stream_omit.contains(&object.id))
        .collect();
    search(&objects, body)
}

fn one_row(object: &super::state::Object, body: &Value) -> Option<Value> {
    search(&[object], body).into_iter().next()
}

fn frame(event: &str, data: &Value) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

fn json_reply(status: StatusCode, body: Value) -> axum::response::Response {
    (status, Json(body)).into_response()
}
