//! Search stream for `hb_file`. One SSE response: a snapshot, then live changes.
//!
//! The body is a search with no `query` and only the cheap fields. Content stays off
//! this connection. A reconnect is a new request and a fresh snapshot. The caller
//! backs off and diffs that snapshot; this module only opens and parses.

use super::error::AnytypeError;
use super::http::{self, API_VERSION};
use super::AnytypeClient;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::pin::Pin;
use std::time::Duration;

/// Fields a stream may carry. `hb_content` and `hb_blob` are never requested.
const CHEAP_FIELDS: &[&str] = &[
    "hb_path",
    "hb_sha",
    "hb_deleted",
    "hb_device",
    "hb_rev",
    "last_modified_date",
    "hb_size",
];

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StreamEvent {
    Added(Value),
    Updated(Value),
    Removed { id: String },
    SnapshotComplete,
    Heartbeat,
}

pub(crate) struct FileStream {
    parser: SseParser,
    incoming: Pin<Box<dyn futures_util::Stream<Item = Result<Vec<u8>, reqwest::Error>> + Send>>,
}

impl FileStream {
    pub(crate) async fn next_event(&mut self) -> Result<Option<StreamEvent>, AnytypeError> {
        loop {
            if let Some(event) = self.parser.pop() {
                return Ok(Some(event));
            }
            match self.incoming.next().await {
                None => return Ok(self.parser.pop()),
                Some(Err(_error)) => return Err(AnytypeError::Unreachable),
                Some(Ok(bytes)) => self.parser.push(&bytes),
            }
        }
    }
}

impl AnytypeClient {
    /// `POST /v2/spaces/{id}/search/stream?heartbeat=<seconds>` for `hb_file`.
    pub(crate) async fn open_file_stream(
        &self,
        heartbeat: Duration,
    ) -> Result<FileStream, AnytypeError> {
        let seconds = heartbeat.as_secs().clamp(1, 120);
        let path = format!(
            "/v2/spaces/{}/search/stream?heartbeat={seconds}",
            self.space_id
        );
        let body = json!({
            "type": "hb_file",
            "fields": CHEAP_FIELDS,
        });
        let response = self
            .stream_http
            .post(format!("{}{path}", self.host))
            .header("Anytype-Version", API_VERSION)
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .map_err(|_error| AnytypeError::Unreachable)?;
        if !response.status().is_success() {
            return Err(stream_failure(response).await);
        }
        let incoming = response
            .bytes_stream()
            .map(|item| item.map(|bytes| bytes.to_vec()));
        Ok(FileStream {
            parser: SseParser::default(),
            incoming: Box::pin(incoming),
        })
    }
}

async fn stream_failure(response: reqwest::Response) -> AnytypeError {
    let status = response.status();
    let headers = response.headers().clone();
    let text = response.text().await.unwrap_or_default();
    let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let message = body["message"].as_str().unwrap_or("").to_string();
    match status.as_u16() {
        401 | 403 => AnytypeError::NotGranted,
        404 | 410 => AnytypeError::Gone,
        429 => AnytypeError::RateLimited {
            retry_after: http::retry_after(&headers),
        },
        _ => AnytypeError::Bad {
            status: status.as_u16(),
            message,
        },
    }
}

#[derive(Debug, Default)]
struct SseParser {
    buf: Vec<u8>,
    ready: VecDeque<StreamEvent>,
}

impl SseParser {
    fn push(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
        while let Some(frame) = split_frame(&mut self.buf) {
            if let Some(event) = parse_frame(&frame) {
                self.ready.push_back(event);
            }
        }
    }

    fn pop(&mut self) -> Option<StreamEvent> {
        self.ready.pop_front()
    }
}

fn split_frame(buf: &mut Vec<u8>) -> Option<String> {
    let (at, len) = find_sep(buf)?;
    let frame = buf[..at].to_vec();
    buf.drain(..at + len);
    Some(String::from_utf8(frame).unwrap_or_default())
}

fn find_sep(buf: &[u8]) -> Option<(usize, usize)> {
    let crlf = find_slice(buf, b"\r\n\r\n").map(|at| (at, 4));
    let lf = find_slice(buf, b"\n\n").map(|at| (at, 2));
    match (crlf, lf) {
        (Some(cr), Some(lf)) if cr.0 <= lf.0 => Some(cr),
        (Some(_cr), Some(lf)) => Some(lf),
        (Some(cr), None) => Some(cr),
        (None, Some(lf)) => Some(lf),
        (None, None) => None,
    }
}

fn find_slice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_frame(frame: &str) -> Option<StreamEvent> {
    let mut name = String::new();
    let mut data = String::new();
    let mut comment = false;
    for line in frame.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some(_rest) = line.strip_prefix(':') {
            comment = true;
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            name = rest.trim().to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    if name.is_empty() && data.is_empty() && comment {
        return Some(StreamEvent::Heartbeat);
    }
    match name.as_str() {
        "object_added" => parse_row(&data).map(StreamEvent::Added),
        "object_updated" => parse_row(&data).map(StreamEvent::Updated),
        "object_removed" => removed_id(&data).map(|id| StreamEvent::Removed { id }),
        "snapshot_complete" => Some(StreamEvent::SnapshotComplete),
        _ => None,
    }
}

fn parse_row(data: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(data).ok()?;
    value.as_object().is_some().then_some(value)
}

fn removed_id(data: &str) -> Option<String> {
    let value: Value = serde_json::from_str(data).ok()?;
    value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{parse_frame, SseParser, StreamEvent};

    #[test]
    fn frames_split_across_chunks_and_a_comment_is_a_heartbeat() {
        let mut parser = SseParser::default();
        parser.push(b"event: object_added\ndata: {\"id\":\"obj-1\"");
        assert!(parser.pop().is_none());
        parser.push(b"}\n\n: keepalive\n\n");
        match parser.pop() {
            Some(StreamEvent::Added(row)) => assert_eq!(row["id"], "obj-1"),
            other => panic!("expected an added row, got {other:?}"),
        }
        assert_eq!(parser.pop(), Some(StreamEvent::Heartbeat));
        assert!(parser.pop().is_none());
    }

    #[test]
    fn known_events_parse_and_a_bad_row_is_skipped() {
        assert!(matches!(
            parse_frame("event: snapshot_complete\ndata: {}"),
            Some(StreamEvent::SnapshotComplete)
        ));
        assert_eq!(
            parse_frame("event: object_removed\ndata: {\"id\":\"obj-9\"}"),
            Some(StreamEvent::Removed { id: "obj-9".into() })
        );
        assert!(parse_frame("event: object_updated\ndata: not-json").is_none());
        assert!(parse_frame("event: other\ndata: {}").is_none());
    }
}
