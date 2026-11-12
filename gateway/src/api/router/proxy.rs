use super::*;

// ---------- T3 passthrough ----------

pub(crate) async fn t3_route(
    State(s): S,
    ws: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>,
    method: Method,
    uri: Uri,
    h: HeaderMap,
    body: Bytes,
) -> Response {
    let Ok(relay) = s.harness.relay() else {
        return ApiError::not_found("not found").into_response();
    };
    let rest = uri.path().trim_start_matches("/t3");
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    if let Ok(ws) = ws {
        let target = match rest {
            "/ws" => {
                let mut u = format!(
                    "{}/ws?orchestrationProtocol=2&clientSurface=web&clientAppVersion=bots-rs",
                    relay.ws_base()
                );
                if let Some(q) = uri.query() {
                    u.push('&');
                    u.push_str(q);
                }
                u
            }
            "/api/preview-stream/ws" => {
                format!("{}{rest}{query}", relay.ws_base())
            }
            _ => return ApiError::not_found("not found").into_response(),
        };
        let close_sockets = s.auth.close_sockets.clone();
        let harness = s.harness.clone();
        return ws.on_upgrade(move |socket| async move {
            if let Ok(relay) = harness.relay() {
                relay.proxy_ws(socket, target, close_sockets).await;
            }
        });
    }
    if !rest.starts_with("/api/") {
        return ApiError::not_found("not found").into_response();
    }
    relay
        .proxy_http(method, &format!("{rest}{query}"), &h, body)
        .await
}

// ---------- the Bots' tool server ----------

pub(crate) async fn mcp_route(
    State(s): S,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    uri: Uri,
    h: HeaderMap,
    body: Bytes,
) -> Response {
    let via_tunnel = [
        "cf-ray",
        "cf-connecting-ip",
        "x-forwarded-for",
        "cf-access-jwt-assertion",
    ]
    .iter()
    .any(|k| h.contains_key(*k));
    if via_tunnel || !peer.ip().is_loopback() {
        return ApiError::new(403, "The Bot tool server only accepts local requests.")
            .into_response();
    }
    if !bearer(&h).is_some_and(|b| crate::network::auth::safe_equal(b, &s.mcp_token)) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
    }
    if method != Method::POST {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [(header::ALLOW, "POST")],
            Json(json!({ "error": "Method not allowed" })),
        )
            .into_response();
    }
    let Ok(msg) = serde_json::from_slice::<Value>(&body) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "Parse error" } }))).into_response();
    };
    let batch = msg.is_array();
    let mut replies = Vec::new();
    let slug = crate::bots::mcp_identity::bot_slug_from_query(uri.query());
    for m in msg.as_array().cloned().unwrap_or_else(|| vec![msg]) {
        if let Some(r) = crate::platform::mcp::handle(&s, &m, slug.as_deref()).await {
            replies.push(r);
        }
    }
    if replies.is_empty() {
        return StatusCode::ACCEPTED.into_response();
    }
    let out = if batch {
        json!(replies)
    } else {
        replies.remove(0)
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(out)).into_response()
}
