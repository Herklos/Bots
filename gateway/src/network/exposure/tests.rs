use super::*;
use crate::{
    app,
    bots::Bots,
    network::auth::Auth,
    network::remote_access::{RemoteAccess, RemoteConfig},
    platform::config::Config,
    projects::Projects,
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
        storage: crate::storage::settings::ProcessEnv::default(),
    };
    let tailnet = Arc::new(Tailnet::new());
    let workspace: Arc<dyn crate::storage::workspace::Workspace> =
        Arc::new(crate::storage::LocalDisk::new(cfg.bots_root.clone()));
    Arc::new(AppState {
        auth: Auth::load(&app_dir, String::new(), vec![]).unwrap(),
        harness: crate::harness::connect(cfg.t3_url.clone(), cfg.t3_token.clone()),
        bots: Bots::with_workspace(
            cfg.bots_root.clone(),
            cfg.bot_mcp_url.clone(),
            "fixture-only".into(),
            Arc::clone(&workspace),
        ),
        vault: Vault::new(cfg.vault_file.clone(), cfg.vault_key_file.clone()),
        projects: Projects::new(app_dir.join("projects.json")),
        threads: crate::chat::thread_meta::ThreadMeta::new(app_dir.join("thread-meta.json")),
        roster: crate::bots::roster::Roster::default(),
        providers: crate::harness::providers::Providers::default(),
        rooms: crate::chat::rooms::Rooms::new(app_dir.join("groups.json")),
        room_working: std::sync::Mutex::new(crate::chat::rooms::WorkingBook::default()),
        routine_history: crate::chat::routines::RoutineHistory::new(
            app_dir.join("routine-history.json"),
        ),
        icons: crate::bots::icons::Icons::new(dir.join("icons")),
        usage: crate::bots::usage::Usage::new(app_dir.join("usage.json")),
        memory: crate::bots::memory::MemoryStore::new(Arc::clone(&workspace)),
        workspace: Arc::clone(&workspace),
        library: crate::bots::library::Library::new(
            workspace,
            cfg.bot_mcp_url.clone(),
            "fixture-only".into(),
        ),
        images: crate::chat::images::Resolver::new(),
        code_signals: crate::chat::code::SignalCache::default(),
        code_reads: crate::chat::code::ReadCache::default(),
        system: crate::platform::release::System::new(cfg.data_root.clone(), cfg.app_dir()),
        mcp_token: "fixture-only".into(),
        provision_now: tokio::sync::Notify::new(),
        holds: crate::chat::HeldReads::new(),
        hook_limit: Arc::new(crate::network::ratelimit::Limiter::new()),
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
                network_env: None,
                process_host: None,
                merged_host: None,
            },
            tailnet.clone(),
        )),
        tailnet,
        sync: crate::storage::sync::SyncSlot::default(),
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
    let access = crate::network::access::Access::new(
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

#[tokio::test]
async fn landing_only_lets_hook_posts_through_and_still_hides_the_api() {
    let _guard = crate::api::test_env_lock().await;
    let dir = tempfile::tempdir().unwrap();
    let s = state(
        dir.path(),
        Exposure {
            landing_only: true,
            ..Default::default()
        },
    );
    let port = serve(on_public(app(s.clone()), s.clone())).await;
    let client = reqwest::Client::new();
    let posted = client
        .post(format!("http://127.0.0.1:{port}/hooks/task/token"))
        .header("cf-ray", "ray-from-the-tunnel")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_ne!(posted.status().as_u16(), 404);
    assert!(posted.text().await.unwrap().contains("\"ok\""));
    let plain = client
        .post(format!("http://127.0.0.1:{port}/hooks/task/token"))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_ne!(plain.status().as_u16(), 404);
    assert_eq!(get(port, "/api/bots", &[]).await.0, 404);
    assert_eq!(
        get(
            port,
            "/hooks/task/token",
            &[("cf-ray", "ray-from-the-tunnel")]
        )
        .await,
        (404, "not found".into())
    );
}

#[tokio::test]
async fn the_tailnet_listener_allows_hooks_without_a_tailscale_user() {
    let _guard = crate::api::test_env_lock().await;
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
    let client = reqwest::Client::new();
    let posted = client
        .post(format!("http://127.0.0.1:{tailnet_port}/hooks/task/token"))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_ne!(posted.status().as_u16(), 401);
    assert!(posted.text().await.unwrap().contains("\"ok\""));
    assert_eq!(get(tailnet_port, "/api/bots", &[]).await.0, 401);
    s.tailnet.close().await;
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
