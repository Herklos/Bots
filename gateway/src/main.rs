//! `bots`: Threebot's gateway as one Rust binary.
//!
//! It sits between the Bots app (the Exact app in `app/`, on the web, macOS, iOS and Linux) and
//! T3 Code, and keeps what T3 doesn't: Bot folders, settings, the Vault, the Projects board and
//! the Bots' own MCP tool server. Its on-disk layout is Threebot's (`~/.threebot`), so either
//! gateway can run against the same home.
mod auth;
mod bots;
mod chat;
mod config;
mod error;
mod mcp;
mod projects;
mod provisioner;
mod store;
mod t3;
mod vault;

use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, Query, Request, State, WebSocketUpgrade},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{any, get, post, put},
    Json, Router,
};
use error::{ApiError, ApiResult};
use futures_util::{stream, Stream, StreamExt};
use serde_json::{json, Value};
use std::{collections::HashMap, convert::Infallible, net::SocketAddr, sync::Arc};
use tokio_stream::wrappers::BroadcastStream;

pub struct AppState {
    pub cfg: config::Config,
    pub auth: auth::Auth,
    pub t3: t3::T3,
    pub bots: bots::Bots,
    pub vault: vault::Vault,
    pub projects: projects::Projects,
    pub mcp_token: String,
    pub provision_now: tokio::sync::Notify,
}

impl AppState {
    fn settings_path(&self) -> std::path::PathBuf {
        self.cfg.app_dir().join("settings.json")
    }

    /// `settings.json` with Threebot's defaults filled in.
    pub async fn settings(&self) -> Value {
        let saved = store::read_json(&self.settings_path())
            .await
            .unwrap_or_else(|| json!({}));
        let defaults = json!({
            "userName": "", "theme": "dark", "palette": "amoled", "accent": "bot", "timezone": "", "autoReview": true,
            "rules": { "alwaysAsk": [], "allowWithoutAsking": [] }, "sections": [], "defaultModel": null, "avatarDrawingModel": null,
        });
        let mut out = store::merge(&defaults, &saved);
        out["beta"] = json!({ "projects": saved["beta"]["projects"] == true });
        out
    }
}

type S = State<Arc<AppState>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("bots {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let cfg = config::Config::from_env()?;
    let app_dir = cfg.app_dir();
    std::fs::create_dir_all(&app_dir)?;
    // The Bots' tool-server bearer: BOT_MCP_TOKEN, or a random one next to the vault key (as Threebot).
    let token_path = cfg
        .vault_key_file
        .parent()
        .unwrap_or(&cfg.data_root)
        .join(".bot-mcp-token");
    let mcp_token = match std::env::var("BOT_MCP_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| std::fs::read_to_string(&token_path).ok())
    {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => {
            let t = hex::encode(rand::random::<[u8; 32]>());
            auth::write_private(&token_path, &(t.clone() + "\n"))?;
            t
        }
    };
    let state = Arc::new(AppState {
        auth: auth::Auth::load(&app_dir, cfg.password.clone(), cfg.allowed_origins.clone())?,
        t3: t3::T3::new(cfg.t3_url.clone(), cfg.t3_token.clone()),
        bots: bots::Bots::new(
            cfg.bots_root.clone(),
            cfg.bot_mcp_url.clone(),
            mcp_token.clone(),
        ),
        vault: vault::Vault::new(cfg.vault_file.clone(), cfg.vault_key_file.clone()),
        projects: projects::Projects::new(app_dir.join("projects.json")),
        mcp_token,
        provision_now: tokio::sync::Notify::new(),
        cfg,
    });
    // Instructions may have changed with this version.
    for bot in state
        .bots
        .list()
        .await
        .map_err(|e| anyhow::anyhow!(e.message))?
    {
        if let Err(e) = state.bots.write_instructions(&bot).await {
            tracing::warn!("refreshing instructions for {}: {}", bot["slug"], e.message);
        }
    }
    provisioner::start(state.clone());

    let addr: SocketAddr = format!("{}:{}", state.cfg.host, state.cfg.port).parse()?;
    tracing::info!(
        "bots gateway on http://{addr} → T3 {}, bots in {}",
        state.cfg.t3_url,
        state.cfg.bots_root.display()
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

pub fn app(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/api/config", get(config_route))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/bots", get(list_bots).post(create_bot))
        .route(
            "/api/bots/{slug}",
            get(get_bot).put(save_bot).delete(delete_bot),
        )
        .route("/api/bots/{slug}/chat", get(read_chat).post(send_chat))
        .route("/api/bots/{slug}/stop", post(stop_chat))
        .route("/api/main-bot", put(main_bot))
        .route("/api/vault", get(vault_list).post(vault_create))
        .route(
            "/api/vault/{id}",
            get(vault_get).put(vault_update).delete(vault_delete),
        )
        .route("/api/projects", get(projects_board))
        .route("/api/projects/items", post(projects_save))
        .route("/api/projects/delete", post(projects_delete))
        .route("/api/projects/events", get(projects_events))
        .route("/t3/{*rest}", any(t3_route))
        .layer(middleware::from_fn_with_state(state.clone(), require_login));
    Router::new()
        .route(
            "/api/health",
            get(|| async {
                Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION"), "gateway": "rust" }))
            }),
        )
        .route("/api/session", get(session))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout).get(logout))
        .route("/mcp/bot", any(mcp_route))
        .merge(api)
        .route(
            "/api/{*rest}",
            any(|| async { ApiError::not_found("not found") }),
        )
        .fallback(static_files)
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

// ---------- middleware ----------

fn loopback_origin(origin: &str) -> bool {
    url::Url::parse(origin)
        .ok()
        .is_some_and(|u| matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
}

/// Same-origin check for changes, and CORS for the Exact dev loop (`exact.mjs web` on another
/// loopback port). Native Exact hosts send no Origin and authenticate with a bearer.
async fn guard(State(s): S, req: Request, next: Next) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|o| o.to_str().ok())
        .map(String::from);
    let cors = origin
        .clone()
        .filter(|o| loopback_origin(o) || s.cfg.allowed_origins.contains(o));
    if req.method() == Method::OPTIONS {
        let mut res = StatusCode::NO_CONTENT.into_response();
        if let Some(o) = cors {
            add_cors(res.headers_mut(), &o);
        }
        return res;
    }
    let path = req.uri().path();
    let guarded = path.starts_with("/api/") || path.starts_with("/t3/");
    if guarded && auth::needs_origin_check(req.method()) && !s.auth.same_origin(req.headers()) {
        return ApiError::new(403, "cross-site request refused").into_response();
    }
    let mut res = next.run(req).await;
    if let Some(o) = cors {
        add_cors(res.headers_mut(), &o);
    }
    res
}

fn add_cors(h: &mut HeaderMap, origin: &str) {
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

fn bearer(h: &HeaderMap) -> Option<&str> {
    h.get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

fn authed(s: &AppState, h: &HeaderMap) -> bool {
    s.auth.authed(h) || bearer(h).is_some_and(|b| s.auth.valid_session(b))
}

async fn require_login(State(s): S, req: Request, next: Next) -> Response {
    if !authed(&s, req.headers()) {
        return ApiError::new(401, "login required").into_response();
    }
    next.run(req).await
}

fn require_json(h: &HeaderMap) -> ApiResult<()> {
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

fn parse(body: &Bytes) -> ApiResult<Value> {
    if body.is_empty() {
        return Ok(json!({}));
    }
    // serde's message can quote the input, which may hold a password.
    serde_json::from_slice(body).map_err(|_| ApiError::bad_request("invalid JSON body"))
}

// ---------- sign-in ----------

async fn session(State(s): S, h: HeaderMap) -> Json<Value> {
    Json(
        json!({ "authenticated": authed(&s, &h), "passwordRequired": s.auth.password_required(), "accessLogin": false }),
    )
}

async fn login(
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
    if !s.auth.password_matches(&password) {
        s.auth.login_failed(&ip);
        tracing::warn!("failed login from {ip}");
        return Err(ApiError::new(401, "wrong password"));
    }
    s.auth.login_ok(&ip);
    let token = s.auth.make_session();
    let https = h.get("x-forwarded-proto").is_some_and(|v| v == "https");
    // Browsers keep the cookie; native apps keep `token` in the Keychain and send it as a bearer.
    Ok((
        [(header::SET_COOKIE, auth::session_cookie(&token, https))],
        Json(json!({ "ok": true, "token": token })),
    )
        .into_response())
}

async fn logout() -> Response {
    (
        [(
            header::SET_COOKIE,
            format!("{}=; Path=/; Max-Age=0", auth::COOKIE),
        )],
        Json(json!({ "ok": true })),
    )
        .into_response()
}

// ---------- app API ----------

async fn config_route(State(s): S) -> Json<Value> {
    Json(
        json!({ "botsRoot": s.cfg.bots_root, "accessLogout": false, "executorUrl": null, "appUrl": s.cfg.app_url }),
    )
}

async fn get_settings(State(s): S) -> Json<Value> {
    Json(s.settings().await)
}

async fn put_settings(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let patch = parse(&body)?;
    if !patch.is_object() {
        return Err(ApiError::bad_request("settings must be an object"));
    }
    let current = s.settings().await;
    let mut next = store::merge(&current, &patch);
    if let Some(beta) = patch.get("beta") {
        let beta = beta
            .as_object()
            .ok_or_else(|| ApiError::bad_request("beta must be an object"))?;
        let mut merged = current["beta"].clone();
        for (k, v) in beta {
            if k != "projects" {
                return Err(ApiError::bad_request(format!("Unknown beta feature: {k}")));
            }
            merged[k] = json!(v
                .as_bool()
                .ok_or_else(|| ApiError::bad_request(format!("beta.{k} must be true or false")))?);
        }
        next["beta"] = merged;
    }
    store::write_json(&s.settings_path(), &next).await?;
    Ok(Json(next))
}

async fn list_bots(State(s): S) -> ApiResult<Response> {
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "bots": s.bots.list().await? })),
    )
        .into_response())
}

async fn create_bot(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let bot = s.bots.create(&parse(&body)?).await?;
    s.provision_now.notify_one();
    Ok(Json(bot))
}

async fn get_bot(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    s.bots
        .get(&slug)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("not found"))
}

async fn save_bot(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(s.bots.save(&slug, &parse(&body)?, false).await?))
}

async fn delete_bot(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({ "ok": true, "bots": s.bots.remove(&slug).await? }),
    ))
}

async fn main_bot(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let slug = parse(&body)?["slug"].as_str().unwrap_or("").to_string();
    Ok(Json(json!({ "bots": s.bots.select(&slug).await? })))
}

async fn main_thread(s: &AppState, slug: &str) -> ApiResult<String> {
    let bot = s
        .bots
        .get(slug)
        .await?
        .ok_or_else(|| ApiError::not_found("Bot not found"))?;
    bot["mainThreadId"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| ApiError::new(409, "This Bot has no conversation yet"))
}

async fn read_chat(State(s): S, Path(slug): Path<String>) -> ApiResult<Response> {
    let thread = main_thread(&s, &slug).await?;
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    let projection = rpc
        .call(
            "orchestration.getThreadProjection",
            json!({ "threadId": thread }),
        )
        .await
        .map_err(|e| ApiError::new(502, e.to_string()))?;
    let mut out = chat::transcript(&projection);
    out["threadId"] = json!(thread);
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(out)).into_response())
}

async fn send_chat(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let text = parse(&body)?["text"]
        .as_str()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if text.is_empty() {
        return Err(ApiError::bad_request("Write a message first."));
    }
    let thread = main_thread(&s, &slug).await?;
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    let command = chat::send_command(&thread, &text);
    rpc.call("orchestration.dispatchCommand", command.clone())
        .await
        .map_err(|e| ApiError::new(502, e.to_string()))?;
    Ok(Json(
        json!({ "ok": true, "messageId": command["messageId"] }),
    ))
}

async fn stop_chat(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let thread = main_thread(&s, &slug).await?;
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    let projection = rpc
        .call(
            "orchestration.getThreadProjection",
            json!({ "threadId": thread }),
        )
        .await
        .map_err(|e| ApiError::new(502, e.to_string()))?;
    let Some(run) = chat::transcript(&projection)["activeRunId"]
        .as_str()
        .map(String::from)
    else {
        return Ok(Json(json!({ "ok": true, "stopped": false })));
    };
    rpc.dispatch(json!({ "type": "run.interrupt", "threadId": thread, "runId": run }))
        .await
        .map_err(|e| ApiError::new(502, e.to_string()))?;
    Ok(Json(json!({ "ok": true, "stopped": true })))
}

async fn vault_list(State(s): S) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "items": s.vault.list().await? })))
}

async fn vault_get(
    State(s): S,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        s.vault
            .get(&id, q.get("reveal").is_some_and(|v| v == "1"))
            .await?,
    ))
}

async fn vault_create(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(s.vault.create(&parse(&body)?).await?))
}

async fn vault_update(
    State(s): S,
    Path(id): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(s.vault.update(&id, &parse(&body)?).await?))
}

async fn vault_delete(State(s): S, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    s.vault.delete(&id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn projects_on(s: &AppState) -> ApiResult<()> {
    if s.settings().await["beta"]["projects"] == true {
        Ok(())
    } else {
        Err(ApiError::not_found("not found"))
    }
}

async fn projects_board(State(s): S) -> ApiResult<Json<Value>> {
    projects_on(&s).await?;
    Ok(Json(s.projects.board().await))
}

async fn projects_save(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    projects_on(&s).await?;
    require_json(&h)?;
    let items = parse(&body)?["items"]
        .as_array()
        .cloned()
        .ok_or_else(|| ApiError::bad_request("items must be a list"))?;
    Ok(Json(s.projects.save(&items, projects::actor(None)).await?))
}

async fn projects_delete(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    projects_on(&s).await?;
    require_json(&h)?;
    let ids: Vec<String> = parse(&body)?["ids"]
        .as_array()
        .map(|l| {
            l.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(Json(s.projects.delete(&ids, projects::actor(None)).await?))
}

async fn projects_events(
    State(s): S,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    projects_on(&s).await?;
    let hello = Event::default()
        .event("hello")
        .data(json!({ "version": s.projects.version().await }).to_string());
    let changes = BroadcastStream::new(s.projects.events.subscribe()).filter_map(|e| async move {
        e.ok()
            .map(|v| Ok(Event::default().event("change").data(v.to_string())))
    });
    Ok(Sse::new(stream::once(async { Ok(hello) }).chain(changes)).keep_alive(KeepAlive::default()))
}

// ---------- T3 passthrough ----------

async fn t3_route(
    State(s): S,
    ws: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>,
    method: Method,
    uri: Uri,
    h: HeaderMap,
    body: Bytes,
) -> Response {
    let rest = uri.path().trim_start_matches("/t3");
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    if let Ok(ws) = ws {
        let target = match rest {
            "/ws" => {
                let mut u = format!(
                    "{}/ws?orchestrationProtocol=2&clientSurface=web&clientAppVersion=bots-rs",
                    t3::ws_base(&s.t3.base_url)
                );
                if let Some(q) = uri.query() {
                    u.push('&');
                    u.push_str(q);
                }
                u
            }
            "/api/preview-stream/ws" => format!("{}{rest}{query}", t3::ws_base(&s.t3.base_url)),
            _ => return ApiError::not_found("not found").into_response(),
        };
        let token = s.t3.token.clone();
        return ws.on_upgrade(move |socket| t3::proxy_ws(socket, target, token));
    }
    if !rest.starts_with("/api/") {
        return ApiError::not_found("not found").into_response();
    }
    t3::proxy_http(&s.t3, method, &format!("{rest}{query}"), &h, body).await
}

// ---------- the Bots' tool server ----------

async fn mcp_route(
    State(s): S,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
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
    if !bearer(&h).is_some_and(|b| auth::safe_equal(b, &s.mcp_token)) {
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
    for m in msg.as_array().cloned().unwrap_or_else(|| vec![msg]) {
        if let Some(r) = mcp::handle(&s, &m).await {
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

// ---------- the Exact web build ----------

async fn static_files(State(s): S, uri: Uri, req: Request) -> Response {
    use tower::ServiceExt;
    let dir = s.cfg.static_dir.clone();
    let path = uri.path().trim_start_matches('/');
    let candidate = dir.join(path);
    // Unknown paths get the app shell, so links like /?item=… and deep routes load the app.
    let serve_index = path.is_empty() || path.contains("..") || !candidate.is_file();
    let target = if serve_index {
        dir.join("index.html")
    } else {
        candidate
    };
    if !target.is_file() {
        return (StatusCode::NOT_FOUND, "The Bots web app isn't built. Run `bun exact.mjs web --build` in app/, or set STATIC_DIR.").into_response();
    }
    // The web build is made with a placeholder gateway address (see app/app.ts); serve it with the
    // address this request came in on, so one build works wherever the gateway is reached.
    if target.extension().is_some_and(|e| e == "js") {
        if let Ok(js) = tokio::fs::read_to_string(&target).await {
            if js.contains(ORIGIN_PLACEHOLDER) {
                let body = js.replace(ORIGIN_PLACEHOLDER, &request_origin(req.headers()));
                return (
                    [
                        (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
                        (header::CACHE_CONTROL, "no-cache"),
                    ],
                    body,
                )
                    .into_response();
            }
        }
    }
    match tower_http::services::ServeFile::new(target)
        .oneshot(req)
        .await
    {
        Ok(res) => res.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// What the built web app uses for the gateway's address until it is served (see `app/app.ts`).
const ORIGIN_PLACEHOLDER: &str = "http://bots-origin.invalid";

/// The address a request was made to, `scheme://host[:port]`, as the browser sees it behind a proxy.
fn request_origin(h: &HeaderMap) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_comes_from_the_request() {
        let mut h = HeaderMap::new();
        h.insert("host", "bots.tailnet.ts.net:8443".parse().unwrap());
        h.insert("x-forwarded-proto", "https".parse().unwrap());
        assert_eq!(request_origin(&h), "https://bots.tailnet.ts.net:8443");
        h.insert("host", "a\"+alert(1)+\"".parse().unwrap());
        assert_eq!(
            request_origin(&h),
            "https://127.0.0.1",
            "a host that could break out of a string is replaced"
        );
    }
}
