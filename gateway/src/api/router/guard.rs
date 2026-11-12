use super::*;

// ---------- middleware ----------

pub(crate) fn loopback_origin(origin: &str) -> bool {
    url::Url::parse(origin)
        .ok()
        .is_some_and(|u| matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
}

/// Same-origin check for changes, and CORS for the Exact dev loop (`exact.mjs web` on another
/// loopback port). Native Exact hosts send no Origin and authenticate with a bearer.
pub(crate) async fn guard(State(s): S, req: Request, next: Next) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|o| o.to_str().ok())
        .map(String::from);
    let cors = origin.clone().filter(|o| {
        crate::platform::config::dev_allow_any_origin()
            || loopback_origin(o)
            || s.cfg.allowed_origins.contains(o)
    });
    if req.method() == Method::OPTIONS {
        let mut res = StatusCode::NO_CONTENT.into_response();
        if let Some(o) = cors {
            add_cors(res.headers_mut(), &o);
        }
        return res;
    }
    let path = req.uri().path();
    let guarded = path.starts_with("/api/") || path.starts_with("/t3/");
    if guarded
        && crate::network::auth::needs_origin_check(req.method())
        && !s.auth.same_origin(req.headers())
    {
        return ApiError::new(403, "cross-site request refused").into_response();
    }
    let mut res = next.run(req).await;
    if let Some(o) = cors {
        add_cors(res.headers_mut(), &o);
    }
    res
}

pub(crate) fn add_cors(h: &mut HeaderMap, origin: &str) {
    if let Ok(v) = HeaderValue::from_str(origin) {
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, v);
    }
    h.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("authorization, content-type"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"),
    );
    h.insert(header::VARY, HeaderValue::from_static("origin"));
}

pub(crate) fn bearer(h: &HeaderMap) -> Option<&str> {
    h.get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// Who may use the app on this listener: the signed-in Tailscale user on the tailnet
/// listener; elsewhere a valid Cloudflare Access token, or a session when a password is set; with
/// neither, open only when Cloudflare Access isn't configured either.
pub(crate) async fn authed(
    s: &AppState,
    h: &HeaderMap,
    surface: crate::network::exposure::Surface,
) -> bool {
    if surface == crate::network::exposure::Surface::Tailnet {
        return crate::network::exposure::tailnet_user_allowed(h, &s.tailnet.users());
    }
    if let Some(access) = &s.access {
        if let Some(token) = crate::network::access::token_from(h) {
            if access.verify(&token).await.is_some() {
                return true;
            }
        }
    }
    if s.remote.password_required() {
        return crate::network::auth::cookie(h, crate::network::auth::COOKIE)
            .is_some_and(|c| s.auth.valid_session(&c))
            || bearer(h).is_some_and(|b| s.auth.valid_session(b));
    }
    // No password is set (BOT_PASSWORD and Settings both empty), so `Auth::authed` is true here.
    s.access.is_none() && s.auth.authed(h)
}

pub(crate) fn surface_of(req: &Request) -> crate::network::exposure::Surface {
    req.extensions()
        .get::<crate::network::exposure::Surface>()
        .copied()
        .unwrap_or(crate::network::exposure::Surface::Public)
}

pub(crate) async fn require_login(State(s): S, req: Request, next: Next) -> Response {
    if !authed(&s, req.headers(), surface_of(&req)).await {
        return ApiError::new(401, "login required").into_response();
    }
    next.run(req).await
}

pub(crate) fn require_json(h: &HeaderMap) -> ApiResult<()> {
    let ct = h
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if ct.to_ascii_lowercase().starts_with("application/json") {
        Ok(())
    } else {
        Err(ApiError::new(415, "content-type must be application/json"))
    }
}

pub(crate) fn parse(body: &Bytes) -> ApiResult<Value> {
    if body.is_empty() {
        return Ok(json!({}));
    }
    // serde's message can quote the input, which may hold a password.
    #[allow(
        clippy::map_err_ignore,
        reason = "serde quotes the body, which may hold a password"
    )]
    serde_json::from_slice(body).map_err(|_| ApiError::bad_request("invalid JSON body"))
}

// ---------- sign-in ----------

pub(crate) async fn session(
    State(s): S,
    surface: Option<Extension<crate::network::exposure::Surface>>,
    h: HeaderMap,
) -> Json<Value> {
    let surface = surface.map_or(crate::network::exposure::Surface::Public, |Extension(x)| x);
    let needs_password = s.remote.password_required();
    // Cloudflare's login only helps requests that came through Cloudflare; on this machine or the LAN, use the password.
    let access_login = s.access.is_some() && !(needs_password && !h.contains_key("cf-ray"));
    Json(json!({
        "authenticated": authed(&s, &h, surface).await,
        "passwordRequired": needs_password,
        "accessLogin": access_login,
    }))
}

pub(crate) async fn login(
    State(s): S,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let ip = h
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| peer.ip().to_string());
    if s.auth.login_blocked(&ip) {
        return Err(ApiError::new(
            429,
            "Too many attempts. Try again in 15 minutes.",
        ));
    }
    if body.len() > 4096 {
        return Err(ApiError::new(413, "body too large"));
    }
    let password = parse(&body)?["password"].as_str().unwrap_or("").to_string();
    // HBOTS_PASSWORD is checked as before; otherwise the password saved in Settings.
    let ok = if s.remote.password_from_env() {
        s.auth.password_matches(&password)
    } else {
        !s.remote.password_required() || s.remote.password_matches(&password).await
    };
    if !ok {
        s.auth.login_failed(&ip);
        tracing::warn!("failed login from {ip}");
        return Err(ApiError::new(401, "wrong password"));
    }
    s.auth.login_ok(&ip);
    let token = s.auth.make_session();
    let https = h.get("x-forwarded-proto").is_some_and(|v| v == "https");
    // Browsers keep the cookie; native apps keep `token` in the Keychain and send it as a bearer.
    Ok((
        [(
            header::SET_COOKIE,
            crate::network::auth::session_cookie(&token, https),
        )],
        Json(json!({ "ok": true, "token": token })),
    )
        .into_response())
}

pub(crate) async fn logout() -> Response {
    (
        [(
            header::SET_COOKIE,
            format!("{}=; Path=/; Max-Age=0", crate::network::auth::COOKIE),
        )],
        Json(json!({ "ok": true })),
    )
        .into_response()
}

/// The address a request was made to, `scheme://host[:port]`, as the browser sees it behind a proxy.
pub(crate) fn request_origin(h: &HeaderMap) -> String {
    let host = h
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1");
    // Only a plain name or address with an optional port may go into a script.
    let host = if host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
    {
        host
    } else {
        "127.0.0.1"
    };
    let proto = match h.get("x-forwarded-proto").and_then(|v| v.to_str().ok()) {
        Some("https") => "https",
        _ => "http",
    };
    format!("{proto}://{host}")
}
