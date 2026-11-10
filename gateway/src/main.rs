//! `hbots`: the gateway as one Rust binary.
//!
//! It sits between the Exact app (`app/`) and a harness backend. T3 Code is the backend
//! wired today (`mod t3`); other backends may be added. The gateway keeps Bot folders,
//! settings, the Vault, the Projects board and the Bots' own MCP tool server. Home is
//! `~/.hbots`.
mod access; // T4: Cloudflare Access JWTs.
mod approvals; // T5: answering a pending request.
mod apps; // Connected apps and the executor URL.
mod attachments; // Chat files stored on the Bot.
mod auth;
mod bots;
mod chat;
mod cli;
mod config;
mod error;
mod exposure; // T4: Tailscale listener and the publish commands.
mod icons; // Site icons cached under HBOTS_HOME/icons.
mod mcp;
mod projects;
mod projects_tools; // PROJECTS-HOOK (T3): the Projects MCP tools and the same-origin check.
mod provisioner;
mod release; // T6b: self-update and rollback.
mod remote_access; // T4: the scrypt password in access.json.
mod routines; // T5: T3 scheduled tasks.
mod service; // T6b: install and remove the background service.
mod store;
mod t3;
mod thread_meta; // T5: reactions, voice calls, roster previews.
mod usage; // Local usage counts. Nothing leaves the machine.
mod vault;
mod vault_csv; // T2: CSV import and export (hook)
mod vault_tools; // T1 hook: the Vault tools of POST /mcp/bot

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
    Extension, Json, Router,
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
    // T4 remote access: Cloudflare Access, the Tailscale listener, the tunnel, the password in access.json.
    pub access: Option<access::Access>,
    pub exposure: exposure::Exposure,
    pub tailnet: Arc<exposure::Tailnet>,
    pub remote: Arc<remote_access::RemoteAccess>,
    pub threads: thread_meta::ThreadMeta,
    pub routine_history: routines::RoutineHistory,
    pub system: release::System,
    pub icons: icons::Icons,
    pub usage: usage::Usage,
    pub apps: apps::Apps,
    pub mcp_token: String,
    pub provision_now: tokio::sync::Notify,
}

impl AppState {
    fn settings_path(&self) -> std::path::PathBuf {
        self.cfg.app_dir().join("settings.json")
    }

    /// `settings.json` with hbots defaults filled in.
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::run(&args).await {
        std::process::exit(code);
    }
    let cfg = config::Config::from_env()?;
    let app_dir = cfg.app_dir();
    std::fs::create_dir_all(&app_dir)?;
    // The Bots' tool-server bearer: HBOTS_MCP_TOKEN, or a random one next to the vault key.
    let token_path = cfg
        .vault_key_file
        .parent()
        .unwrap_or(&cfg.data_root)
        .join(".bot-mcp-token");
    let mcp_token = match std::env::var("HBOTS_MCP_TOKEN")
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
    let exposure_cfg =
        exposure::Exposure::from_env(|k| std::env::var(k).ok().filter(|v| !v.is_empty()));
    let tailnet = Arc::new(exposure::Tailnet::new());
    let remote = Arc::new(remote_access::RemoteAccess::new(
        remote_access::RemoteConfig {
            home: cfg.data_root.clone(),
            store_file: app_dir.join("access.json"),
            url_file: app_dir.join("tunnel-url"),
            local_port: cfg.port,
            env_password: cfg.password.clone(),
            env_tailnet: exposure_cfg.tailnet_port > 0,
            env_tailnet_users: exposure_cfg.tailnet_users.clone(),
            tailscale_bin: remote_access::find_tailscale(),
        },
        tailnet.clone(),
    ));
    let access = access::Access::from_env(
        &cfg.access_team,
        &cfg.access_aud,
        &cfg.access_emails.join(","),
    );
    let state = Arc::new(AppState {
        access,
        exposure: exposure_cfg,
        tailnet: tailnet.clone(),
        remote: remote.clone(),
        auth: auth::Auth::load(&app_dir, cfg.password.clone(), cfg.allowed_origins.clone())?,
        t3: t3::T3::new(cfg.t3_url.clone(), cfg.t3_token.clone()),
        bots: bots::Bots::new(
            cfg.bots_root.clone(),
            cfg.bot_mcp_url.clone(),
            mcp_token.clone(),
        ),
        vault: vault::Vault::new(cfg.vault_file.clone(), cfg.vault_key_file.clone()),
        projects: projects::Projects::new(app_dir.join("projects.json")),
        threads: thread_meta::ThreadMeta::new(app_dir.join("thread-meta.json")),
        routine_history: routines::RoutineHistory::new(app_dir.join("routine-history.json")),
        system: release::System::new(cfg.data_root.clone(), app_dir.clone()),
        icons: icons::Icons::new(cfg.data_root.join("icons")),
        usage: usage::Usage::new(app_dir.join("usage.json")),
        apps: apps::Apps::new(app_dir.join("apps.json")),
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
    release::start(state.clone()).await;

    // The tailnet listener from TAILNET_PORT starts first; Settings' Tailscale and tunnel choices
    // come back in the background.
    if state.exposure.tailnet_port > 0 {
        let router = exposure::on_tailnet(app(state.clone()));
        state
            .tailnet
            .open(
                state.exposure.tailnet_port,
                &state.exposure.tailnet_users,
                router,
            )
            .await?;
    }
    let booting = state.clone();
    tokio::spawn(async move {
        let router = exposure::on_tailnet(app(booting.clone()));
        booting.remote.boot(router).await;
    });
    let stopping = state.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        stopping.remote.stop().await;
        std::process::exit(0);
    });

    let addr: SocketAddr = format!("{}:{}", state.cfg.host, state.cfg.port).parse()?;
    tracing::info!(
        "hbots gateway on http://{addr} → T3 {}, bots in {}",
        state.cfg.t3_url,
        state.cfg.bots_root.display()
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        exposure::on_public(app(state.clone()), state.clone())
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

/// Ctrl-C or SIGTERM: the gateway stops its tunnel before it exits.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
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
        .route(
            "/api/bots/{slug}/attachments",
            get(attachments::list)
                .post(attachments::create)
                .layer(axum::extract::DefaultBodyLimit::max(12 << 20)),
        )
        .route(
            "/api/bots/{slug}/attachments/{id}",
            get(attachments::download).delete(attachments::remove),
        )
        .route(
            "/api/bots/{slug}/requests/{id}/respond",
            post(respond_request),
        )
        .route("/api/main-bot", put(main_bot))
        .route("/api/vault", get(vault_list).post(vault_create))
        .route(
            "/api/vault/{id}",
            get(vault_get).put(vault_update).delete(vault_delete),
        )
        // T2 hook: CSV import and export (vault_csv.rs). Import bodies are up to 8 MB, as in hbots.
        .route(
            "/api/vault/import",
            post(vault_csv::import).layer(axum::extract::DefaultBodyLimit::max(8 << 20)),
        )
        .route(
            "/api/vault/import-csv",
            post(vault_csv::import_csv).layer(axum::extract::DefaultBodyLimit::max(8 << 20)),
        )
        .route("/api/vault/export.csv", get(vault_csv::export))
        .route("/api/projects", get(projects_board))
        .route("/api/projects/items", post(projects_save))
        .route("/api/projects/delete", post(projects_delete))
        .route("/api/projects/events", get(projects_events))
        .route("/api/access", get(access_status))
        .route("/api/access/password", put(access_password))
        .route("/api/access/tailscale", post(access_tailscale))
        .route("/api/access/tunnel", post(access_tunnel))
        .route("/api/thread-previews", post(thread_previews))
        .route("/api/threads/{thread}/meta", get(thread_meta_get))
        .route("/api/threads/{thread}/reactions", post(thread_reaction))
        .route("/api/threads/{thread}/voice", post(thread_voice))
        .route("/api/routines", get(routines_list).put(routines_upsert))
        .route("/api/routines/enabled", post(routines_enabled))
        .route("/api/routines/delete", post(routines_delete))
        .route("/api/routines/run", post(routines_run))
        .route("/api/icons/{host}", get(icons::get_icon))
        .route("/api/usage", get(usage::get_usage))
        .route("/api/apps", get(apps::get_apps).put(apps::put_apps))
        .merge(release::routes())
        .route("/t3/{*rest}", any(t3_route))
        .layer(middleware::from_fn_with_state(state.clone(), require_login));
    let routed = Router::new()
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
        .fallback(static_files);
    routed
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

/// Who may use the app on this listener: the signed-in Tailscale user on the tailnet
/// listener; elsewhere a valid Cloudflare Access token, or a session when a password is set; with
/// neither, open only when Cloudflare Access isn't configured either.
async fn authed(s: &AppState, h: &HeaderMap, surface: exposure::Surface) -> bool {
    if surface == exposure::Surface::Tailnet {
        return exposure::tailnet_user_allowed(h, &s.tailnet.users());
    }
    if let Some(access) = &s.access {
        if let Some(token) = access::token_from(h) {
            if access.verify(&token).await.is_some() {
                return true;
            }
        }
    }
    if s.remote.password_required() {
        return auth::cookie(h, auth::COOKIE).is_some_and(|c| s.auth.valid_session(&c))
            || bearer(h).is_some_and(|b| s.auth.valid_session(b));
    }
    // No password is set (BOT_PASSWORD and Settings both empty), so `Auth::authed` is true here.
    s.access.is_none() && s.auth.authed(h)
}

fn surface_of(req: &Request) -> exposure::Surface {
    req.extensions()
        .get::<exposure::Surface>()
        .copied()
        .unwrap_or(exposure::Surface::Public)
}

async fn require_login(State(s): S, req: Request, next: Next) -> Response {
    if !authed(&s, req.headers(), surface_of(&req)).await {
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

async fn session(
    State(s): S,
    surface: Option<Extension<exposure::Surface>>,
    h: HeaderMap,
) -> Json<Value> {
    let surface = surface.map_or(exposure::Surface::Public, |Extension(x)| x);
    let needs_password = s.remote.password_required();
    // Cloudflare's login only helps requests that came through Cloudflare; on this machine or the LAN, use the password.
    let access_login = s.access.is_some() && !(needs_password && !h.contains_key("cf-ray"));
    Json(json!({
        "authenticated": authed(&s, &h, surface).await,
        "passwordRequired": needs_password,
        "accessLogin": access_login,
    }))
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
        [(header::SET_COOKIE, auth::session_cookie(&token, https))],
        Json(json!({ "ok": true, "token": token })),
    )
        .into_response())
}

/// The app as the tailnet listener serves it.
fn tailnet_app(s: &Arc<AppState>) -> Router {
    exposure::on_tailnet(app(s.clone()))
}

fn access_body(body: &Bytes) -> ApiResult<Value> {
    parse(body)
}

async fn access_status(State(s): S) -> Json<Value> {
    let mut out = s.remote.status().await;
    out["cloudflareAccess"] = match &s.access {
        Some(access) => json!({ "emails": access.emails() }),
        None => Value::Null,
    };
    Json(out)
}

async fn access_password(
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

async fn access_tailscale(
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

async fn access_tunnel(
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

async fn thread_meta_get(State(s): S, Path(thread): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(s.threads.get(Some(&thread)).await?))
}

async fn thread_reaction(
    State(s): S,
    Path(thread): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    Ok(Json(
        s.threads
            .toggle_reaction(Some(&thread), body["item"].as_str(), body["emoji"].as_str())
            .await?,
    ))
}

async fn thread_voice(
    State(s): S,
    Path(thread): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let at = body["at"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(routines::stamped);
    Ok(Json(
        s.threads
            .add_voice_call(Some(&thread), body.get("seconds"), &at)
            .await?,
    ))
}

async fn routines_list(State(s): S) -> ApiResult<Json<Value>> {
    let mut listed = routines::list(&s.t3).await?;
    if let Some(obj) = listed.as_object_mut() {
        obj.insert("history".into(), s.routine_history.recent().await);
    }
    Ok(Json(listed))
}

async fn routines_upsert(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(
        routines::change(
            &s.t3,
            &s.routine_history,
            "upsert",
            routines::UPSERT,
            parse(&body)?,
        )
        .await?,
    ))
}

async fn routines_enabled(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(
        routines::change(
            &s.t3,
            &s.routine_history,
            "enabled",
            routines::SET_ENABLED,
            parse(&body)?,
        )
        .await?,
    ))
}

async fn routines_delete(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(
        routines::change(
            &s.t3,
            &s.routine_history,
            "delete",
            routines::DELETE,
            parse(&body)?,
        )
        .await?,
    ))
}

async fn routines_run(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(
        routines::change(
            &s.t3,
            &s.routine_history,
            "run",
            routines::RUN_NOW,
            parse(&body)?,
        )
        .await?,
    ))
}

async fn respond_request(
    State(s): S,
    Path((slug, id)): Path<(String, String)>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let bot = s
        .bots
        .get(&slug)
        .await?
        .ok_or_else(|| ApiError::not_found("no such bot"))?;
    let thread = bot["mainThreadId"]
        .as_str()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| ApiError::bad_request("This Bot has no conversation yet."))?;
    let kind = body["kind"].as_str().unwrap_or("");
    let outbound = approvals::outbound(kind, thread, &id, &body).map_err(ApiError::bad_request)?;
    Ok(Json(approvals::send(&s.t3, outbound).await?))
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

async fn config_route(State(s): S, surface: Option<Extension<exposure::Surface>>) -> Json<Value> {
    let on_tailnet = surface.is_some_and(|Extension(x)| x == exposure::Surface::Tailnet);
    Json(json!({
        "botsRoot": s.cfg.bots_root,
        "accessLogout": s.access.is_some() && !on_tailnet,
        "executorUrl": if s.cfg.executor_url.is_empty() { Value::Null } else { json!(s.cfg.executor_url) },
        "appUrl": s.cfg.app_url,
    }))
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
    let body = parse(&body)?;
    let text = body["text"]
        .as_str()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let files = attachments::for_send(&s.cfg.bots_root, &slug, &body).await?;
    if text.is_empty() && files.is_empty() {
        return Err(ApiError::bad_request("Write a message first."));
    }
    let thread = main_thread(&s, &slug).await?;
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    let command = chat::send_command(&thread, &text, &files);
    rpc.call("orchestration.dispatchCommand", command.clone())
        .await
        .map_err(|e| ApiError::new(502, e.to_string()))?;
    s.usage.note(usage::Event::Message).await;
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

async fn thread_previews(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Response> {
    require_json(&h)?;
    let b = parse(&body)?;
    let names: HashMap<String, String> = s
        .bots
        .list()
        .await?
        .iter()
        .filter_map(|bot| {
            Some((
                bot["mainThreadId"].as_str()?.to_string(),
                bot["name"].as_str()?.to_string(),
            ))
        })
        .collect();
    let rpc = s.t3.rpc().await.ok();
    let previews = s
        .threads
        .previews(b.get("stamps"), &names, |thread| {
            let rpc = rpc.clone();
            async move {
                let rpc = rpc.ok_or_else(|| anyhow::anyhow!("T3 unavailable"))?;
                rpc.call(
                    "orchestration.getThreadProjection",
                    json!({ "threadId": thread }),
                )
                .await
            }
        })
        .await?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "previews": previews })),
    )
        .into_response())
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
    let item = s.vault.create(&parse(&body)?).await?;
    s.usage.note(usage::Event::VaultWrite).await;
    Ok(Json(item))
}

async fn vault_update(
    State(s): S,
    Path(id): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let item = s.vault.update(&id, &parse(&body)?).await?;
    s.usage.note(usage::Event::VaultWrite).await;
    Ok(Json(item))
}

async fn vault_delete(State(s): S, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    s.vault.delete(&id).await?;
    s.usage.note(usage::Event::VaultWrite).await;
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
    // PROJECTS-HOOK (T3): writes must come from the app's own page.
    if !projects_tools::same_origin(&h) {
        return Err(ApiError::new(403, "This change didn't come from the app."));
    }
    require_json(&h)?;
    let items = parse(&body)?["items"]
        .as_array()
        .cloned()
        .ok_or_else(|| ApiError::bad_request("items must be a list"))?;
    Ok(Json(s.projects.save(&items, projects::actor(None)).await?))
}

async fn projects_delete(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    projects_on(&s).await?;
    if !projects_tools::same_origin(&h) {
        return Err(ApiError::new(403, "This change didn't come from the app."));
    }
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
        let close_sockets = s.auth.close_sockets.clone();
        return ws.on_upgrade(move |socket| t3::proxy_ws(socket, target, token, close_sockets));
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
const ORIGIN_PLACEHOLDER: &str = "http://hbots-origin.invalid";

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
