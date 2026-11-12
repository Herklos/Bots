//! Where the app answers, ported from `server/exposure.mts`.
//!
//! With a Tailscale listener configured (`HBOTS_TAILNET_PORT` / legacy `TAILNET_PORT`), the full app can live on that loopback
//! port alone, and only `tailscale serve` should proxy to it: Serve sets the signed-in user's login
//! on each request and replaces any value the client sent. The tailnet port never accepts
//! Cloudflare tokens, and the normal port can be reduced to the public landing page
//! (`HBOTS_PUBLIC_LANDING_ONLY=true` / legacy `PUBLIC_LANDING_ONLY`).
//!
//! Node tells the two ports apart by the socket's local port. Here each listener serves the same
//! router with a `Surface` marker added by a layer, so the rest of the gateway asks the request.

use crate::AppState;
use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderMap, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::task::JoinHandle;
use tower::ServiceExt;
use tower_http::services::ServeFile;

#[cfg(test)]
mod tests;

/// The settings `HBOTS_TAILNET_PORT`, `HBOTS_TAILNET_USERS` and `HBOTS_PUBLIC_LANDING_ONLY`
/// (legacy unprefixed names still accepted).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exposure {
    pub tailnet_port: u16,
    pub tailnet_users: Vec<String>,
    pub landing_only: bool,
}

fn env_pref(get: &impl Fn(&str) -> Option<String>, hbots: &str, legacy: &str) -> Option<String> {
    get(hbots).or_else(|| get(legacy))
}

impl Exposure {
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            tailnet_port: env_pref(&get, "HBOTS_TAILNET_PORT", "TAILNET_PORT")
                .and_then(|p| p.trim().parse().ok())
                .unwrap_or(0),
            tailnet_users: normalize_users(
                &env_pref(&get, "HBOTS_TAILNET_USERS", "TAILNET_USERS").unwrap_or_default(),
            ),
            landing_only: env_pref(&get, "HBOTS_PUBLIC_LANDING_ONLY", "PUBLIC_LANDING_ONLY")
                .as_deref()
                == Some("true"),
        }
    }
}

/// Comma-separated logins, trimmed, lowercased, de-duplicated, empties dropped.
pub(crate) fn normalize_users(list: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for user in list.split(',').map(|u| u.trim().to_lowercase()) {
        if !user.is_empty() && !out.contains(&user) {
            out.push(user);
        }
    }
    out
}

/// Which listener a request came in on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    Public,
    Tailnet,
}

/// The loopback listener for Tailscale Serve. It is open only while a tailnet user is allowed.
#[derive(Debug, Default)]
pub struct Tailnet {
    listener: Mutex<Option<Listener>>,
    users: Mutex<Vec<String>>,
}

#[derive(Debug)]
struct Listener {
    port: u16,
    task: JoinHandle<()>,
}

impl Tailnet {
    pub fn new() -> Self {
        Self::default()
    }

    /// The port the listener is on, or 0 when it is closed.
    pub fn port(&self) -> u16 {
        self.listener.lock().unwrap().as_ref().map_or(0, |l| l.port)
    }

    pub fn users(&self) -> Vec<String> {
        self.users.lock().unwrap().clone()
    }

    /// Start (or re-point) the listener on 127.0.0.1:`port` with the given allowed users.
    pub async fn open(&self, port: u16, users: &[String], router: Router) -> anyhow::Result<()> {
        if users.is_empty() {
            anyhow::bail!("The Tailscale listener needs at least one allowed user");
        }
        *self.users.lock().unwrap() = normalize_users(&users.join(","));
        if port != 0 && self.port() == port {
            return Ok(());
        }
        self.close().await;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
        let task = tokio::spawn(async move {
            if let Err(e) = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            {
                tracing::error!("tailnet listener stopped: {e}");
            }
        });
        *self.listener.lock().unwrap() = Some(Listener { port, task });
        tracing::info!("tailnet listener on 127.0.0.1:{port}");
        Ok(())
    }

    /// Stop the listener and free its port. Connections already open are not tracked here.
    pub async fn close(&self) {
        let taken = self.listener.lock().unwrap().take();
        if let Some(listener) = taken {
            listener.task.abort();
            let _ = listener.task.await;
        }
    }
}

/// Whether a request on the tailnet listener comes from an allowed Tailscale user.
pub(crate) fn tailnet_user_allowed(headers: &HeaderMap, users: &[String]) -> bool {
    headers
        .get("tailscale-user-login")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_lowercase())
        .is_some_and(|login| users.contains(&login))
}

/// The only paths the internet-facing port serves in landing-only mode.
pub(crate) fn landing_path(path: &str) -> bool {
    matches!(path, "/" | "/welcome" | "/welcome/") || is_landing_asset(path)
}

/// An image or font under the landing page: `/welcome/img/<name>` or `/welcome/fonts/<name>`.
pub(crate) fn is_landing_asset(path: &str) -> bool {
    let rest = path
        .strip_prefix("/welcome/img/")
        .or_else(|| path.strip_prefix("/welcome/fonts/"));
    rest.is_some_and(|r| {
        r.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
            && r.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    })
}

/// Anything under `/welcome` that isn't the landing page or one of its assets, including
/// encoded or `..` paths that try to climb out of it.
fn welcome_escape(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let under = ["/welcome/", "/welcome\\", "/welcome%2f", "/welcome%5c"]
        .iter()
        .any(|p| lower.starts_with(p));
    under && path != "/welcome/" && !is_landing_asset(path)
}

/// The file behind a landing path, under the built web app.
pub(crate) fn landing_file(static_dir: &Path, path: &str) -> Option<PathBuf> {
    if matches!(path, "/" | "/welcome" | "/welcome/") {
        return Some(static_dir.join("welcome").join("index.html"));
    }
    is_landing_asset(path).then(|| static_dir.join(path.trim_start_matches('/')))
}

/// Marks requests that arrive on the tailnet listener.
pub(crate) fn on_tailnet(router: Router) -> Router {
    router.layer(middleware::from_fn(
        |mut req: Request, next: Next| async move {
            req.extensions_mut().insert(Surface::Tailnet);
            next.run(req).await
        },
    ))
}

/// Marks requests on the public port and applies landing-only mode and the landing page routes.
pub(crate) fn on_public(router: Router, state: Arc<AppState>) -> Router {
    router.layer(middleware::from_fn_with_state(state, public_guard))
}

async fn public_guard(State(s): State<Arc<AppState>>, mut req: Request, next: Next) -> Response {
    req.extensions_mut().insert(Surface::Public);
    let path = req.uri().path().to_string();
    let read = matches!(*req.method(), Method::GET | Method::HEAD);
    // The updater and `hbots up` wait on this in every mode.
    if path == "/api/health" && *req.method() == Method::GET {
        return next.run(req).await;
    }
    if welcome_escape(&path) {
        return not_found();
    }
    if s.exposure.landing_only {
        // The app isn't here at all. Bots still reach their local tool server, which refuses tunnel traffic itself.
        if path == "/mcp/bot" && !req.headers().contains_key("cf-ray") {
            return next.run(req).await;
        }
        // Senders come through the tunnel, so `cf-ray` is expected. The URL token is the secret.
        if hook_path(&path) && hook_method(req.method()) {
            return next.run(req).await;
        }
        if read && landing_path(&path) {
            return serve_landing(&s.cfg.static_dir, &path, req).await;
        }
        return not_found();
    }
    if read && path != "/" && landing_path(&path) {
        return serve_landing(&s.cfg.static_dir, &path, req).await;
    }
    next.run(req).await
}

async fn serve_landing(static_dir: &Path, path: &str, req: Request) -> Response {
    let Some(file) = landing_file(static_dir, path) else {
        return not_found();
    };
    match ServeFile::new(file).oneshot(req).await {
        Ok(res) => res.into_response(),
        Err(infallible) => match infallible {},
    }
}

/// `/hooks/...` on the public port. POST, PUT, and PATCH only.
fn hook_path(path: &str) -> bool {
    path.starts_with("/hooks/")
}

fn hook_method(method: &Method) -> bool {
    matches!(*method, Method::POST | Method::PUT | Method::PATCH)
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        Body::from("not found"),
    )
        .into_response()
}
