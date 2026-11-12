use super::client::*;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

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
