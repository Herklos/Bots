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
mod api;
mod bots;
mod chat;
mod harness; // The agent runtime: T3 Code today.
mod network; // Where the gateway listens: this device, the local network, or Tailscale.
mod platform; // Config, errors, the CLI, the service, and self-update.
mod projects;
mod storage; // Where Bots live: the Workspace trait, saved choice, and backend adapters.
mod vault;

pub use api::AppState;
use api::*;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = crate::platform::cli::run(&args).await {
        std::process::exit(code);
    }
    // Snapshot HOST before dotenv. `hbots up` writes HOST=127.0.0.1 into ~/.hbots/.env, and that
    // default must not lock Settings. A HOST already in the process environment is intentional.
    let process_host = std::env::var("HOST").ok().filter(|v| !v.is_empty());
    let process_network = std::env::var("HBOTS_NETWORK_ACCESS")
        .ok()
        .filter(|v| !v.is_empty());
    let cfg = crate::platform::config::Config::from_env()?;
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
            crate::network::auth::write_private(&token_path, &(t.clone() + "\n"))?;
            t
        }
    };
    let exposure_cfg = crate::network::exposure::Exposure::from_env(|k| {
        std::env::var(k).ok().filter(|v| !v.is_empty())
    });
    let tailnet = Arc::new(crate::network::exposure::Tailnet::new());
    let remote = Arc::new(crate::network::remote_access::RemoteAccess::new(
        crate::network::remote_access::RemoteConfig {
            home: cfg.data_root.clone(),
            store_file: app_dir.join("access.json"),
            url_file: app_dir.join("tunnel-url"),
            local_port: cfg.port,
            env_password: cfg.password.clone(),
            env_tailnet: exposure_cfg.tailnet_port > 0,
            env_tailnet_users: exposure_cfg.tailnet_users.clone(),
            tailscale_bin: crate::network::remote_access::find_tailscale(),
            network_env,
            process_host,
            merged_host,
        },
        tailnet.clone(),
    ));
    let access = crate::network::access::Access::from_env(
        &cfg.access_team,
        &cfg.access_aud,
        &cfg.access_emails.join(","),
    );
    let workspace: Arc<dyn storage::workspace::Workspace> =
        Arc::new(storage::LocalDisk::new(cfg.bots_root.clone()));
    let state = Arc::new(AppState {
        access,
        exposure: exposure_cfg,
        tailnet: tailnet.clone(),
        remote: remote.clone(),
        auth: crate::network::auth::Auth::load(
            &app_dir,
            cfg.password.clone(),
            cfg.allowed_origins.clone(),
        )?,
        harness: crate::harness::connect(cfg.t3_url.clone(), cfg.t3_token.clone()),
        bots: bots::Bots::with_workspace(
            cfg.bots_root.clone(),
            cfg.bot_mcp_url.clone(),
            mcp_token.clone(),
            Arc::clone(&workspace),
        ),
        vault: vault::Vault::new(cfg.vault_file.clone(), cfg.vault_key_file.clone()),
        projects: projects::Projects::new(app_dir.join("projects.json")),
        threads: crate::chat::thread_meta::ThreadMeta::new(app_dir.join("thread-meta.json")),
        roster: crate::bots::roster::Roster::default(),
        providers: crate::harness::providers::Providers::default(),
        rooms: crate::chat::rooms::Rooms::new(app_dir.join("groups.json")),
        room_working: Mutex::new(crate::chat::rooms::WorkingBook::default()),
        routine_history: crate::chat::routines::RoutineHistory::new(
            app_dir.join("routine-history.json"),
        ),
        system: crate::platform::release::System::new(cfg.data_root.clone(), app_dir.clone()),
        icons: crate::bots::icons::Icons::new(cfg.data_root.join("icons")),
        usage: crate::bots::usage::Usage::new(app_dir.join("usage.json")),
        memory: crate::bots::memory::MemoryStore::new(Arc::clone(&workspace)),
        workspace: Arc::clone(&workspace),
        library: crate::bots::library::Library::new(
            workspace,
            cfg.bot_mcp_url.clone(),
            mcp_token.clone(),
        ),
        images: chat::images::Resolver::new(),
        code_signals: chat::code::SignalCache::default(),
        mcp_token,
        provision_now: tokio::sync::Notify::new(),
        holds: chat::HeldReads::new(),
        cfg,
    });
    // Write the defaults once; an existing file, even an unreadable one, is left alone.
    if !state.settings_path().exists() {
        if let Err(e) =
            storage::write_json(&state.settings_path(), &AppState::settings_defaults()).await
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
    crate::bots::provisioner::start(state.clone());
    crate::bots::agent_context::start(state.clone());
    crate::platform::release::start(state.clone()).await;

    let applied = network::apply(
        &state.remote.network_choice(),
        state.remote.password_required(),
        crate::platform::config::dev_allow_any_origin(),
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
        let router = crate::network::exposure::on_tailnet(app(state.clone()));
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
        let router = crate::network::exposure::on_tailnet(app(booting.clone()));
        booting.remote.boot(router).await;
    });
    let stopping = state.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        stopping.holds.stop();
        stopping.remote.stop().await;
        // Held reads are waiting on `holds`. Give them a moment to answer, then leave.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
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
        crate::network::exposure::on_public(app(state.clone()), state.clone())
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
