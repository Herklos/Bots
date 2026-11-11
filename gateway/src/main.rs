//! `hbots`: the gateway as one Rust binary.
//!
//! It sits between the Exact app (`app/`) and a harness backend. T3 Code is the backend
//! wired today (`mod t3`); other backends may be added. The gateway keeps Bot folders,
//! settings, the Vault, the Projects board and the Bots' own MCP tool server. Home is
//! `~/.hbots`.
// These copies are pinned by crates this binary does not publish.
// base64 0.22 (this crate, axum, reqwest) and 0.23 (hyper-util).
// cpufeatures 0.2 (aes, sha1, sha2, polyval) and 0.3 (chacha20).
// getrandom 0.2 (ring, and rand 0.8 through this crate and rsa), 0.3 (tungstenite's
// rand 0.9), and 0.4 (quinn, uuid, tempfile). Dropping the direct rand 0.8 still
// leaves 0.2 via ring and rsa, and nothing here can retarget tungstenite or quinn.
// The same upstream split also keeps rand, rand_core, rand_chacha, r-efi,
// tokio-tungstenite 0.26 next to axum's 0.29, tungstenite, webpki-roots,
// windows-sys 0.52 (ring) next to 0.61, and syn.
#![allow(
    clippy::multiple_crate_versions,
    reason = "base64 0.22/0.23, cpufeatures 0.2/0.3, and getrandom 0.2/0.3/0.4 are upstream pins this repo cannot collapse, and the same graph keeps the other duplicate crates"
)]
mod access; // T4: Cloudflare Access JWTs.
mod approvals; // T5: answering a pending request.
mod attachments; // Chat files stored on the Bot.
mod auth;
mod bots;
mod chat;
mod cli;
// Fixed strings for the browser stream. The socket error can name the T3 address,
// and the bearer-header error can echo the token.
#[allow(
    clippy::map_err_ignore,
    reason = "the client sees a fixed browser-stream failure, never the socket error or the bearer token"
)]
// `computer.rs` is intentionally left unchanged. `pub` in that private module is
// still meaningless to other crates; the allow stays until that file is edited.
#[allow(
    unreachable_pub,
    reason = "computer.rs is left unchanged; its pub items are not part of another crate's API"
)]
mod computer; // The Bot's browser, watch first.
mod config;
mod error;
mod exposure; // T4: Tailscale listener and the publish commands.
mod face_icon; // The Bot face written into the workspace for the thread list.
mod icons; // Site icons cached under HBOTS_HOME/icons.
mod library; // Files and records through the workspace adapter.
mod lineage; // Subagents and delegated threads of one Bot.
mod mcp;
mod memory_tools;
mod network; // Where the gateway listens: this device, the local network, or Tailscale.
mod projects;
mod projects_tools; // PROJECTS-HOOK (T3): the Projects MCP tools and the same-origin check.
mod providers; // The providers and models T3 Code offers.
mod provisioner;
mod release; // T6b: self-update and rollback.
mod remote_access; // T4: the scrypt password in access.json.
mod rooms; // Local rooms. A send is stored here, then dispatched to each member's main thread.
mod roster; // What an inbox row needs: preview, working, unread.
mod routines; // T5: T3 scheduled tasks.
mod service; // T6b: install and remove the background service.
mod store;
mod subagent_tools;
mod suggestions; // New Bot starting points.
mod t3;
mod thread_meta; // T5: reactions, voice calls, roster previews.
mod usage; // Local usage counts. Nothing leaves the machine.
mod vault;
mod vault_csv; // T2: CSV import and export (hook)
mod vault_tools;
mod workspace; // Where Bot folders live. Library and connectors use this. // T1 hook: the Vault tools of POST /mcp/bot

use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, Query, Request, State, WebSocketUpgrade},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, delete, get, post, put},
    Extension, Json, Router,
};
use error::{ApiError, ApiResult};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Debug)]
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
    pub roster: roster::Roster,
    pub providers: providers::Providers,
    pub rooms: rooms::Rooms,
    /// Last known working flag per group thread, and the short grace after a dispatch.
    /// Not written to `groups.json`.
    pub(crate) room_working: Mutex<rooms::WorkingBook>,
    pub routine_history: routines::RoutineHistory,
    pub system: release::System,
    pub icons: icons::Icons,
    pub usage: usage::Usage,
    pub library: library::Library,
    pub mcp_token: String,
    pub provision_now: tokio::sync::Notify,
}

impl AppState {
    fn settings_path(&self) -> std::path::PathBuf {
        self.cfg.app_dir().join("settings.json")
    }

    pub fn settings_defaults() -> Value {
        json!({
            "userName": "", "theme": "dark", "palette": "amoled", "accent": "bot", "timezone": "", "autoReview": true,
            "rules": { "alwaysAsk": [], "allowWithoutAsking": [] }, "sections": [], "defaultModel": null, "avatarDrawingModel": null,
            "beta": { "projects": false },
            "notifications": { "enabled": false, "sound": true, "preview": "full", "when": "any" },
            "groupAnswerPolicy": "last",
        })
    }

    /// Known notification fields from the file. Missing or invalid values stay at the defaults.
    /// Unknown keys are dropped, the same way `beta` keeps only `projects`.
    fn notifications_view(saved: &Value) -> Value {
        let mut out = Self::settings_defaults()["notifications"].clone();
        let Some(given) = saved.get("notifications").and_then(|v| v.as_object()) else {
            return out;
        };
        for (k, v) in given {
            match k.as_str() {
                "enabled" | "sound" => {
                    if let Some(on) = v.as_bool() {
                        out[k] = json!(on);
                    }
                }
                "preview" => {
                    if matches!(v.as_str(), Some("full" | "hidden")) {
                        out[k] = v.clone();
                    }
                }
                "when" => {
                    if matches!(v.as_str(), Some("any" | "background")) {
                        out[k] = v.clone();
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// `settings.json` with hbots defaults filled in.
    pub async fn settings(&self) -> Value {
        let saved = store::read_json(&self.settings_path())
            .await
            .unwrap_or_else(|| json!({}));
        let mut out = store::merge(&Self::settings_defaults(), &saved);
        out["beta"] = json!({ "projects": saved["beta"]["projects"] == true });
        out["notifications"] = Self::notifications_view(&saved);
        out["groupAnswerPolicy"] = json!(group_answer_policy(&out).as_str());
        out
    }
}

fn group_answer_policy(settings: &Value) -> rooms::AnswerPolicy {
    rooms::AnswerPolicy::parse(settings["groupAnswerPolicy"].as_str())
        .unwrap_or(rooms::AnswerPolicy::Last)
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
    // Snapshot HOST before dotenv. `hbots up` writes HOST=127.0.0.1 into ~/.hbots/.env, and that
    // default must not lock Settings. A HOST already in the process environment is intentional.
    let process_host = std::env::var("HOST").ok().filter(|v| !v.is_empty());
    let process_network = std::env::var("HBOTS_NETWORK_ACCESS")
        .ok()
        .filter(|v| !v.is_empty());
    let cfg = config::Config::from_env()?;
    let network_env = process_network.or_else(|| {
        std::env::var("HBOTS_NETWORK_ACCESS")
            .ok()
            .filter(|v| !v.is_empty())
    });
    let merged_host = std::env::var("HOST").ok().filter(|v| !v.is_empty());
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
            network_env,
            process_host,
            merged_host,
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
        roster: roster::Roster::default(),
        providers: providers::Providers::default(),
        rooms: rooms::Rooms::new(app_dir.join("groups.json")),
        room_working: Mutex::new(rooms::WorkingBook::default()),
        routine_history: routines::RoutineHistory::new(app_dir.join("routine-history.json")),
        system: release::System::new(cfg.data_root.clone(), app_dir.clone()),
        icons: icons::Icons::new(cfg.data_root.join("icons")),
        usage: usage::Usage::new(app_dir.join("usage.json")),
        library: library::Library::new(
            Arc::new(workspace::LocalDisk::new(cfg.bots_root.clone())),
            cfg.bot_mcp_url.clone(),
            mcp_token.clone(),
        ),
        mcp_token,
        provision_now: tokio::sync::Notify::new(),
        cfg,
    });
    // Write the defaults once; an existing file, even an unreadable one, is left alone.
    if !state.settings_path().exists() {
        if let Err(e) =
            store::write_json(&state.settings_path(), &AppState::settings_defaults()).await
        {
            tracing::warn!("writing settings.json: {e}");
        }
    }
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

    let applied = network::apply(
        &state.remote.network_choice(),
        state.remote.password_required(),
        config::dev_allow_any_origin(),
    );
    let (bound, bind_error, listener) = match bind_gateway(&applied.bind, state.cfg.port).await {
        Ok(listener) => (applied.bind.clone(), applied.error.clone(), listener),
        Err(err) if applied.bind != "127.0.0.1" => {
            let message = format!(
                "Couldn't listen on {}:{} ({err}). hbots is listening on this device only.",
                applied.bind, state.cfg.port
            );
            tracing::error!("{message}");
            let listener = bind_gateway("127.0.0.1", state.cfg.port).await?;
            ("127.0.0.1".into(), Some(message), listener)
        }
        Err(err) => return Err(err),
    };
    state
        .remote
        .note_listener(&bound, bind_error.clone(), applied.withheld);
    if applied.withheld {
        if let Some(err) = &applied.error {
            tracing::error!("network access: {err}");
        }
    }

    // The tailnet listener from HBOTS_TAILNET_PORT starts first; Settings' Tailscale and tunnel choices
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

    let addr: SocketAddr = format!("{bound}:{}", state.cfg.port).parse()?;
    tracing::info!(
        "hbots gateway on http://{addr} → T3 {}, bots in {}",
        state.cfg.t3_url,
        state.cfg.bots_root.display()
    );
    if !state.cfg.static_dir.join("index.html").is_file() {
        tracing::warn!(
            "{WEB_APP_MISSING} (looked in {})",
            state.cfg.static_dir.display()
        );
    }
    axum::serve(
        listener,
        exposure::on_public(app(state.clone()), state.clone())
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn bind_gateway(host: &str, port: u16) -> anyhow::Result<tokio::net::TcpListener> {
    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    Ok(tokio::net::TcpListener::bind(addr).await?)
}

/// The running process is the installed background service, so Settings can restart it.
fn service_can_restart(home: &std::path::Path) -> bool {
    if std::env::var("HBOTS_SERVICE").ok().as_deref() != Some("1") {
        return false;
    }
    let os = std::env::consts::OS;
    service::supported(os) && service::installed(&cli::service_spec(home), os)
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
        .route("/api/bots/{slug}/seen", post(mark_seen))
        .route("/api/bots/{slug}/chat", get(read_chat).post(send_chat))
        .route("/api/bots/{slug}/voice", post(send_voice))
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
        .route(
            "/api/bots/{slug}/library",
            get(library_get).post(library_save),
        )
        .route(
            "/api/bots/{slug}/library/items/{id}",
            delete(library_delete),
        )
        .route("/api/bots/{slug}/library/file", get(library_file))
        .route("/api/bots/{slug}/shelf", get(shelf_get))
        .route(
            "/api/bots/{slug}/connectors",
            post(connector_save).delete(connector_delete),
        )
        .route("/api/providers", get(providers_get))
        .route("/api/suggestions", get(suggestions_get))
        .route("/api/bots/{slug}/model", get(model_get).put(model_put))
        .route("/api/bots/{slug}/usage", get(bot_usage))
        .route(
            "/api/bots/{slug}/routines",
            get(bot_routines).post(bot_routine_save),
        )
        .route(
            "/api/bots/{slug}/routines/enabled",
            post(bot_routine_enabled),
        )
        .route("/api/bots/{slug}/routines/delete", post(bot_routine_delete))
        .route("/api/bots/{slug}/routines/run", post(bot_routine_run))
        .route("/api/bots/{slug}/lineage", get(lineage_get))
        .route("/api/bots/{slug}/computer", get(computer_get))
        .route("/api/bots/{slug}/computer/open", post(computer_open))
        .route("/api/bots/{slug}/computer/close", post(computer_close))
        .route("/api/bots/{slug}/computer/control", post(computer_control))
        .route("/api/bots/{slug}/computer/input", post(computer_input))
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
        .route("/api/access", get(access_status))
        .route("/api/access/password", put(access_password))
        .route("/api/access/network", put(access_network))
        .route("/api/access/network/restart", post(access_network_restart))
        .route("/api/access/tailscale", post(access_tailscale))
        .route("/api/access/tunnel", post(access_tunnel))
        .route("/api/thread-previews", post(thread_previews))
        .route("/api/rooms", get(rooms_list).post(rooms_create))
        .route(
            "/api/rooms/{id}",
            get(room_get).put(room_update).delete(room_delete),
        )
        .route("/api/rooms/{id}/messages", post(room_send))
        .route("/api/rooms/{id}/seen", post(room_seen))
        .route("/api/threads/{thread}/meta", get(thread_meta_get))
        .route("/api/threads/{thread}/reactions", post(thread_reaction))
        .route("/api/threads/{thread}/pins", post(thread_pin))
        .route("/api/threads/{thread}/reply", post(thread_reply))
        .route("/api/threads/{thread}/read", post(thread_read))
        .route("/api/threads/{thread}/voice", post(thread_voice))
        .route("/api/routines", get(routines_list).put(routines_upsert))
        .route("/api/routines/enabled", post(routines_enabled))
        .route("/api/routines/delete", post(routines_delete))
        .route("/api/routines/run", post(routines_run))
        .route("/api/icons/{host}", get(icons::get_icon))
        .route("/api/usage", get(usage::get_usage))
        .route("/api/usage/report", get(usage_report))
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
    let cors = origin.clone().filter(|o| {
        config::dev_allow_any_origin() || loopback_origin(o) || s.cfg.allowed_origins.contains(o)
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
    #[allow(
        clippy::map_err_ignore,
        reason = "serde quotes the body, which may hold a password"
    )]
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

async fn access_view(s: &AppState) -> Value {
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

async fn access_status(State(s): S) -> Json<Value> {
    Json(access_view(&s).await)
}

async fn access_network(
    State(s): S,
    body: Bytes,
) -> Result<Json<Value>, remote_access::AccessError> {
    let body =
        access_body(&body).map_err(|e| remote_access::AccessError::bad_request(e.message))?;
    let mode = body["mode"].as_str().unwrap_or("");
    s.remote.set_network(mode, tailnet_app(&s)).await?;
    Ok(Json(access_view(&s).await))
}

async fn access_network_restart(State(s): S) -> Result<Json<Value>, remote_access::AccessError> {
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

async fn rooms_list(State(s): S) -> ApiResult<Json<Value>> {
    let listed = s.rooms.list().await?;
    let bots = s.bots.list().await?;
    let mut prepared = Vec::with_capacity(listed.len());
    for room in &listed {
        prepared.push(room_members(&s, room, &bots, false).await?);
    }
    let threads = room_thread_ids(prepared.iter().flatten());
    let reads = read_room_threads(&s, threads).await;
    let now = Instant::now();
    let mut book = s
        .room_working
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rooms: Vec<Value> = listed
        .iter()
        .zip(&prepared)
        .map(|(room, members)| {
            let working = working_slugs(members, &reads, &mut book, now);
            rooms::summary(room, &working)
        })
        .collect();
    Ok(Json(json!({ "rooms": rooms })))
}

fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Refuses a slug that is not a Bot on this machine. The list is the one `Bots::list` loaded.
async fn known_bots(s: &AppState, slugs: &[String]) -> ApiResult<Vec<Value>> {
    let known = s.bots.list().await?;
    if slugs.iter().any(|slug| {
        !known
            .iter()
            .any(|bot| bot["slug"].as_str() == Some(slug.as_str()))
    }) {
        return Err(ApiError::bad_request("Pick Bots that already exist."));
    }
    Ok(known)
}

async fn rooms_create(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let slugs = string_list(&body["members"]);
    let known = known_bots(&s, &slugs).await?;
    let room = s
        .rooms
        .create(
            body["title"].as_str().unwrap_or(""),
            body["description"].as_str().unwrap_or(""),
            body["color"].as_str().unwrap_or(""),
            body["shape"].as_str().unwrap_or(""),
            &slugs,
            &known,
            body["answerPolicy"].as_str(),
            body["chief"].as_str(),
        )
        .await?;
    let policy = group_answer_policy(&s.settings().await);
    let (members, _) = room_snapshot(&s, &room, true).await?;
    Ok(Json(rooms::present(&room, &members, policy)))
}

async fn room_get(State(s): S, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let room = s.rooms.get(&id).await?;
    let (members, threads) = room_snapshot(&s, &room, false).await?;
    // Replies the members have finished since the last read join the room here, and a reply that
    // mentions another member wakes that member.
    let policy = group_answer_policy(&s.settings().await);
    let (room, wake) = s.rooms.sync(&id, &members, &threads, policy).await?;
    if let Err(err) = dispatch_room(&s, &wake).await {
        tracing::warn!("group wake-up failed: {}", err.message);
    }
    Ok(Json(rooms::present(&room, &members, policy)))
}

async fn room_update(
    State(s): S,
    Path(id): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let known = if body.get("members").is_some() {
        known_bots(&s, &string_list(&body["members"])).await?
    } else {
        Vec::new()
    };
    let room = s.rooms.update(&id, &body, &known).await?;
    let policy = group_answer_policy(&s.settings().await);
    let (members, _) = room_snapshot(&s, &room, false).await?;
    Ok(Json(rooms::present(&room, &members, policy)))
}

async fn room_seen(State(s): S, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let room = s.rooms.seen(&id).await?;
    let policy = group_answer_policy(&s.settings().await);
    let (members, _) = room_snapshot(&s, &room, false).await?;
    Ok(Json(rooms::present(&room, &members, policy)))
}

async fn room_delete(State(s): S, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    s.rooms.delete(&id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn room_send(
    State(s): S,
    Path(id): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let text = body["text"].as_str().unwrap_or("").trim().to_string();
    let room = s.rooms.get(&id).await?;
    let (members, _) = room_snapshot(&s, &room, true).await?;
    let (stored, dispatches) = s
        .rooms
        .send(
            &id,
            &members,
            &text,
            &uuid::Uuid::new_v4().to_string(),
            &routines::stamped(),
            group_answer_policy(&s.settings().await),
        )
        .await?;
    let dispatched = dispatch_room(&s, &dispatches).await?;
    let policy = group_answer_policy(&s.settings().await);
    Ok(Json(json!({
        "ok": true,
        "room": rooms::present(&stored, &members, policy),
        "dispatched": dispatched,
    })))
}

/// Sends each dispatch to its member's main thread and counts it as that Bot's message.
async fn dispatch_room(s: &AppState, dispatches: &[rooms::Dispatch]) -> ApiResult<Vec<String>> {
    let mut sent = Vec::new();
    if dispatches.is_empty() {
        return Ok(sent);
    }
    let Ok(rpc) = s.t3.rpc().await else {
        return Ok(sent);
    };
    for (item, command) in dispatches.iter().zip(rooms::dispatch_commands(dispatches)) {
        rpc.call("orchestration.dispatchCommand", command)
            .await
            .map_err(|err| ApiError::new(502, err.to_string()))?;
        s.usage.note_for(&item.slug, usage::Event::Message).await;
        {
            let mut book = s
                .room_working
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            book.note(&item.thread_id, Instant::now());
        }
        sent.push(item.slug.clone());
    }
    Ok(sent)
}

fn room_thread_ids<'a>(members: impl Iterator<Item = &'a rooms::Member>) -> Vec<String> {
    members
        .filter(|member| !member.thread_id.is_empty())
        .map(|member| member.thread_id.clone())
        .collect()
}

fn working_slugs(
    members: &[rooms::Member],
    reads: &HashMap<String, (rooms::MemberProbe, Vec<Value>)>,
    book: &mut rooms::WorkingBook,
    now: Instant,
) -> Vec<String> {
    members
        .iter()
        .filter(|member| {
            let probe = reads
                .get(&member.thread_id)
                .map(|(probe, _)| *probe)
                .unwrap_or(rooms::MemberProbe::Failed);
            !member.thread_id.is_empty() && book.decide(&member.thread_id, probe, now)
        })
        .map(|member| member.slug.clone())
        .collect()
}

async fn read_room_threads(
    s: &AppState,
    thread_ids: Vec<String>,
) -> HashMap<String, (rooms::MemberProbe, Vec<Value>)> {
    let rpc = s.t3.rpc().await.ok();
    rooms::read_group_threads(thread_ids, |thread| {
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
    .await
}

/// Member rows for a room, with each member's working flag, and each member's group thread messages.
/// With `ensure`, a member who is set up but has no thread for this group gets one now. An empty
/// id or the Bot's one-to-one thread counts as none: the read path drops it from the member and
/// passes no messages, and the next ensure launches. A failed launch stays empty. A group talks
/// in its own threads, so nothing it says lands in a Bot's one-to-one conversation.
async fn room_snapshot(
    s: &AppState,
    room: &Value,
    ensure: bool,
) -> ApiResult<(
    Vec<rooms::Member>,
    std::collections::HashMap<String, Vec<Value>>,
)> {
    let bots = s.bots.list().await?;
    let mut members = room_members(s, room, &bots, ensure).await?;
    let reads = read_room_threads(s, room_thread_ids(members.iter())).await;
    let now = Instant::now();
    let mut book = s
        .room_working
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut threads = std::collections::HashMap::new();
    for member in &mut members {
        if member.thread_id.is_empty() {
            member.working = false;
            continue;
        }
        if let Some((rooms::MemberProbe::Read { .. }, messages)) = reads.get(&member.thread_id) {
            // This thread is the group, so the one-to-one filter would drop the anchor and the reply.
            threads.insert(member.slug.clone(), messages.clone());
        }
        let probe = reads
            .get(&member.thread_id)
            .map(|(probe, _)| *probe)
            .unwrap_or(rooms::MemberProbe::Failed);
        member.working = book.decide(&member.thread_id, probe, now);
    }
    Ok((members, threads))
}

/// Seats for a room, before their projections are read. With `ensure`, a member who is set up
/// but has no thread for this group gets one now. An empty id or the Bot's one-to-one thread
/// counts as none. A failed launch stays empty.
async fn room_members(
    s: &AppState,
    room: &Value,
    bots: &[Value],
    ensure: bool,
) -> ApiResult<Vec<rooms::Member>> {
    let rpc = s.t3.rpc().await.ok();
    let room_id = room["id"].as_str().unwrap_or("").to_string();
    let settings = if ensure {
        s.settings().await
    } else {
        Value::Null
    };
    let mut members = Vec::new();
    for slug in string_list(&room["members"]) {
        let Some(bot) = bots
            .iter()
            .find(|bot| bot["slug"].as_str() == Some(slug.as_str()))
        else {
            continue;
        };
        let main = bot["mainThreadId"].as_str().unwrap_or("");
        let provisioned = !main.is_empty();
        let mut thread = room["threads"][&slug].as_str().unwrap_or("").to_string();
        if rooms::Rooms::group_thread_missing(&thread, main) {
            thread.clear();
        }
        if thread.is_empty() && ensure && provisioned {
            if let (Some(rpc), Some(project)) = (&rpc, bot["projectId"].as_str()) {
                let model = [&bot["model"], &settings["defaultModel"]]
                    .into_iter()
                    .find(|m| m.is_object())
                    .cloned()
                    .unwrap_or_else(provisioner::default_model);
                let id = uuid::Uuid::new_v4().to_string();
                let launched = rpc
                    .call(
                        "orchestration.launchThread",
                        json!({
                            "commandId": uuid::Uuid::new_v4().to_string(), "threadId": id, "projectId": project,
                            "title": room["title"], "modelSelection": model,
                            "runtimeMode": provisioner::launch_runtime_mode(bot, &settings),
                            "interactionMode": "default", "workspaceStrategy": { "type": "root" },
                        }),
                    )
                    .await;
                match launched {
                    Ok(_) => {
                        s.rooms.set_thread(&room_id, &slug, &id).await?;
                        thread = id;
                    }
                    Err(err) => tracing::warn!("group thread for {slug} failed: {err}"),
                }
            }
        }
        members.push(rooms::Member {
            slug,
            title: bot["name"].as_str().unwrap_or("Bot").to_string(),
            thread_id: thread,
            working: false,
            provisioned,
        });
    }
    Ok(members)
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

async fn thread_pin(
    State(s): S,
    Path(thread): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    Ok(Json(
        s.threads
            .toggle_pin(Some(&thread), body["item"].as_str())
            .await?,
    ))
}

async fn thread_reply(
    State(s): S,
    Path(thread): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    Ok(Json(
        s.threads
            .set_reply(
                Some(&thread),
                body["messageId"].as_str(),
                body["replyTo"].as_str(),
                body["text"].as_str(),
            )
            .await?,
    ))
}

async fn thread_read(
    State(s): S,
    Path(thread): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    Ok(Json(
        s.threads
            .mark_read(
                Some(&thread),
                body["latest"].as_str(),
                body["open"].as_bool().unwrap_or(false),
                body["hidden"].as_bool().unwrap_or(false),
            )
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
    let thread = resolve_chat_thread(&s, &slug, body["child"].as_str()).await?;
    let kind = body["kind"].as_str().unwrap_or("");
    let outbound = approvals::outbound(kind, &thread, &id, &body).map_err(ApiError::bad_request)?;
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
        "appUrl": s.cfg.app_url,
        "version": env!("CARGO_PKG_VERSION"),
        "harness": { "name": "T3 Code", "connected": s.t3.rpc().await.is_ok() },
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
    let mut patch = patch;
    // The default model is checked like a Bot's, and stored as T3 stores a selection.
    if patch.get("defaultModel").is_some_and(|v| v.is_object()) {
        let chosen =
            checked_selection(&s, &patch["defaultModel"], &current["defaultModel"]).await?;
        patch["defaultModel"] = chosen;
    }
    if let Some(policy) = patch.get("groupAnswerPolicy") {
        if rooms::AnswerPolicy::parse(policy.as_str()).is_none() {
            return Err(ApiError::bad_request(
                "groupAnswerPolicy must be last, everyone, or chief",
            ));
        }
    }
    if let Some(mode) = patch.get("defaultRuntimeMode") {
        if !matches!(
            mode.as_str(),
            Some("approval-required" | "auto" | "full-access")
        ) {
            return Err(ApiError::bad_request(
                "defaultRuntimeMode must be approval-required, auto, or full-access",
            ));
        }
    }
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
    if let Some(notifications) = patch.get("notifications") {
        let notifications = notifications
            .as_object()
            .ok_or_else(|| ApiError::bad_request("notifications must be an object"))?;
        let mut merged = current["notifications"].clone();
        for (k, v) in notifications {
            match k.as_str() {
                "enabled" | "sound" => {
                    merged[k] = json!(v.as_bool().ok_or_else(|| {
                        ApiError::bad_request(format!("notifications.{k} must be true or false"))
                    })?);
                }
                "preview" => {
                    let preview = v
                        .as_str()
                        .filter(|p| matches!(*p, "full" | "hidden"))
                        .ok_or_else(|| {
                            ApiError::bad_request("notifications.preview must be full or hidden")
                        })?;
                    merged[k] = json!(preview);
                }
                "when" => {
                    let when = v
                        .as_str()
                        .filter(|w| matches!(*w, "any" | "background"))
                        .ok_or_else(|| {
                            ApiError::bad_request("notifications.when must be any or background")
                        })?;
                    merged[k] = json!(when);
                }
                _ => {
                    return Err(ApiError::bad_request(format!(
                        "Unknown notifications setting: {k}"
                    )));
                }
            }
        }
        next["notifications"] = merged;
    }
    store::write_json(&s.settings_path(), &next).await?;
    Ok(Json(next))
}

async fn list_bots(State(s): S) -> ApiResult<Response> {
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "bots": roster::list(&s, s.bots.list().await?).await, "now": unix_now() * 1000 })),
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

/// The person has this Bot's chat open: everything up to now is read.
async fn mark_seen(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    s.bots
        .save(&slug, &json!({ "lastSeenAt": vault::now_iso() }), false)
        .await?;
    Ok(Json(json!({ "ok": true })))
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

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn require_bot(s: &AppState, slug: &str) -> ApiResult<Value> {
    s.bots
        .get(slug)
        .await?
        .ok_or_else(|| ApiError::not_found("Bot not found"))
}

async fn library_get(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    require_bot(&s, &slug).await?;
    let messages = library_messages(&s, &slug).await;
    Ok(Json(s.library.view(&slug, &messages, unix_now()).await?))
}

/// What the panel's Library shows: pictures, files and links, read only.
async fn shelf_get(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    require_bot(&s, &slug).await?;
    let messages = library_messages(&s, &slug).await;
    let view = s.library.view(&slug, &messages, unix_now()).await?;
    Ok(Json(library::shelf_from(&slug, &view)))
}

async fn library_messages(s: &AppState, slug: &str) -> Vec<Value> {
    let Ok(thread) = main_thread(s, slug).await else {
        return Vec::new();
    };
    let Ok(rpc) = s.t3.rpc().await else {
        return Vec::new();
    };
    let Ok(projection) = rpc
        .call(
            "orchestration.getThreadProjection",
            json!({ "threadId": thread }),
        )
        .await
    else {
        return Vec::new();
    };
    chat::transcript(&projection)["messages"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

async fn library_save(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    require_bot(&s, &slug).await?;
    Ok(Json(
        s.library
            .save_item(&slug, &parse(&body)?, unix_now())
            .await?,
    ))
}

async fn library_delete(
    State(s): S,
    Path((slug, id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    require_bot(&s, &slug).await?;
    Ok(Json(s.library.delete_item(&slug, &id).await?))
}

/// Content type for a library file. The extension match ignores case: the default macOS volume does too.
fn library_mime(path: &str) -> &'static str {
    let ext = std::path::Path::new(path).extension();
    let is = |want: &str| ext.is_some_and(|found| found.eq_ignore_ascii_case(want));
    if is("html") || is("htm") {
        "text/html; charset=utf-8"
    } else if is("png") {
        "image/png"
    } else if is("jpg") || is("jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}

async fn library_file(
    State(s): S,
    Path(slug): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    require_bot(&s, &slug).await?;
    let path = q.get("path").map(String::as_str).unwrap_or("");
    let bytes = s.library.file_bytes(&slug, path).await?;
    let mime = library_mime(path);
    Ok(([(header::CONTENT_TYPE, mime)], bytes).into_response())
}

async fn connector_save(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    require_bot(&s, &slug).await?;
    Ok(Json(s.library.save_connector(&slug, &parse(&body)?).await?))
}

async fn connector_delete(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    require_bot(&s, &slug).await?;
    let name = parse(&body)?["name"].as_str().unwrap_or("").to_string();
    Ok(Json(s.library.delete_connector(&slug, &name).await?))
}

/// Starting points for New Bot.
async fn suggestions_get() -> Json<Value> {
    Json(json!({ "suggestions": suggestions::all() }))
}

/// Every provider T3 Code has ready, with its models and effort levels.
async fn providers_get(State(s): S) -> ApiResult<Json<Value>> {
    let list = s
        .providers
        .list(&s.t3)
        .await
        .map_err(|e| ApiError::new(502, format!("T3 didn't list providers: {e}")))?;
    Ok(Json(json!({ "ok": true, "providers": list["providers"] })))
}

/// A model choice from a request body, checked against T3's providers. When T3 can't be read
/// the old character check stands, so a choice is never lost to a hiccup.
async fn checked_selection(s: &AppState, body: &Value, current: &Value) -> ApiResult<Value> {
    let instance = body["instanceId"].as_str().unwrap_or("");
    let model = body["model"].as_str().unwrap_or("");
    let effort = body["effort"].as_str();
    // The character check first: it also refuses an empty choice.
    let plain = provisioner::model_selection(instance, model).map_err(ApiError::bad_request)?;
    match s.providers.list(&s.t3).await {
        Ok(list) => providers::selection(&list, instance, model, effort, current)
            .map_err(ApiError::bad_request),
        Err(_) => Ok(plain),
    }
}

async fn model_get(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let bot = require_bot(&s, &slug).await?;
    let settings = s.settings().await;
    let current = [&bot["model"], &settings["defaultModel"]]
        .into_iter()
        .find(|m| m.is_object())
        .cloned()
        .unwrap_or_else(provisioner::default_model);
    Ok(Json(json!({
        "ok": true,
        "current": current["model"].as_str().unwrap_or(""),
        "instanceId": current["instanceId"].as_str().unwrap_or("claudeAgent"),
        "effort": providers::effort_of(&current),
        "runtimeMode": provisioner::launch_runtime_mode(&bot, &settings),
        "choices": provisioner::model_choices(&bot["model"], &settings["defaultModel"]),
    })))
}

async fn model_put(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let bot = require_bot(&s, &slug).await?;
    let body = parse(&body)?;
    let selection = checked_selection(&s, &body, &bot["model"]).await?;
    let mut patch = json!({ "model": selection });
    if body.get("runtimeMode").is_some() {
        let mode = body["runtimeMode"].as_str().unwrap_or("");
        if !matches!(mode, "approval-required" | "auto" | "full-access") {
            return Err(ApiError::bad_request(
                "runtimeMode must be approval-required, auto, or full-access",
            ));
        }
        patch["runtimeMode"] = json!(mode);
    }
    s.bots.save(&slug, &patch, false).await?;
    if let Ok(rpc) = s.t3.rpc().await {
        provisioner::push_model(&rpc, &bot, &selection)
            .await
            .map_err(|e| ApiError::new(502, format!("T3 didn't take the model: {e}")))?;
    }
    Ok(Json(json!({ "ok": true })))
}

/// Tokens and activity for one Bot or all of them over a period: `period` is today, 7d, 30d or
/// all; `bot` is a slug or all; `offset` is the person's minutes east of UTC.
async fn usage_report(
    State(s): S,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let period = q.get("period").map(String::as_str).unwrap_or("7d");
    let offset = q
        .get("offset")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
        .clamp(-840, 840);
    let which = q.get("bot").map(String::as_str).unwrap_or("all");
    let now_ms = i64::try_from(unix_now()).unwrap_or(i64::MAX) * 1000;
    let range = usage::period_days(period, now_ms, offset)
        .ok_or_else(|| ApiError::bad_request("Pick today, 7d, 30d or all."))?;
    let bots: Vec<Value> = s
        .bots
        .list()
        .await?
        .into_iter()
        .filter(|b| which == "all" || b["slug"] == which)
        .collect();
    if which != "all" && bots.is_empty() {
        return Err(ApiError::not_found("Bot not found"));
    }
    let doc = s.usage.doc().await?;
    let facts = s.roster.facts(&s).await;
    let (mut turns, mut by_bot, mut error) = (Vec::new(), Vec::new(), "");
    for bot in &bots {
        let slug = bot["slug"].as_str().unwrap_or("");
        let mine = match bot["mainThreadId"].as_str().filter(|t| !t.is_empty()) {
            Some(thread) => {
                let stamp = facts.get(thread).map(|f| f.stamp.as_str()).unwrap_or("");
                match s.usage.turns_for(&s, thread, stamp).await {
                    Ok(t) => t,
                    Err(_) => {
                        error = "Open T3 Code, then try again.";
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        };
        let own = usage::report(&mine, &usage::activity_by_day(&doc, slug), range, offset);
        by_bot.push(json!({
            "slug": slug, "name": bot["name"], "color": bot["color"], "shape": bot["shape"],
            "tokens": own["totals"]["tokens"], "turns": own["totals"]["turns"], "messages": own["totals"]["messages"],
        }));
        turns.extend(mine);
    }
    let activity = usage::activity_by_day(&doc, if which == "all" { "" } else { which });
    let mut out = usage::report(&turns, &activity, range, offset);
    out["period"] = json!(period);
    out["error"] = json!(error);
    out["byBot"] = json!(if which == "all" { by_bot } else { Vec::new() });
    Ok(Json(out))
}

async fn bot_usage(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let bot = require_bot(&s, &slug).await?;
    let (messages, attachments) = s.usage.bot_activity(&slug).await?;
    let Some(thread) = bot["mainThreadId"].as_str().filter(|t| !t.is_empty()) else {
        return Ok(Json(usage::bot_payload(
            messages,
            attachments,
            &usage::TokenTotals::default(),
            false,
            "This Bot is still getting set up.",
        )));
    };
    let Ok(rpc) = s.t3.rpc().await else {
        return Ok(Json(usage::bot_payload(
            messages,
            attachments,
            &usage::TokenTotals::default(),
            false,
            "Open T3 Code, then try again.",
        )));
    };
    let projection = match rpc
        .call(
            "orchestration.getThreadProjection",
            json!({ "threadId": thread }),
        )
        .await
    {
        Ok(projection) => projection,
        Err(_) => {
            return Ok(Json(usage::bot_payload(
                messages,
                attachments,
                &usage::TokenTotals::default(),
                false,
                "Open T3 Code, then try again.",
            )));
        }
    };
    let (totals, known) = usage::tokens_from_projection(&projection);
    Ok(Json(usage::bot_payload(
        messages,
        attachments,
        &totals,
        known,
        "",
    )))
}

async fn bot_routines(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let bot = require_bot(&s, &slug).await?;
    let Some(thread) = bot["mainThreadId"].as_str().filter(|t| !t.is_empty()) else {
        return Ok(Json(json!({
            "ok": true,
            "tasks": [],
            "error": "This Bot is still getting set up.",
        })));
    };
    let listed = routines::list(&s.t3)
        .await
        .map_err(|e| ApiError::new(502, format!("T3 didn't list routines: {}", e.message)))?;
    let tasks: Vec<Value> = routines::tasks_for_thread(&listed, thread)
        .iter()
        .map(routines::task_row)
        .collect();
    Ok(Json(json!({ "ok": true, "tasks": tasks, "error": "" })))
}

async fn bot_routine_save(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let bot = require_bot(&s, &slug).await?;
    let body = parse(&body)?;
    let task = routines::new_routine(
        &bot,
        body["title"].as_str().unwrap_or(""),
        body["kind"].as_str().unwrap_or("hour"),
        unix_now(),
    )
    .map_err(ApiError::bad_request)?;
    Ok(Json(
        routines::change(&s.t3, &s.routine_history, "upsert", routines::UPSERT, task).await?,
    ))
}

async fn owned_routine(s: &AppState, slug: &str, id: &str) -> ApiResult<()> {
    let bot = require_bot(s, slug).await?;
    let thread = bot["mainThreadId"].as_str().unwrap_or("");
    let listed = routines::list(&s.t3).await?;
    if routines::tasks_for_thread(&listed, thread)
        .iter()
        .any(|t| t["id"] == id)
    {
        Ok(())
    } else {
        Err(ApiError::not_found("That routine is not on this Bot."))
    }
}

async fn bot_routine_enabled(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let id = body["id"].as_str().unwrap_or("");
    owned_routine(&s, &slug, id).await?;
    Ok(Json(
        routines::change(
            &s.t3,
            &s.routine_history,
            "enabled",
            routines::SET_ENABLED,
            json!({ "id": id, "enabled": body["enabled"] == true }),
        )
        .await?,
    ))
}

async fn bot_routine_delete(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let id = parse(&body)?["id"].as_str().unwrap_or("").to_string();
    owned_routine(&s, &slug, &id).await?;
    Ok(Json(
        routines::change(
            &s.t3,
            &s.routine_history,
            "delete",
            routines::DELETE,
            json!({ "id": id }),
        )
        .await?,
    ))
}

async fn bot_routine_run(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let id = parse(&body)?["id"].as_str().unwrap_or("").to_string();
    owned_routine(&s, &slug, &id).await?;
    Ok(Json(
        routines::change(
            &s.t3,
            &s.routine_history,
            "run",
            routines::RUN_NOW,
            json!({ "id": id }),
        )
        .await?,
    ))
}

async fn lineage_get(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
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
        .map_err(|e| ApiError::new(502, format!("T3 didn't return the thread: {e}")))?;
    Ok(Json(
        json!({ "ok": true, "rows": lineage::rows(&projection) }),
    ))
}

async fn computer_get(
    State(s): S,
    Path(slug): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let thread = main_thread(&s, &slug).await?;
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    let listed = rpc
        .call("preview.list", json!({ "threadId": thread }))
        .await
        .map_err(|e| ApiError::new(502, format!("T3 didn't list pages: {e}")))?;
    let tabs = computer::tabs_for_screen(&listed);
    let asked = q.get("tab").map(String::as_str);
    let selected = computer::frame_tab(&tabs, asked)
        .and_then(|tab| tab.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut image = String::new();
    let mut error = String::new();
    // A held page keeps the JPEG from its interactive session. Watching that
    // same tab does not open a second socket.
    let held = {
        let sessions = computer::sessions()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        sessions.picture(&slug, &selected)
    };
    match held {
        Some(frame) => image = frame,
        None if !selected.is_empty() => {
            let size = if q.get("view").is_some_and(|view| view == "large") {
                computer::WatchSize::Large
            } else {
                computer::WatchSize::Card
            };
            match computer::one_frame(&s.t3, &thread, &selected, size).await {
                Ok(bytes) => image = library::encode_file(&bytes),
                Err(reason) => error = reason,
            }
        }
        None => {}
    }
    // A frame the app already has is not sent again.
    let tag = computer::etag(&image);
    let unchanged = !image.is_empty() && q.get("since").is_some_and(|s| *s == tag);
    Ok(Json(json!({
        "ok": true,
        "tabs": tabs,
        "image": if unchanged { String::new() } else { image },
        "etag": tag,
        "unchanged": unchanged,
        "selected": selected,
        "error": error,
    })))
}

/// Steers the Bot to open a server browser tab. Does not wait for the tab to appear.
async fn computer_open(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    computer_steer(&s, &slug, computer::open_command).await
}

/// Steers the Bot to close its server browser tabs. Does not wait for them to go.
async fn computer_close(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    computer_steer(&s, &slug, computer::close_command).await
}

async fn computer_steer(
    s: &AppState,
    slug: &str,
    command: fn(&str) -> Value,
) -> ApiResult<Json<Value>> {
    let thread = main_thread(s, slug).await?;
    let command = command(&thread);
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    rpc.call("orchestration.dispatchCommand", command.clone())
        .await
        .map_err(|e| ApiError::new(502, e.to_string()))?;
    Ok(Json(
        json!({ "ok": true, "messageId": command["messageId"] }),
    ))
}

async fn computer_control(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let thread = main_thread(&s, &slug).await?;
    let body = parse(&body)?;
    let tab = body["tabId"].as_str().unwrap_or("");
    let action = body["action"].as_str().unwrap_or("");
    if tab.is_empty() {
        return Err(ApiError::bad_request("Pick a page first."));
    }
    let frame = computer::send_control(&s.t3, &thread, tab, action)
        .await
        .map_err(ApiError::bad_request)?;
    let mut sessions = computer::sessions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    match action {
        "take" => {
            let width = u32::try_from(body["width"].as_u64().unwrap_or(0)).unwrap_or(0);
            let height = u32::try_from(body["height"].as_u64().unwrap_or(0)).unwrap_or(0);
            sessions.take(&slug, tab, width, height);
            if !frame.is_empty() {
                sessions.note_image(&slug, &library::encode_file(&frame));
                if let Some((w, h)) = computer::jpeg_size(&frame) {
                    sessions.remember_size(&slug, w, h);
                }
            }
        }
        "release" => sessions.release(&slug),
        _ => {}
    }
    Ok(Json(json!({ "ok": true })))
}

async fn computer_input(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let thread = main_thread(&s, &slug).await?;
    let body = parse(&body)?;
    let hold = {
        let sessions = computer::sessions()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        sessions.get(&slug).cloned()
    };
    let Some(hold) = hold else {
        return Err(ApiError::bad_request("Take control first."));
    };
    let width = body["width"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0)
        .unwrap_or(hold.width);
    let height = body["height"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0)
        .unwrap_or(hold.height);
    let box_w = num(&body, "boxW");
    let box_h = num(&body, "boxH");
    let (x, y) = if box_w > 0.0 && box_h > 0.0 {
        match computer::contain_fractions(
            num(&body, "x"),
            num(&body, "y"),
            box_w,
            box_h,
            width,
            height,
        ) {
            Ok(Some(point)) => point,
            Ok(None) => return Ok(Json(json!({ "ok": true }))),
            Err(reason) => return Err(ApiError::bad_request(reason)),
        }
    } else {
        (num(&body, "x"), num(&body, "y"))
    };
    let kind = body["kind"].as_str().unwrap_or("");
    let driven = match kind {
        "click" => {
            let messages =
                computer::click_messages(x, y, width, height).map_err(ApiError::bad_request)?;
            computer::send_input(&s.t3, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        "wheel" => {
            let messages = vec![
                computer::wheel_message(x, y, width, height, num(&body, "deltaY"))
                    .map_err(ApiError::bad_request)?,
            ];
            computer::send_input(&s.t3, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        "text" => {
            let messages = vec![computer::text_message(body["text"].as_str().unwrap_or(""))
                .map_err(ApiError::bad_request)?];
            computer::send_input(&s.t3, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        "key" => {
            let modifiers = match body.get("modifiers") {
                None | Some(serde_json::Value::Null) => 0,
                Some(value) => value
                    .as_i64()
                    .ok_or_else(|| ApiError::bad_request("Those modifiers are not sent."))?,
            };
            let message = computer::key_message(body["key"].as_str().unwrap_or(""), modifiers)
                .map_err(ApiError::bad_request)?;
            if let Some(sent) = body.get("keyCode").filter(|value| !value.is_null()) {
                let sent = sent
                    .as_i64()
                    .ok_or_else(|| ApiError::bad_request("That key is not sent."))?;
                if sent != message["keyCode"].as_i64().unwrap_or(sent) {
                    return Err(ApiError::bad_request("That key is not sent."));
                }
            }
            let messages = vec![message];
            computer::send_input(&s.t3, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        "copy" | "cut" => computer::send_edit(&s.t3, &thread, &hold.tab, kind)
            .await
            .map_err(ApiError::bad_request)?,
        "selectAll" => {
            let messages =
                vec![computer::edit_message("selectAll").map_err(ApiError::bad_request)?];
            computer::send_input(&s.t3, &thread, &hold.tab, &messages)
                .await
                .map_err(ApiError::bad_request)?
        }
        _ => {
            return Err(ApiError::bad_request(
                "Say click, wheel, text, key, copy, cut, or selectAll.",
            ))
        }
    };
    let mut sessions = computer::sessions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if sessions.get(&slug).is_some() {
        if width > 0 && height > 0 {
            sessions.take(&slug, &hold.tab, width, height);
        }
        if !driven.image.is_empty() {
            sessions.note_image(&slug, &library::encode_file(&driven.image));
            if let Some((w, h)) = computer::jpeg_size(&driven.image) {
                sessions.remember_size(&slug, w, h);
            }
        }
    }
    Ok(Json(json!({ "ok": true, "text": driven.clipboard })))
}

fn num(body: &Value, key: &str) -> f64 {
    body.get(key).and_then(Value::as_f64).unwrap_or(f64::NAN)
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

/// Rows of the parent's thread family. Used only to decide which child a send may use.
async fn parent_rows(s: &AppState, parent: &str) -> ApiResult<Vec<Value>> {
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    let projection = rpc
        .call(
            "orchestration.getThreadProjection",
            json!({ "threadId": parent }),
        )
        .await
        .map_err(|e| ApiError::new(502, e.to_string()))?;
    Ok(lineage::rows(&projection))
}

/// Parent thread, or a resolved local child. A remote child or the parent id is an error
/// and nothing has been written yet.
async fn resolve_chat_thread(s: &AppState, slug: &str, asked: Option<&str>) -> ApiResult<String> {
    let parent = main_thread(s, slug).await?;
    let child = asked.map(str::trim).filter(|id| !id.is_empty());
    if child.is_none() {
        return Ok(parent);
    }
    let rows = parent_rows(s, &parent).await?;
    lineage::thread_for(&rows, &parent, child).map_err(ApiError::bad_request)
}

async fn read_chat(
    State(s): S,
    Path(slug): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let parent = main_thread(&s, &slug).await?;
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    let asked = q
        .get("child")
        .map(String::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let (thread, read_only) = if let Some(child) = asked {
        let parent_projection = rpc
            .call(
                "orchestration.getThreadProjection",
                json!({ "threadId": parent }),
            )
            .await
            .map_err(|e| ApiError::new(502, e.to_string()))?;
        let id = lineage::thread_for(&lineage::rows(&parent_projection), &parent, Some(child))
            .map_err(ApiError::bad_request)?;
        // A resolved local child is the thread the person is in, and they can send on it.
        (id, false)
    } else {
        (parent, false)
    };
    let projection = rpc
        .call(
            "orchestration.getThreadProjection",
            json!({ "threadId": thread }),
        )
        .await
        .map_err(|e| ApiError::new(502, e.to_string()))?;
    let listed = s.bots.list().await?;
    let hidden = listed
        .iter()
        .any(|b| b["slug"].as_str() == Some(slug.as_str()) && b["hidden"] == true);
    let peers: chat::Peers = listed
        .iter()
        .filter_map(|b| {
            Some((
                b["mainThreadId"].as_str()?.to_string(),
                json!({ "slug": b["slug"], "name": b["name"], "color": b["color"], "shape": b["shape"] }),
            ))
        })
        .collect();
    // Opening the Bot's own chat advances last-read to the message id the inbox stored.
    // A child thread is not that chat, and a visible Bot keeps the roster's own mark.
    if asked.is_none() && hidden {
        if let Ok(meta) = s.threads.get(Some(&thread)).await {
            let stored = meta["lastRead"].as_str().unwrap_or("");
            let latest = meta["latest"].as_str().unwrap_or("");
            if !latest.is_empty() && stored != latest {
                let _ = s.threads.mark_read(Some(&thread), None, true, true).await;
            }
        }
    }
    let mut out = chat::with_tokens(chat::transcript_for(&projection, &peers), &projection);
    out["threadId"] = json!(thread);
    out["readOnly"] = json!(read_only);
    out["hidden"] = json!(hidden);
    // The client has no clock it may read in its data code, so the gateway says what time it is.
    out["now"] = json!(unix_now() * 1000);
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
    let parent = main_thread(&s, &slug).await?;
    let asked = body["child"]
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    // Resolve before any reply is stored or any command is dispatched. A refused child
    // builds no command, so the parent thread is not written.
    let command = if let Some(child) = asked {
        let rows = parent_rows(&s, &parent).await?;
        chat::child_send(&rows, &parent, child, &text, &files).map_err(ApiError::bad_request)?
    } else {
        chat::send_command(&parent, &text, &files)
    };
    let thread = command["threadId"].as_str().unwrap_or(&parent).to_string();
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    if let Some(reply_to) = body["replyTo"].as_str().filter(|id| !id.is_empty()) {
        s.threads
            .set_reply(
                Some(&thread),
                command["messageId"].as_str(),
                Some(reply_to),
                body["quote"].as_str(),
            )
            .await?;
    }
    rpc.call("orchestration.dispatchCommand", command.clone())
        .await
        .map_err(|e| ApiError::new(502, e.to_string()))?;
    s.usage.note_for(&slug, usage::Event::Message).await;
    Ok(Json(
        json!({ "ok": true, "messageId": command["messageId"] }),
    ))
}

async fn send_voice(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let seconds = body
        .get("seconds")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .ok_or_else(|| ApiError::bad_request("invalid duration"))?;
    let thread = resolve_chat_thread(&s, &slug, body["child"].as_str()).await?;
    let attachment_id = body["attachmentId"]
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    let (stored, wrote) = if let Some(id) = attachment_id {
        (
            chat::dispatch_stored_voice(
                &s.cfg.bots_root,
                &slug,
                &s.threads,
                &thread,
                &id,
                seconds,
                &routines::stamped(),
            )
            .await?,
            false,
        )
    } else {
        #[allow(
            clippy::map_err_ignore,
            reason = "a base64 error can quote the clip, which is the user's audio"
        )]
        let bytes = body["data"]
            .as_str()
            .map(|data| {
                base64::engine::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
            })
            .transpose()
            .map_err(|_| ApiError::bad_request("That clip could not be read."))?
            .unwrap_or_default();
        if bytes.is_empty() {
            return Err(ApiError::bad_request("That clip is empty."));
        }
        (
            chat::store_voice_send(
                &s.cfg.bots_root,
                &slug,
                &s.threads,
                &thread,
                &bytes,
                seconds,
                &routines::stamped(),
            )
            .await?,
            true,
        )
    };
    let rpc =
        s.t3.rpc()
            .await
            .map_err(|e| ApiError::new(502, format!("T3 unavailable: {e}")))?;
    rpc.call("orchestration.dispatchCommand", stored["command"].clone())
        .await
        .map_err(|e| ApiError::new(502, e.to_string()))?;
    s.usage.note_for(&slug, usage::Event::Message).await;
    if wrote {
        s.usage.note_for(&slug, usage::Event::Attachment).await;
    }
    Ok(Json(json!({
        "ok": true,
        "attachment": stored["attachment"],
        "voiceCalls": stored["voiceCalls"],
        "messageId": stored["command"]["messageId"],
    })))
}

async fn stop_chat(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let thread = resolve_chat_thread(&s, &slug, body["child"].as_str()).await?;
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
    // A web build with no icon of its own gets the brand's mark.
    if matches!(path, "favicon.ico" | "favicon.svg") && serve_index {
        return (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            hbots_design::FAVICON_SVG,
        )
            .into_response();
    }
    if !target.is_file() {
        return (StatusCode::NOT_FOUND, WEB_APP_MISSING).into_response();
    }
    // The web build is made with a placeholder gateway address (see app/app.ts); serve it with the
    // address this request came in on, so one build works wherever the gateway is reached.
    if target
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("js"))
    {
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

/// `index.html` is missing from `STATIC_DIR`. `cargo run` does not build the Exact web app.
const WEB_APP_MISSING: &str = "The Bots web app isn't built. Exact is a separate checkout (https://github.com/expo/exact), not a script in this repo. From that directory: EXACT_APP_DIR=<hbots>/app bun host/web/build.mjs bots-web (writes app/target/web-dist). An installed release looks for web/ next to the binary. Or set STATIC_DIR.";

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

    #[test]
    fn library_mime_ignores_extension_case() {
        assert_eq!(library_mime("page.HTML"), "text/html; charset=utf-8");
        assert_eq!(library_mime("page.HTM"), "text/html; charset=utf-8");
        assert_eq!(library_mime("shot.PNG"), "image/png");
        assert_eq!(library_mime("shot.JPEG"), "image/jpeg");
        assert_eq!(library_mime("shot.JPG"), "image/jpeg");
        assert_eq!(library_mime("notes.TXT"), "application/octet-stream");
    }

    fn settings_state(dir: &std::path::Path) -> Arc<AppState> {
        let app_dir = dir.join("bots/.botapp");
        let cfg = config::Config {
            host: "127.0.0.1".into(),
            port: 0,
            t3_url: "http://127.0.0.1:1".into(),
            t3_token: "fixture-only".into(),
            data_root: dir.to_path_buf(),
            bots_root: dir.join("bots"),
            static_dir: dir.join("dist"),
            password: String::new(),
            vault_file: dir.join("vault.enc"),
            vault_key_file: dir.join("vault-key"),
            bot_mcp_url: "http://127.0.0.1:1/mcp/bot".into(),
            allowed_origins: vec![],
            app_url: String::new(),
            access_team: String::new(),
            access_aud: String::new(),
            access_emails: vec![],
        };
        let tailnet = Arc::new(exposure::Tailnet::new());
        Arc::new(AppState {
            auth: auth::Auth::load(&app_dir, String::new(), vec![]).unwrap(),
            t3: t3::T3::new(cfg.t3_url.clone(), cfg.t3_token.clone()),
            bots: bots::Bots::new(
                cfg.bots_root.clone(),
                cfg.bot_mcp_url.clone(),
                "fixture-only".into(),
            ),
            vault: vault::Vault::new(cfg.vault_file.clone(), cfg.vault_key_file.clone()),
            projects: projects::Projects::new(app_dir.join("projects.json")),
            threads: thread_meta::ThreadMeta::new(app_dir.join("thread-meta.json")),
            roster: roster::Roster::default(),
            providers: providers::Providers::default(),
            rooms: rooms::Rooms::new(app_dir.join("groups.json")),
            room_working: Mutex::new(rooms::WorkingBook::default()),
            routine_history: routines::RoutineHistory::new(app_dir.join("routine-history.json")),
            icons: icons::Icons::new(dir.join("icons")),
            usage: usage::Usage::new(app_dir.join("usage.json")),
            library: library::Library::new(
                Arc::new(workspace::LocalDisk::new(cfg.bots_root.clone())),
                cfg.bot_mcp_url.clone(),
                "fixture-only".into(),
            ),
            system: release::System::new(cfg.data_root.clone(), cfg.app_dir()),
            mcp_token: "fixture-only".into(),
            provision_now: tokio::sync::Notify::new(),
            access: None,
            exposure: exposure::Exposure::from_env(|_| None),
            remote: Arc::new(remote_access::RemoteAccess::new(
                remote_access::RemoteConfig {
                    home: dir.to_path_buf(),
                    store_file: app_dir.join("access.json"),
                    url_file: app_dir.join("tunnel-url"),
                    local_port: 0,
                    env_password: String::new(),
                    env_tailnet: false,
                    env_tailnet_users: vec![],
                    tailscale_bin: None,
                    network_env: None,
                    process_host: None,
                    merged_host: None,
                },
                tailnet.clone(),
            )),
            tailnet,
            cfg,
        })
    }

    fn json_headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
        h
    }

    async fn put(state: &Arc<AppState>, body: Value) -> Result<Value, ApiError> {
        put_settings(
            State(state.clone()),
            json_headers(),
            Bytes::from(body.to_string()),
        )
        .await
        .map(|Json(v)| v)
    }

    #[tokio::test]
    async fn user_name_put_keeps_notifications() {
        let dir = tempfile::tempdir().unwrap();
        let state = settings_state(dir.path());
        let saved = json!({
            "userName": "Ada",
            "notifications": { "enabled": true, "sound": false, "preview": "hidden", "when": "background" }
        });
        store::write_json(&state.settings_path(), &saved)
            .await
            .unwrap();
        let next = put(&state, json!({ "userName": "Bea" })).await.unwrap();
        assert_eq!(next["userName"], "Bea");
        assert_eq!(next["notifications"], saved["notifications"]);
        let on_disk = store::read_json(&state.settings_path()).await.unwrap();
        assert_eq!(on_disk["notifications"], saved["notifications"]);
    }

    #[tokio::test]
    async fn bad_notifications_when_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let state = settings_state(dir.path());
        store::write_json(&state.settings_path(), &json!({ "userName": "Ada" }))
            .await
            .unwrap();
        let err = put(&state, json!({ "notifications": { "when": "always" } }))
            .await
            .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        let on_disk = store::read_json(&state.settings_path()).await.unwrap();
        assert!(on_disk.get("notifications").is_none());
    }

    #[tokio::test]
    async fn notifications_patch_keeps_other_fields() {
        let dir = tempfile::tempdir().unwrap();
        let state = settings_state(dir.path());
        store::write_json(
            &state.settings_path(),
            &json!({
                "notifications": { "enabled": false, "sound": false, "preview": "hidden", "when": "background" }
            }),
        )
        .await
        .unwrap();
        let next = put(&state, json!({ "notifications": { "enabled": true } }))
            .await
            .unwrap();
        assert_eq!(
            next["notifications"],
            json!({ "enabled": true, "sound": false, "preview": "hidden", "when": "background" })
        );
    }

    #[tokio::test]
    async fn group_answer_policy_defaults_to_last_and_put_rejects_anything_else() {
        let dir = tempfile::tempdir().unwrap();
        let state = settings_state(dir.path());
        assert_eq!(
            AppState::settings_defaults()["groupAnswerPolicy"],
            json!("last")
        );
        store::write_json(
            &state.settings_path(),
            &json!({ "groupAnswerPolicy": "nope", "userName": "Ada" }),
        )
        .await
        .unwrap();
        assert_eq!(state.settings().await["groupAnswerPolicy"], "last");
        let err = put(&state, json!({ "groupAnswerPolicy": "default" }))
            .await
            .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(
            err.message,
            "groupAnswerPolicy must be last, everyone, or chief"
        );
        let next = put(&state, json!({ "groupAnswerPolicy": "chief" }))
            .await
            .unwrap();
        assert_eq!(next["groupAnswerPolicy"], "chief");
        assert_eq!(next["userName"], "Ada");
        let again = state.settings().await;
        assert_eq!(again["groupAnswerPolicy"], "chief");
    }
}
