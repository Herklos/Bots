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
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        Body::from("not found"),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app,
        auth::Auth,
        bots::Bots,
        config::Config,
        projects::Projects,
        remote_access::{RemoteAccess, RemoteConfig},
        t3::T3,
        vault::Vault,
    };
    use axum::http::HeaderValue;

    fn env_map<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k: &str| {
            pairs
                .iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn configuration_is_explicit() {
        assert_eq!(Exposure::from_env(env_map(&[])), Exposure::default());
        let on = Exposure::from_env(env_map(&[
            ("TAILNET_USERS", " A@b.com, c@d.com,A@B.com "),
            ("TAILNET_PORT", "3781"),
        ]));
        assert_eq!(on.tailnet_users, vec!["a@b.com", "c@d.com"]);
        assert_eq!(on.tailnet_port, 3781);
        let preferred = Exposure::from_env(env_map(&[
            ("HBOTS_TAILNET_PORT", "3790"),
            ("TAILNET_PORT", "3781"),
        ]));
        assert_eq!(preferred.tailnet_port, 3790);
        assert!(!Exposure::from_env(env_map(&[("PUBLIC_LANDING_ONLY", "1")])).landing_only);
        assert!(Exposure::from_env(env_map(&[("PUBLIC_LANDING_ONLY", "true")])).landing_only);
        assert!(Exposure::from_env(env_map(&[("HBOTS_PUBLIC_LANDING_ONLY", "true")])).landing_only);
    }

    #[test]
    fn only_the_landing_page_and_its_assets_are_public() {
        for url in [
            "/",
            "/welcome/",
            "/welcome",
            "/welcome/img/ball.webp",
            "/welcome/fonts/a-b.woff2",
        ] {
            assert!(landing_path(url), "{url}");
        }
        for url in [
            "/welcome/../api/bots",
            "/welcome/img/../../sw.js",
            "/welcome/img/.env",
            "/index.html",
            "/welcome/img/a/b.png",
        ] {
            assert!(!landing_path(url), "{url}");
        }
        assert!(welcome_escape("/welcome/../api/bots"));
        assert!(welcome_escape("/welcome%2f..%2fapi"));
        assert!(!welcome_escape("/welcome/img/ball.webp"));
    }

    #[test]
    fn the_tailnet_user_comes_from_the_header() {
        let users = vec!["me@example.com".to_string()];
        let mut h = HeaderMap::new();
        assert!(!tailnet_user_allowed(&h, &users));
        h.insert(
            "tailscale-user-login",
            HeaderValue::from_static("ME@example.com "),
        );
        assert!(tailnet_user_allowed(&h, &users));
        h.insert(
            "tailscale-user-login",
            HeaderValue::from_static("guest@example.com"),
        );
        assert!(!tailnet_user_allowed(&h, &users));
    }

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// A gateway state over temp folders and a fake web build, with the given exposure settings.
    fn state(dir: &Path, exposure: Exposure) -> Arc<AppState> {
        let static_dir = dir.join("dist");
        std::fs::create_dir_all(static_dir.join("welcome/img")).unwrap();
        std::fs::write(static_dir.join("index.html"), "APP SHELL").unwrap();
        std::fs::write(static_dir.join("welcome/index.html"), "LANDING").unwrap();
        std::fs::write(static_dir.join("welcome/img/ball.webp"), "BALL").unwrap();
        let app_dir = dir.join("bots/.botapp");
        let cfg = Config {
            host: "127.0.0.1".into(),
            port: 0,
            t3_url: "http://127.0.0.1:1".into(),
            t3_token: "fixture-only".into(),
            data_root: dir.to_path_buf(),
            bots_root: dir.join("bots"),
            static_dir,
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
        let tailnet = Arc::new(Tailnet::new());
        Arc::new(AppState {
            auth: Auth::load(&app_dir, String::new(), vec![]).unwrap(),
            t3: T3::new(cfg.t3_url.clone(), cfg.t3_token.clone()),
            bots: Bots::new(
                cfg.bots_root.clone(),
                cfg.bot_mcp_url.clone(),
                "fixture-only".into(),
            ),
            vault: Vault::new(cfg.vault_file.clone(), cfg.vault_key_file.clone()),
            projects: Projects::new(app_dir.join("projects.json")),
            threads: crate::thread_meta::ThreadMeta::new(app_dir.join("thread-meta.json")),
            roster: crate::roster::Roster::default(),
            providers: crate::providers::Providers::default(),
            rooms: crate::rooms::Rooms::new(app_dir.join("groups.json")),
            routine_history: crate::routines::RoutineHistory::new(
                app_dir.join("routine-history.json"),
            ),
            icons: crate::icons::Icons::new(dir.join("icons")),
            usage: crate::usage::Usage::new(app_dir.join("usage.json")),
            library: crate::library::Library::new(
                Arc::new(crate::workspace::LocalDisk::new(cfg.bots_root.clone())),
                cfg.bot_mcp_url.clone(),
                "fixture-only".into(),
            ),
            system: crate::release::System::new(cfg.data_root.clone(), cfg.app_dir()),
            mcp_token: "fixture-only".into(),
            provision_now: tokio::sync::Notify::new(),
            access: None,
            exposure,
            remote: Arc::new(RemoteAccess::new(
                RemoteConfig {
                    home: dir.to_path_buf(),
                    store_file: app_dir.join("access.json"),
                    url_file: app_dir.join("tunnel-url"),
                    local_port: 0,
                    env_password: String::new(),
                    env_tailnet: false,
                    env_tailnet_users: vec![],
                    tailscale_bin: None,
                },
                tailnet.clone(),
            )),
            tailnet,
            cfg,
        })
    }

    /// Serve a router on a random local port, as the gateway does for each listener.
    async fn serve(router: Router) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap()
        });
        port
    }

    async fn get(port: u16, path: &str, headers: &[(&str, &str)]) -> (u16, String) {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let mut req = client.get(format!("http://127.0.0.1:{port}{path}"));
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let res = req.send().await.unwrap();
        (res.status().as_u16(), res.text().await.unwrap())
    }

    #[tokio::test]
    async fn landing_only_public_port_shows_the_landing_page_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let s = state(
            dir.path(),
            Exposure {
                landing_only: true,
                ..Default::default()
            },
        );
        let port = serve(on_public(app(s.clone()), s.clone())).await;
        assert_eq!(get(port, "/", &[]).await, (200, "LANDING".into()));
        assert_eq!(get(port, "/welcome/", &[]).await, (200, "LANDING".into()));
        assert_eq!(
            get(port, "/welcome/img/ball.webp", &[]).await,
            (200, "BALL".into())
        );
        for url in [
            "/index.html",
            "/sw.js",
            "/login",
            "/api/session",
            "/api/bots",
            "/t3/ws",
            "/welcome/img/missing.png",
        ] {
            assert_eq!(get(port, url, &[]).await.0, 404, "{url}");
        }
        assert_eq!(
            get(
                port,
                "/api/bots",
                &[("tailscale-user-login", "me@example.com")]
            )
            .await
            .0,
            404
        );
        assert_eq!(
            get(port, "/api/health", &[]).await.0,
            200,
            "health stays up for the updater"
        );
    }

    #[tokio::test]
    async fn the_local_tool_server_stays_reachable_but_not_through_cloudflare() {
        let dir = tempfile::tempdir().unwrap();
        let s = state(
            dir.path(),
            Exposure {
                landing_only: true,
                ..Default::default()
            },
        );
        let port = serve(on_public(app(s.clone()), s.clone())).await;
        let res = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/mcp/bot"))
            .header("authorization", "Bearer fixture-only")
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_ne!(res.status().as_u16(), 404);
        let res = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/mcp/bot"))
            .header("cf-ray", "x")
            .header("authorization", "Bearer fixture-only")
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(res.status().as_u16(), 404);
    }

    #[tokio::test]
    async fn the_tailnet_listener_serves_the_app_to_its_user_only() {
        let dir = tempfile::tempdir().unwrap();
        let s = state(dir.path(), Exposure::default());
        let tailnet_port = free_port();
        s.tailnet
            .open(
                tailnet_port,
                &["me@example.com".into()],
                on_tailnet(app(s.clone())),
            )
            .await
            .unwrap();
        let you = [("tailscale-user-login", "ME@example.com ")];
        assert_eq!(get(tailnet_port, "/", &[]).await, (200, "APP SHELL".into()));
        assert_eq!(get(tailnet_port, "/api/bots", &you).await.0, 200);
        assert_eq!(get(tailnet_port, "/api/bots", &[]).await.0, 401);
        assert_eq!(
            get(
                tailnet_port,
                "/api/bots",
                &[("tailscale-user-login", "guest@example.com")]
            )
            .await
            .0,
            401
        );
        assert_eq!(
            get(
                tailnet_port,
                "/api/bots",
                &[("cf-access-jwt-assertion", "a.b.c")]
            )
            .await
            .0,
            401
        );
        assert!(
            get(tailnet_port, "/api/config", &you)
                .await
                .1
                .contains("\"accessLogout\":false"),
            "signing out on the tailnet stays on the tailnet"
        );
        s.tailnet.close().await;
        assert_eq!(s.tailnet.port(), 0);
    }

    #[tokio::test]
    async fn the_normal_public_port_needs_a_session_when_cloudflare_access_is_configured() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = state(dir.path(), Exposure::default());
        // Cloudflare Access on: a request without a valid token or session is refused.
        let access = crate::access::Access::new(
            "https://fixture.invalid".into(),
            "http://127.0.0.1:1/certs".into(),
            "fixture".into(),
            vec!["me@example.com".into()],
        );
        Arc::get_mut(&mut s).unwrap().access = Some(access);
        let port = serve(on_public(app(s.clone()), s.clone())).await;
        assert_eq!(get(port, "/", &[]).await, (200, "APP SHELL".into()));
        assert_eq!(get(port, "/api/bots", &[]).await.0, 401);
        assert_eq!(
            get(
                port,
                "/api/bots",
                &[("tailscale-user-login", "me@example.com")]
            )
            .await
            .0,
            401,
            "a Tailscale header means nothing on the public port"
        );
        assert_eq!(
            get(port, "/welcome/img/ball.webp", &[]).await,
            (200, "BALL".into())
        );
        assert_eq!(get(port, "/welcome%2f..%2fx", &[]).await.0, 404);
    }

    #[test]
    fn the_landing_file_maps_paths_under_the_web_build() {
        let root = Path::new("/srv/app");
        assert_eq!(
            landing_file(root, "/"),
            Some(root.join("welcome/index.html"))
        );
        assert_eq!(
            landing_file(root, "/welcome"),
            Some(root.join("welcome/index.html"))
        );
        assert_eq!(
            landing_file(root, "/welcome/img/x.webp"),
            Some(root.join("welcome/img/x.webp"))
        );
        assert_eq!(landing_file(root, "/index.html"), None);
    }
}
