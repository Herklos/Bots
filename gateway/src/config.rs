//! Configuration, read from the same environment variables and `.env` file as Threebot's
//! Node gateway (`server/gateway.mts`), so an existing `~/.threebot` keeps working.
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// T3 Code server, e.g. `http://127.0.0.1:3773`.
    pub t3_url: String,
    pub t3_token: String,
    pub data_root: PathBuf,
    pub bots_root: PathBuf,
    /// The built Exact web app (`app/` built with `exact.mjs web --build`), served at `/`.
    pub static_dir: PathBuf,
    pub password: String,
    pub vault_file: PathBuf,
    pub vault_key_file: PathBuf,
    pub bot_mcp_url: String,
    pub allowed_origins: Vec<String>,
    pub app_url: String,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn home() -> PathBuf {
    env("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let data_root = env("THREEBOT_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".threebot"));
        // Same order as Threebot: the checkout's .env, then THREEBOT_HOME/.env. Real env wins.
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
            anyhow::bail!("No T3 token: set T3_TOKEN or T3_TOKEN_FILE (t3 auth session issue --ttl 30d --label threebot --token-only)");
        }
        let bots_root = env("BOTS_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_root.join("bots"));
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
                .unwrap_or_else(|| PathBuf::from("app/dist")),
            password: env("BOT_PASSWORD").unwrap_or_default(),
            vault_file: env("VAULT_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| data_root.join("vault.enc")),
            vault_key_file: env("VAULT_KEY_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| data_root.join(".vault-key")),
            bot_mcp_url: env("BOT_MCP_URL")
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
            data_root,
            bots_root,
        })
    }

    /// `bots/.botapp`: settings, sessions, the Projects board, the main-Bot choice.
    pub fn app_dir(&self) -> PathBuf {
        self.bots_root.join(".botapp")
    }
}
