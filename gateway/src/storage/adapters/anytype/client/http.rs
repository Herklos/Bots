//! Shared HTTP: version header, bearer auth, idempotency, If-Match, and error mapping.
//! The legacy helpers keep the pairing messages Settings already shows.

use super::error::AnytypeError;
use super::limiter::Limiter;
use super::AnytypeClient;
use crate::platform::error::{ApiError, ApiResult};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::time::Duration;

pub(super) const API_VERSION: &str = "2025-11-08";
pub(super) const APP_NAME: &str = "hbots";

pub(super) fn legacy_client() -> ApiResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_error| ApiError::new(500, "Could not build the Anytype client."))
}

pub(super) fn shared_client() -> Result<reqwest::Client, AnytypeError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_error| AnytypeError::Unreachable)
}

pub(super) fn legacy_unreachable(_error: reqwest::Error) -> ApiError {
    ApiError::new(
        502,
        "Anytype is not reachable. Open the Anytype app and check the host.",
    )
}

pub(super) async fn legacy_json(response: reqwest::Response, what: &str) -> ApiResult<Value> {
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(body);
    }
    let detail = body["message"].as_str().unwrap_or("").to_string();
    let code = match status.as_u16() {
        400 | 404 => 400,
        401 | 403 => 403,
        _ => 502,
    };
    let message = if detail.is_empty() {
        format!("Anytype refused {what} ({}).", status.as_u16())
    } else {
        format!("Anytype refused {what}: {detail}")
    };
    Err(ApiError::new(code, message))
}

pub(super) struct JsonCall<'a> {
    pub method: reqwest::Method,
    pub path: &'a str,
    pub idem_key: Option<&'a str>,
    pub etag: Option<&'a str>,
    pub body: Option<&'a Value>,
}

pub(super) async fn send_json(
    client: &AnytypeClient,
    call: JsonCall<'_>,
) -> Result<Value, AnytypeError> {
    client.limiter.acquire().await;
    let mut request = client
        .http
        .request(call.method, format!("{}{}", client.host, call.path))
        .header("Anytype-Version", API_VERSION)
        .bearer_auth(&client.key);
    if let Some(key) = call.idem_key {
        request = request.header("Idempotency-Key", key);
    }
    if let Some(etag) = call.etag {
        request = request.header("If-Match", etag);
    }
    if let Some(body) = call.body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|_error| AnytypeError::Unreachable)?;
    read_value(response, &client.limiter).await
}

pub(super) async fn send_raw(
    client: &AnytypeClient,
    method: reqwest::Method,
    path: &str,
    idem_key: Option<&str>,
    content_type: &str,
    body: Vec<u8>,
) -> Result<Value, AnytypeError> {
    client.limiter.acquire().await;
    let mut request = client
        .http
        .request(method, format!("{}{path}", client.host))
        .header("Anytype-Version", API_VERSION)
        .header("content-type", content_type)
        .bearer_auth(&client.key)
        .body(body);
    if let Some(key) = idem_key {
        request = request.header("Idempotency-Key", key);
    }
    let response = request
        .send()
        .await
        .map_err(|_error| AnytypeError::Unreachable)?;
    read_value(response, &client.limiter).await
}

pub(super) async fn send_bytes(
    client: &AnytypeClient,
    path: &str,
) -> Result<Vec<u8>, AnytypeError> {
    client.limiter.acquire().await;
    let response = client
        .http
        .get(format!("{}{path}", client.host))
        .header("Anytype-Version", API_VERSION)
        .bearer_auth(&client.key)
        .send()
        .await
        .map_err(|_error| AnytypeError::Unreachable)?;
    let status = response.status();
    if !status.is_success() {
        let headers = response.headers().clone();
        let text = response.text().await.unwrap_or_default();
        return Err(map_failure(status, &headers, &text, &client.limiter));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|_error| AnytypeError::Unreachable)
}

fn require_key(idem_key: &str) -> Result<(), AnytypeError> {
    if idem_key.is_empty() {
        return Err(AnytypeError::Bad {
            status: 400,
            message: "An idempotency key is required.".into(),
        });
    }
    Ok(())
}

pub(super) fn require_idempotency(idem_key: &str) -> Result<(), AnytypeError> {
    require_key(idem_key)
}

async fn read_value(response: reqwest::Response, limiter: &Limiter) -> Result<Value, AnytypeError> {
    let status = response.status();
    let headers = response.headers().clone();
    let text = response
        .text()
        .await
        .map_err(|_error| AnytypeError::Unreachable)?;
    if !status.is_success() {
        return Err(map_failure(status, &headers, &text, limiter));
    }
    serde_json::from_str(&text).map_err(|_error| AnytypeError::ApiChanged)
}

fn map_failure(
    status: StatusCode,
    headers: &HeaderMap,
    text: &str,
    limiter: &Limiter,
) -> AnytypeError {
    let body: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let code = body["code"].as_str().unwrap_or("");
    let message = body["message"].as_str().unwrap_or("").to_string();
    match status.as_u16() {
        401 | 403 => AnytypeError::NotGranted,
        404 | 410 => AnytypeError::Gone,
        429 => {
            limiter.shrink();
            AnytypeError::RateLimited {
                retry_after: retry_after(headers),
            }
        }
        409 if code == "object_archived" || body["archived"] == json!(true) => {
            AnytypeError::ObjectArchived
        }
        409 if code == "etag_mismatch" || body.get("etag").is_some() => {
            AnytypeError::EtagMismatch {
                current: body["etag"].as_str().map(str::to_string),
            }
        }
        _ => AnytypeError::Bad {
            status: status.as_u16(),
            message,
        },
    }
}

fn retry_after(headers: &HeaderMap) -> Duration {
    headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|text| text.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_default()
}
