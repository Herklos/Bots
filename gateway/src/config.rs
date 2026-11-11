//! Configuration from the environment and `.env`. An existing `~/.hbots` keeps working.
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// T3 Code, the harness backend wired today, e.g. `http://127.0.0.1:3773`.
    /// Other backends may be added later.
    pub t3_url: String,
    pub t3_token: String,
    pub data_root: PathBuf,
    pub bots_root: PathBuf,
    /// Environment locks for storage. The key text is not kept here.
    pub storage: crate::storage_settings::ProcessEnv,
    /// The built Exact web app, served at `/`. An installed release uses `web/` next to the binary.
    /// A checkout uses `app/target/web-dist`.
    pub static_dir: PathBuf,
    pub password: String,
    pub vault_file: PathBuf,
    pub vault_key_file: PathBuf,
    pub bot_mcp_url: String,
    pub allowed_origins: Vec<String>,
    pub app_url: String,
    /// Cloudflare Access team name (or host) and the application's audience tag.
    pub access_team: String,
    pub access_aud: String,
    /// Empty allows any email on a token that already passed the other checks.
    pub access_emails: Vec<String>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Only debug builds may opt into unrestricted browser origins for LAN development.
pub(crate) fn dev_allow_any_origin() -> bool {
    cfg!(debug_assertions) && std::env::var("HBOTS_DEV_ALLOW_ANY_ORIGIN").as_deref() == Ok("1")
}

fn home() -> PathBuf {
    env("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The web app an installed release carries: `web/` next to the binary. A checkout falls back to
/// the Exact build at `app/target/web-dist`.
fn default_static_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("web")))
        .filter(|d| d.join("index.html").is_file())
        .unwrap_or_else(|| PathBuf::from("app/target/web-dist"))
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let data_root = env("HBOTS_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".hbots"));
        // The checkout's .env, then HBOTS_HOME/.env. Real env wins.
        let _ = dotenvy::from_filename(".env");
        let _ = dotenvy::from_path(data_root.join(".env"));
        let port: u16 = env("PORT").and_then(|p| p.parse().ok()).unwrap_or(3780);
        let t3_token = match env("T3_TOKEN") {
            Some(t) => t,
            None => {
                let file = env("T3_TOKEN_FILE")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| data_root.join(".t3-token"));
                std::fs::read_to_string(&file).unwrap_or_default()
            }
        }
        .trim()
        .to_string();
        if t3_token.is_empty() {
            anyhow::bail!("No T3 token: set T3_TOKEN or T3_TOKEN_FILE (t3 auth session issue --ttl 365d --label hbots --token-only)");
        }
        // Environment, then ~/.hbots/storage.json, then the default folder.
        // A bad saved folder falls back to the default and does not stop startup.
        let storage = crate::storage_settings::ProcessEnv::capture();
        let bots_root = crate::storage_settings::resolve_root(&data_root, &storage);
        Ok(Self {
            host: env("HOST").unwrap_or_else(|| "127.0.0.1".into()),
            port,
            t3_url: env("T3_URL")
                .unwrap_or_else(|| "http://127.0.0.1:3773".into())
                .trim_end_matches('/')
                .to_string(),
            t3_token,
            static_dir: env("STATIC_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(default_static_dir),
            password: env("HBOTS_PASSWORD").unwrap_or_default(),
            vault_file: env("VAULT_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| data_root.join("vault.enc")),
            vault_key_file: env("VAULT_KEY_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| data_root.join(".vault-key")),
            bot_mcp_url: env("HBOTS_MCP_URL")
                .unwrap_or_else(|| format!("http://127.0.0.1:{port}/mcp/bot"))
                .trim_end_matches('/')
                .to_string(),
            allowed_origins: env("ALLOWED_ORIGINS")
                .map(|s| {
                    s.split(',')
                        .map(|o| o.trim().to_string())
                        .filter(|o| !o.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            app_url: env("APP_URL")
                .unwrap_or_default()
                .trim_end_matches('/')
                .to_string(),
            access_team: env("HBOTS_ACCESS_TEAM").unwrap_or_default(),
            access_aud: env("HBOTS_ACCESS_AUD").unwrap_or_default(),
            access_emails: env("HBOTS_ACCESS_EMAILS")
                .map(|s| {
                    s.split(',')
                        .map(|e| e.trim().to_ascii_lowercase())
                        .filter(|e| !e.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            data_root,
            bots_root,
            storage,
        })
    }

    /// `bots/.botapp`: settings, sessions, the Projects board, the main-Bot choice.
    pub fn app_dir(&self) -> PathBuf {
        self.bots_root.join(".botapp")
    }
}
