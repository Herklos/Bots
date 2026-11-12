use super::*;

pub(crate) fn access_body(body: &Bytes) -> ApiResult<Value> {
    parse(body)
}

pub(crate) async fn access_view(s: &AppState) -> Value {
    let mut out = s.remote.status().await;
    out["cloudflareAccess"] = match &s.access {
        Some(access) => json!({ "emails": access.emails() }),
        None => Value::Null,
    };
    if let Some(network) = out
        .get_mut("network")
        .and_then(|value| value.as_object_mut())
    {
        network.insert(
            "canRestart".into(),
            json!(service_can_restart(&s.cfg.data_root)),
        );
    }
    out
}

pub(crate) async fn access_status(State(s): S) -> Json<Value> {
    Json(access_view(&s).await)
}

pub(crate) async fn access_network(
    State(s): S,
    body: Bytes,
) -> Result<Json<Value>, remote_access::AccessError> {
    let body =
        access_body(&body).map_err(|e| remote_access::AccessError::bad_request(e.message))?;
    let mode = body["mode"].as_str().unwrap_or("");
    s.remote.set_network(mode, tailnet_app(&s)).await?;
    Ok(Json(access_view(&s).await))
}

pub(crate) async fn access_network_restart(
    State(s): S,
) -> Result<Json<Value>, remote_access::AccessError> {
    if !service_can_restart(&s.cfg.data_root) {
        return Err(remote_access::AccessError::conflict(
            "Restart hbots to apply this.",
        ));
    }
    let home = s.cfg.data_root.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let spec = cli::service_spec(&home);
        let os = std::env::consts::OS;
        match tokio::task::spawn_blocking(move || service::restart(&spec, os)).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => tracing::error!("restarting hbots: {err:#}"),
            Err(err) => tracing::error!("restarting hbots: {err}"),
        }
    });
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn access_password(
    State(s): S,
    h: HeaderMap,
    body: Bytes,
) -> Result<Response, remote_access::AccessError> {
    let body =
        access_body(&body).map_err(|e| remote_access::AccessError::bad_request(e.message))?;
    // A missing or null password removes it, like `password ?? null` in the Node gateway.
    let password = match &body["password"] {
        Value::Null => None,
        Value::String(p) => Some(p.clone()),
        _ => Some(String::new()),
    };
    let setting = password.is_some();
    s.remote.set_password(password).await?;
    // Everyone signs in again with the new password; this browser gets a fresh session now.
    s.auth
        .rotate()
        .map_err(remote_access::AccessError::internal)?;
    let https = h.get("x-forwarded-proto").is_some_and(|v| v == "https");
    let mut res = Json(s.remote.status().await).into_response();
    if setting {
        let token = s.auth.make_session();
        if let Ok(value) = HeaderValue::from_str(&auth::session_cookie(&token, https)) {
            res.headers_mut().insert(header::SET_COOKIE, value);
        }
    }
    Ok(res)
}

pub(crate) async fn access_tailscale(
    State(s): S,
    body: Bytes,
) -> Result<Json<Value>, remote_access::AccessError> {
    let body =
        access_body(&body).map_err(|e| remote_access::AccessError::bad_request(e.message))?;
    if body["on"].as_bool() == Some(true) {
        s.remote.tailscale_on(tailnet_app(&s)).await?;
    } else {
        s.remote.tailscale_off().await?;
    }
    Ok(Json(s.remote.status().await))
}

pub(crate) async fn access_tunnel(
    State(s): S,
    body: Bytes,
) -> Result<Json<Value>, remote_access::AccessError> {
    let body =
        access_body(&body).map_err(|e| remote_access::AccessError::bad_request(e.message))?;
    if body["on"].as_bool() == Some(true) {
        s.remote.tunnel_on().await?;
    } else {
        s.remote.tunnel_off().await?;
    }
    Ok(Json(s.remote.status().await))
}
