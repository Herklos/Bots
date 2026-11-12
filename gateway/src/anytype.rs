//! Thin client for the local Anytype HTTP API (v2): the pairing flow, `whoami`, and the
//! space check behind Settings → Storage. Pairing and key checks need no MCP and no
//! Node. Nothing here returns or logs the API key.

use crate::error::{ApiError, ApiResult};
use serde_json::{json, Value};
use std::time::Duration;

const API_VERSION: &str = "2025-11-08";
const APP_NAME: &str = "hbots";

fn client() -> ApiResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_error| ApiError::new(500, "Could not build the Anytype client."))
}

fn unreachable(_error: reqwest::Error) -> ApiError {
    ApiError::new(
        502,
        "Anytype is not reachable. Open the Anytype app and check the host.",
    )
}

async fn read_json(response: reqwest::Response, what: &str) -> ApiResult<Value> {
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

/// Starts pairing. Anytype Desktop then shows an approval dialog.
pub(crate) async fn start_pairing(host: &str) -> ApiResult<String> {
    let response = client()?
        .post(format!("{host}/v2/auth/challenges"))
        .header("Anytype-Version", API_VERSION)
        .json(&json!({ "app_name": APP_NAME }))
        .send()
        .await
        .map_err(unreachable)?;
    let body = read_json(response, "the pairing request").await?;
    body["challenge_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ApiError::new(502, "Anytype did not return a pairing challenge."))
}

/// Exchanges the 4 digit code for a key. Returns `(key, grant)`.
pub(crate) async fn confirm_pairing(
    host: &str,
    challenge_id: &str,
    code: &str,
) -> ApiResult<(String, Value)> {
    let response = client()?
        .post(format!("{host}/v2/auth/api_keys"))
        .header("Anytype-Version", API_VERSION)
        .json(&json!({ "challenge_id": challenge_id, "code": code }))
        .send()
        .await
        .map_err(unreachable)?;
    let body = read_json(response, "that code").await?;
    let key = body["api_key"]
        .as_str()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ApiError::new(502, "Anytype did not return a key."))?
        .to_string();
    Ok((key, body["grant"].clone()))
}

async fn get(host: &str, key: &str, path: &str) -> ApiResult<Value> {
    let response = client()?
        .get(format!("{host}{path}"))
        .header("Anytype-Version", API_VERSION)
        .bearer_auth(key)
        .send()
        .await
        .map_err(unreachable)?;
    read_json(response, "the request").await
}

/// What the key reaches and whether the chosen space is private. No secrets in the result.
pub(crate) async fn status(host: &str, key: &str, space_name: &str) -> ApiResult<Value> {
    let who = get(host, key, "/v2/auth/whoami?ids=full").await?;
    let spaces = get(host, key, "/v2/spaces?ids=full").await?;
    let names: Vec<Value> = spaces["data"]
        .as_array()
        .map(|rows| rows.iter().map(|row| row["name"].clone()).collect())
        .unwrap_or_default();
    let chosen = spaces["data"].as_array().and_then(|rows| {
        rows.iter()
            .find(|row| row["name"].as_str() == Some(space_name))
    });
    let (space_found, shared) = match chosen {
        Some(row) => {
            let id = row["id"].as_str().unwrap_or_default();
            let members = get(host, key, &format!("/v2/spaces/{id}/members")).await?;
            let count = members["data"].as_array().map(Vec::len).unwrap_or(0);
            (true, Some(count > 1))
        }
        None => (false, None),
    };
    Ok(json!({
        "reachable": true,
        "api": who["api"]["version"],
        "keyName": who["key"]["name"],
        "scoped": who["grant"]["scoped"],
        "restricted": who["grant"]["restricted"],
        "permission": who["grant"]["permission"],
        "spaces": names,
        "hasNotGrantedSpaces": spaces["has_not_granted_spaces"],
        "space": space_name,
        "spaceFound": space_found,
        "shared": shared,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::{get, post};
    use axum::{Json, Router};

    async fn mock() -> String {
        let app = Router::new()
            .route(
                "/v2/auth/challenges",
                post(|| async { Json(json!({ "challenge_id": "CH1" })) }),
            )
            .route(
                "/v2/auth/api_keys",
                post(|Json(body): Json<Value>| async move {
                    if body["code"] == "1234" && body["challenge_id"] == "CH1" {
                        (
                            axum::http::StatusCode::CREATED,
                            Json(json!({ "api_key": "anytype_secret", "grant": { "permission": "readwrite" } })),
                        )
                    } else {
                        (
                            axum::http::StatusCode::BAD_REQUEST,
                            Json(json!({ "message": "wrong code" })),
                        )
                    }
                }),
            )
            .route(
                "/v2/auth/whoami",
                get(|| async {
                    Json(json!({ "api": { "version": "2025-11-08" }, "key": { "name": "hbots" },
                        "grant": { "scoped": true, "restricted": true, "permission": "readwrite" } }))
                }),
            )
            .route(
                "/v2/spaces",
                get(|| async {
                    Json(json!({ "has_not_granted_spaces": true,
                        "data": [{ "id": "s1", "name": "hbots" }, { "id": "s2", "name": "other" }] }))
                }),
            )
            .route(
                "/v2/spaces/{id}/members",
                get(|| async { Json(json!({ "data": [{ "name": "me" }] })) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn pairing_exchanges_a_code_and_a_wrong_code_is_refused() {
        let host = mock().await;
        let challenge = start_pairing(&host).await.unwrap();
        let (key, grant) = confirm_pairing(&host, &challenge, "1234").await.unwrap();
        assert_eq!(key, "anytype_secret");
        assert_eq!(grant["permission"], "readwrite");
        let error = confirm_pairing(&host, &challenge, "0000")
            .await
            .unwrap_err();
        assert!(format!("{error:?}").contains("wrong code"));
    }

    #[tokio::test]
    async fn status_reports_scope_space_and_privacy_without_the_key() {
        let host = mock().await;
        let out = status(&host, "anytype_secret", "hbots").await.unwrap();
        assert_eq!(out["scoped"], true);
        assert_eq!(out["spaceFound"], true);
        assert_eq!(out["shared"], false);
        assert!(!out.to_string().contains("anytype_secret"));
        let missing = status(&host, "k", "nope").await.unwrap();
        assert_eq!(missing["spaceFound"], false);
    }

    #[tokio::test]
    async fn an_unreachable_host_is_a_readable_error() {
        let error = start_pairing("http://127.0.0.1:9").await.unwrap_err();
        assert!(format!("{error:?}").contains("not reachable"));
    }
}
