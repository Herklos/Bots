//! One-click ways to reach Bots from a phone, ported from `server/remote-access.mts` (modelled on
//! T3 Code's):
//! - Tailscale: `tailscale serve` on a free HTTPS port, in front of a loopback-only listener that
//!   trusts the signed-in Tailscale user. Private to the tailnet.
//! - Cloudflare: a quick tunnel (`cloudflared tunnel --url`), a public https://*.trycloudflare.com
//!   link with no account. Public, so it only starts once the app has a password.
//!
//! The choices persist in `access.json` (the Node gateway's format, so either gateway reads it)
//! and are applied again when the gateway starts. The app password is stored as a scrypt hash.
use crate::{auth, exposure::Tailnet};
use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::{mpsc, oneshot, Mutex as AsyncMutex},
    task::JoinHandle,
};

pub const MIN_PASSWORD: usize = 8;
const CMD_TIMEOUT: Duration = Duration::from_secs(10);
const STATUS_TIMEOUT: Duration = Duration::from_secs(5);
const TUNNEL_START_TIMEOUT: Duration = Duration::from_secs(60);
const TUNNEL_STOP_TIMEOUT: Duration = Duration::from_secs(2);
/// Tailscale Serve's HTTPS ports,.
const WEB_PORTS: [u16; 5] = [443, 8443, 8444, 8445, 9443];
/// The same pinned cloudflared and checksums T3 Code downloads.
pub const CLOUDFLARED_VERSION: &str = "2026.10.4";
const CF_BASE: &str = "https://github.com/cloudflare/cloudflared/releases/download/2026.10.4";

// ---------- errors ----------

/// A failed request: a status and a message that is safe to show, plus a code the app can act on.
#[derive(Debug)]
pub struct AccessError {
    pub status: u16,
    pub code: Option<&'static str>,
    pub message: String,
}

impl AccessError {
    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: 409,
            code: None,
            message: message.into(),
        }
    }
    pub fn coded(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: 409,
            code: Some(code),
            message: message.into(),
        }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            code: None,
            message: message.into(),
        }
    }
    pub fn internal(e: impl std::fmt::Display) -> Self {
        Self {
            status: 500,
            code: None,
            message: e.to_string(),
        }
    }
}

impl IntoResponse for AccessError {
    fn into_response(self) -> Response {
        let mut body = json!({ "error": self.message });
        if let Some(code) = self.code {
            body["code"] = json!(code);
        }
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (
            status,
            [(header::X_CONTENT_TYPE_OPTIONS, "nosniff")],
            Json(body),
        )
            .into_response()
    }
}

// ---------- access.json ----------

/// A saved password: its scrypt hash and salt, base64.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredPassword {
    pub salt: String,
    pub hash: String,
    pub set_at: String,
}

/// The saved Tailscale route. An enabled route always has its ports and users saved with it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleSaved {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub https_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub users: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelSaved {
    #[serde(default)]
    pub enabled: bool,
}

/// What `access.json` holds.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<StoredPassword>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailscale: Option<TailscaleSaved>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelSaved>,
}

/// A missing or unreadable file reads as nothing saved.
fn read_state(file: &Path) -> AccessState {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_state(file: &Path, state: &AccessState) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(state).map_err(std::io::Error::other)? + "\n";
    let temp = PathBuf::from(format!("{}.{}.tmp", file.display(), std::process::id()));
    auth::write_private(&temp, &text)?;
    std::fs::rename(temp, file)
}

// ---------- passwords ----------

/// scrypt with Node's defaults (N=16384, r=8, p=1),.
fn scrypt_bytes(password: &[u8], salt: &[u8], len: usize) -> anyhow::Result<Vec<u8>> {
    // The scrypt crate's error types don't implement std::error::Error without its std feature.
    let params =
        scrypt::Params::new(14, 8, 1, len).map_err(|e| anyhow::anyhow!("scrypt params: {e:?}"))?;
    let mut out = vec![0u8; len];
    scrypt::scrypt(password, salt, &params, &mut out)
        .map_err(|e| anyhow::anyhow!("scrypt: {e:?}"))?;
    Ok(out)
}

pub async fn hash_password(password: String) -> anyhow::Result<StoredPassword> {
    tokio::task::spawn_blocking(move || {
        let salt: [u8; 16] = rand::random();
        let hash = scrypt_bytes(password.as_bytes(), &salt, 32)?;
        Ok(StoredPassword {
            salt: B64.encode(salt),
            hash: B64.encode(hash),
            set_at: iso_now(),
        })
    })
    .await?
}

pub async fn verify_password(password: String, stored: StoredPassword) -> bool {
    tokio::task::spawn_blocking(move || {
        let (Ok(salt), Ok(expected)) = (B64.decode(&stored.salt), B64.decode(&stored.hash)) else {
            return false;
        };
        if salt.is_empty() || expected.is_empty() {
            return false;
        }
        scrypt_bytes(password.as_bytes(), &salt, expected.len())
            .map(|actual| bool::from(actual.ct_eq(&expected)))
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// An ISO-8601 UTC timestamp, without pulling in a date crate.
fn iso_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a (year, month, day) in the proleptic Gregorian calendar.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

// ---------- Tailscale ----------

/// The fields of `tailscale status --json` this file reads, reduced to what the setup needs.
#[derive(Debug, Default, PartialEq)]
pub struct TailscaleFacts {
    pub state: String,
    pub running: bool,
    pub needs_login: bool,
    pub auth_url: Option<String>,
    pub dns_name: Option<String>,
    pub https: bool,
    pub login: Option<String>,
}

pub fn parse_tailscale_status(s: &Value) -> TailscaleFacts {
    let state = s["BackendState"].as_str().unwrap_or("Unknown").to_string();
    let me = &s["Self"];
    let caps_https = me["CapMap"]
        .as_object()
        .is_some_and(|c| c.contains_key("https"));
    let certs = s["CertDomains"].as_array().is_some_and(|c| !c.is_empty());
    let login = me["UserID"]
        .as_i64()
        .and_then(|id| s["User"].get(id.to_string()))
        .and_then(|u| u["LoginName"].as_str())
        .map(String::from);
    TailscaleFacts {
        running: state == "Running",
        needs_login: state == "NeedsLogin" || state == "NoState",
        auth_url: s["AuthURL"]
            .as_str()
            .filter(|u| !u.is_empty())
            .map(String::from),
        dns_name: me["DNSName"]
            .as_str()
            .map(|d| d.trim_end_matches('.').to_string())
            .filter(|d| !d.is_empty()),
        https: caps_https || certs,
        login,
        state,
    }
}

/// An HTTPS port already serving this listener, or the first one nothing else uses.
pub fn pick_serve_port(serve: &Value, local_url: &str) -> Option<(u16, bool)> {
    let mut used: Vec<u16> = serve["TCP"]
        .as_object()
        .into_iter()
        .flat_map(|t| t.keys())
        .filter_map(|k| k.parse().ok())
        .collect();
    for (host_port, conf) in serve["Web"].as_object().into_iter().flatten() {
        let Some(port) = port_of(host_port) else {
            continue;
        };
        used.push(port);
        if conf["Handlers"]["/"]["Proxy"].as_str() == Some(local_url) {
            return Some((port, true));
        }
    }
    WEB_PORTS
        .into_iter()
        .find(|p| !used.contains(p))
        .map(|p| (p, false))
}

/// What serves `port` on the tailnet, if anything.
pub fn serve_owner(serve: &Value, port: u16) -> Option<String> {
    serve["Web"]
        .as_object()?
        .iter()
        .find(|(host_port, _)| port_of(host_port) == Some(port))
        .and_then(|(_, conf)| conf["Handlers"]["/"]["Proxy"].as_str().map(String::from))
}

fn port_of(host_port: &str) -> Option<u16> {
    host_port.rsplit(':').next()?.parse().ok()
}

pub fn tailnet_url(dns_name: &str, port: u16) -> String {
    if port == 443 {
        format!("https://{dns_name}")
    } else {
        format!("https://{dns_name}:{port}")
    }
}

/// The `tailscale` to run: `TAILSCALE_PATH`, else one on the PATH, else the macOS app's binary.
pub fn find_tailscale() -> Option<String> {
    const MAC_APP: &str = "/Applications/Tailscale.app/Contents/MacOS/Tailscale";
    if let Some(path) = std::env::var("TAILSCALE_PATH")
        .ok()
        .filter(|p| !p.is_empty())
    {
        return Some(path);
    }
    if let Some(path) = find_on_path("tailscale") {
        return Some(path.to_string_lossy().into_owned());
    }
    (std::env::consts::OS == "macos" && Path::new(MAC_APP).is_file()).then(|| MAC_APP.to_string())
}

/// The result of `tailscale status`: whether it is installed, the facts, and an error if it failed.
#[derive(Debug, Default)]
pub struct TailscaleStatus {
    pub installed: bool,
    pub facts: TailscaleFacts,
    pub error: Option<String>,
}

#[derive(Debug)]
struct CmdError {
    stdout: String,
    stderr: String,
    message: String,
}

async fn run_cmd(bin: &str, args: &[&str], timeout: Duration) -> Result<String, CmdError> {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .env(
            "SHLVL",
            std::env::var("SHLVL")
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "1".into()),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    match tokio::time::timeout(timeout, cmd.output()).await {
        Err(_) => Err(CmdError {
            stdout: String::new(),
            stderr: String::new(),
            message: format!("{bin} timed out"),
        }),
        Ok(Err(e)) => Err(CmdError {
            stdout: String::new(),
            stderr: String::new(),
            message: e.to_string(),
        }),
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if out.status.success() {
                Ok(stdout)
            } else {
                let message = format!("{bin} exited with {}", out.status);
                Err(CmdError {
                    stdout,
                    stderr,
                    message,
                })
            }
        }
    }
}

/// `tailscale status`, as the app's setup needs it. Tailscale exits non-zero while logged out but
/// still prints the state, so that output counts.
pub async fn tailscale_status(bin: Option<&str>) -> TailscaleStatus {
    let Some(bin) = bin else {
        return TailscaleStatus::default();
    };
    match run_cmd(bin, &["status", "--json"], STATUS_TIMEOUT).await {
        Ok(out) => match serde_json::from_str::<Value>(&out) {
            Ok(v) => TailscaleStatus {
                installed: true,
                facts: parse_tailscale_status(&v),
                error: None,
            },
            Err(e) => not_running(e.to_string()),
        },
        Err(e) => {
            if let Ok(v) = serde_json::from_str::<Value>(&e.stdout) {
                return TailscaleStatus {
                    installed: true,
                    facts: parse_tailscale_status(&v),
                    error: None,
                };
            }
            let text = if e.stderr.trim().is_empty() {
                e.message
            } else {
                e.stderr
            };
            not_running(text.trim().chars().take(300).collect())
        }
    }
}

fn not_running(error: String) -> TailscaleStatus {
    TailscaleStatus {
        installed: true,
        facts: TailscaleFacts {
            state: "NotRunning".into(),
            ..Default::default()
        },
        error: Some(error),
    }
}

async fn serve_config(bin: &str) -> Value {
    match run_cmd(bin, &["serve", "status", "--json"], STATUS_TIMEOUT).await {
        Ok(out) if out.trim().is_empty() => json!({}),
        Ok(out) => serde_json::from_str(&out).unwrap_or_else(|_| json!({})),
        Err(_) => json!({}),
    }
}

// ---------- cloudflared ----------

struct CloudflaredAsset {
    url: String,
    sha256: &'static str,
    tgz: bool,
}

/// Node-style platform names (`darwin`, `linux`) and arch names (`arm64`, `x64`).
fn cloudflared_asset(platform: &str, arch: &str) -> Option<CloudflaredAsset> {
    let (file, sha256, tgz) = match (platform, arch) {
        ("darwin", "arm64") => (
            "cloudflared-darwin-arm64.tgz",
            "68b6bc4080ac1330234db2a9002d07f98b000537a1bd531a687b660d4160fa7f",
            true,
        ),
        ("darwin", "x64") => (
            "cloudflared-darwin-amd64.tgz",
            "f11fbef4207d715892331917dafd437f032cf4b7137ed64dea3008338c1c613d",
            true,
        ),
        ("linux", "arm64") => (
            "cloudflared-linux-arm64",
            "2b7203b4fb0ba28a0c1e12f8855ce2c25f76d266a8d5edafac5540b6812acff6",
            false,
        ),
        ("linux", "x64") => (
            "cloudflared-linux-amd64",
            "ae5e8add7884b0531d66bbca4570d1de491ff6da51e4ebe7eb160cdf241d9e42",
            false,
        ),
        _ => return None,
    };
    Some(CloudflaredAsset {
        url: format!("{CF_BASE}/{file}"),
        sha256,
        tgz,
    })
}

/// This machine's platform and arch in Node's names.
pub fn host_platform() -> (&'static str, &'static str) {
    let platform = match std::env::consts::OS {
        "macos" => "darwin",
        "linux" => "linux",
        _ => "other",
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => "other",
    };
    (platform, arch)
}

fn pinned_cloudflared(home: &Path, platform: &str, arch: &str) -> PathBuf {
    home.join("tools")
        .join("cloudflared")
        .join(CLOUDFLARED_VERSION)
        .join(format!("{platform}-{arch}"))
        .join("cloudflared")
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// The cloudflared to run: `CLOUDFLARED_PATH`, else the pinned copy, else one on the PATH.
pub fn find_cloudflared(home: &Path) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CLOUDFLARED_PATH").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let (platform, arch) = host_platform();
    let pinned = pinned_cloudflared(home, platform, arch);
    if pinned.is_file() {
        return Some(pinned);
    }
    find_on_path("cloudflared")
}

fn verify_sha256(bytes: &[u8], expected: &str) -> anyhow::Result<()> {
    let sum = hex::encode(Sha256::digest(bytes));
    if sum != expected {
        anyhow::bail!("cloudflared checksum mismatch: expected {expected}, got {sum}");
    }
    Ok(())
}

/// Download the pinned cloudflared, check its checksum, and install it under `home`.
pub async fn install_cloudflared(home: &Path) -> anyhow::Result<PathBuf> {
    let (platform, arch) = host_platform();
    let asset = cloudflared_asset(platform, arch).ok_or_else(|| {
        anyhow::anyhow!(
            "No cloudflared download for {platform}-{arch}. Install it yourself and try again."
        )
    })?;
    let target = pinned_cloudflared(home, platform, arch);
    let res = reqwest::get(&asset.url).await?;
    if !res.status().is_success() {
        anyhow::bail!(
            "Couldn't download cloudflared (HTTP {})",
            res.status().as_u16()
        );
    }
    let bytes = res.bytes().await?;
    verify_sha256(&bytes, asset.sha256)?;
    let dir = target.parent().unwrap_or(home).to_path_buf();
    tokio::fs::create_dir_all(&dir).await?;
    if asset.tgz {
        let tgz = target.with_extension("tgz");
        tokio::fs::write(&tgz, &bytes).await?;
        let status = Command::new("tar")
            .arg("-xzf")
            .arg(&tgz)
            .arg("-C")
            .arg(&dir)
            .status()
            .await?;
        tokio::fs::remove_file(&tgz).await?;
        if !status.success() {
            anyhow::bail!("Couldn't unpack cloudflared");
        }
    } else {
        tokio::fs::write(&target, &bytes).await?;
    }
    set_executable(&target)?;
    Ok(target)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// The first `https://<name>.trycloudflare.com` in the text.
pub fn find_quick_url(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(i) = rest.find("https://") {
        let after = &rest[i + "https://".len()..];
        let host_len = after
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
            .unwrap_or(after.len());
        let host = &after[..host_len];
        if !host.is_empty() && after[host_len..].starts_with(".trycloudflare.com") {
            return Some(format!("https://{host}.trycloudflare.com"));
        }
        rest = after;
    }
    None
}

/// A running quick tunnel: its link, and the task that owns the process.
struct Running {
    id: u64,
    url: String,
    kill: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

/// Start a quick tunnel to `local_url`. Resolves once cloudflared has a link and a registered edge
/// connection: it prints the link first, and the link doesn't work until a connection is up.
/// HTTP/2, like T3 Code's tunnels, because many networks block QUIC on port 7844.
async fn start_quick_tunnel(
    bin: &Path,
    local_url: &str,
    timeout: Duration,
) -> anyhow::Result<(String, Child)> {
    let mut child = Command::new(bin)
        .args([
            "tunnel",
            "--no-autoupdate",
            "--protocol",
            "http2",
            "--url",
            local_url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(line);
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(line);
            }
        });
    }
    drop(tx);
    let mut tail = String::new();
    let mut url: Option<String> = None;
    let mut registered = false;
    let ready = tokio::time::timeout(timeout, async {
        while let Some(line) = rx.recv().await {
            tail.push_str(&line);
            tail.push('\n');
            if tail.len() > 4000 {
                let cut = tail.len() - 4000;
                tail = tail[cut..].to_string();
            }
            url = url.or_else(|| find_quick_url(&tail));
            registered |= tail.contains("Registered tunnel connection");
            if let (Some(u), true) = (&url, registered) {
                return Some(u.clone());
            }
        }
        None
    })
    .await;
    match ready {
        Ok(Some(found)) => Ok((found, child)),
        Ok(None) => {
            let code = child
                .wait()
                .await
                .ok()
                .and_then(|s| s.code())
                .map_or("null".into(), |c| c.to_string());
            let last: Vec<&str> = tail.trim().lines().rev().take(3).collect();
            let last: Vec<&str> = last.into_iter().rev().collect();
            anyhow::bail!("cloudflared exited ({code}): {}", last.join(" "))
        }
        Err(_) => {
            let _ = child.kill().await;
            anyhow::bail!("cloudflared didn't hand out a link in time. Try again.")
        }
    }
}

// ---------- the gateway's side ----------

#[derive(Clone, Debug)]
pub struct RemoteConfig {
    pub home: PathBuf,
    pub store_file: PathBuf,
    /// Where the tunnel's link is kept for the `bots` CLI to read.
    pub url_file: PathBuf,
    pub local_port: u16,
    pub env_password: String,
    pub env_tailnet: bool,
    pub env_tailnet_users: Vec<String>,
    /// The `tailscale` binary, or `None` when it isn't installed.
    pub tailscale_bin: Option<String>,
}

pub struct RemoteAccess {
    cfg: RemoteConfig,
    tailnet: Arc<Tailnet>,
    /// One change at a time: two quick clicks must not start two tunnels, or drop the password mid-start.
    queue: AsyncMutex<()>,
    tunnel: Mutex<Option<Running>>,
    tunnel_error: Mutex<Option<String>>,
    next_id: AtomicU64,
}

impl RemoteAccess {
    pub fn new(cfg: RemoteConfig, tailnet: Arc<Tailnet>) -> Self {
        Self {
            cfg,
            tailnet,
            queue: AsyncMutex::new(()),
            tunnel: Mutex::new(None),
            tunnel_error: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    fn saved(&self) -> AccessState {
        read_state(&self.cfg.store_file)
    }

    /// Change the saved choices under the queue lock, so no two changes interleave.
    fn update(&self, change: impl FnOnce(&mut AccessState)) -> Result<(), AccessError> {
        let mut state = self.saved();
        change(&mut state);
        write_state(&self.cfg.store_file, &state).map_err(AccessError::internal)
    }

    /// The port Tailscale's listener is on: the open one, else the one it would open.
    fn tailnet_port(&self) -> u16 {
        match self.tailnet.port() {
            0 => self.cfg.local_port.saturating_add(1),
            open => open,
        }
    }

    fn local_tunnel_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.cfg.local_port)
    }

    fn tailnet_listener_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.tailnet_port())
    }

    pub fn password_from_env(&self) -> bool {
        !self.cfg.env_password.is_empty()
    }

    /// Whether the app asks for a password: BOT_PASSWORD, or one set in Settings.
    pub fn password_required(&self) -> bool {
        self.password_from_env() || self.saved().password.is_some()
    }

    /// The password check for sign-in: BOT_PASSWORD if it is set, else the saved hash.
    pub async fn password_matches(&self, given: &str) -> bool {
        if self.password_from_env() {
            return bool::from(given.as_bytes().ct_eq(self.cfg.env_password.as_bytes()));
        }
        match self.saved().password {
            Some(stored) => verify_password(given.to_string(), stored).await,
            None => false,
        }
    }

    fn running_url(&self) -> Option<String> {
        self.tunnel.lock().unwrap().as_ref().map(|r| r.url.clone())
    }

    /// Everything the Settings screen shows about remote access.
    pub async fn status(&self) -> Value {
        let saved = self.saved();
        let bin = self.cfg.tailscale_bin.as_deref();
        let ts = tailscale_status(bin).await;
        let mut tailscale = Map::new();
        tailscale.insert("installed".into(), json!(ts.installed));
        if ts.installed {
            let f = &ts.facts;
            tailscale.insert("state".into(), json!(f.state));
            tailscale.insert("running".into(), json!(f.running));
            tailscale.insert("needsLogin".into(), json!(f.needs_login));
            tailscale.insert("dnsName".into(), json!(f.dns_name));
            tailscale.insert("https".into(), json!(f.https));
            tailscale.insert("login".into(), json!(f.login));
        }
        if let Some(error) = &ts.error {
            tailscale.insert("error".into(), json!(error));
        }
        tailscale.insert("authUrl".into(), json!(ts.facts.auth_url));
        let enabled = saved.tailscale.as_ref().is_some_and(|t| t.enabled);
        tailscale.insert("enabled".into(), json!(enabled));
        tailscale.insert("fromEnv".into(), json!(self.cfg.env_tailnet));
        let users = saved
            .tailscale
            .as_ref()
            .and_then(|t| t.users.clone())
            .unwrap_or_else(|| self.cfg.env_tailnet_users.clone());
        tailscale.insert("users".into(), json!(users));
        if let (true, Some(dns), Some(https_port), Some(bin)) = (
            ts.installed && ts.facts.running && enabled,
            ts.facts.dns_name.as_deref(),
            saved.tailscale.as_ref().and_then(|t| t.https_port),
            bin,
        ) {
            let owner = serve_owner(&serve_config(bin).await, https_port);
            tailscale.insert("url".into(), json!(tailnet_url(dns, https_port)));
            tailscale.insert(
                "serving".into(),
                json!(owner.as_deref() == Some(self.tailnet_listener_url().as_str())),
            );
        }
        json!({
            "password": { "set": self.password_required(), "fromEnv": self.password_from_env() },
            "tailscale": Value::Object(tailscale),
            "tunnel": {
                "enabled": saved.tunnel.as_ref().is_some_and(|t| t.enabled),
                "url": self.running_url(),
                "error": self.tunnel_error.lock().unwrap().clone(),
                "available": find_cloudflared(&self.cfg.home).is_some(),
            },
        })
    }

    /// Set the password (`Some`) or remove it (`None`). The caller rotates the session secret.
    pub async fn set_password(&self, password: Option<String>) -> Result<(), AccessError> {
        let _turn = self.queue.lock().await;
        if self.password_from_env() {
            return Err(AccessError::conflict(
                "The password is set by BOT_PASSWORD in .env.",
            ));
        }
        let Some(password) = password else {
            if self.saved().tunnel.is_some_and(|t| t.enabled) {
                return Err(AccessError::conflict(
                    "Turn off the Cloudflare link before removing the password.",
                ));
            }
            return self.update(|s| s.password = None);
        };
        if password.chars().count() < MIN_PASSWORD {
            return Err(AccessError::bad_request(format!(
                "Use at least {MIN_PASSWORD} characters."
            )));
        }
        let stored = hash_password(password)
            .await
            .map_err(AccessError::internal)?;
        self.update(|s| s.password = Some(stored))
    }

    /// Turn on the Tailscale route: a listener for the signed-in user, then Serve on a free HTTPS port.
    /// `router` is the app as the tailnet listener serves it.
    pub async fn tailscale_on(&self, router: Router) -> Result<Value, AccessError> {
        let _turn = self.queue.lock().await;
        self.tailscale_on_locked(router).await
    }

    async fn tailscale_on_locked(&self, router: Router) -> Result<Value, AccessError> {
        if self.cfg.env_tailnet {
            return Err(AccessError::conflict(
                "Tailscale is set up in .env (TAILNET_PORT).",
            ));
        }
        let ts = tailscale_status(self.cfg.tailscale_bin.as_deref()).await;
        let Some(bin) = self.cfg.tailscale_bin.clone().filter(|_| ts.installed) else {
            return Err(AccessError::coded(
                "not-installed",
                "Install Tailscale on this machine first.",
            ));
        };
        let f = ts.facts;
        if !f.running {
            return Err(if f.needs_login {
                AccessError::coded("needs-login", "Sign in to Tailscale on this machine first.")
            } else {
                AccessError::coded("not-running", "Start Tailscale on this machine first.")
            });
        }
        if !f.https {
            return Err(AccessError::coded(
                "https-off",
                "Turn on HTTPS certificates for your tailnet first.",
            ));
        }
        let Some(login) = f.login.clone() else {
            return Err(AccessError::conflict(
                "Couldn't tell which Tailscale user this machine belongs to.",
            ));
        };
        let Some(dns) = f.dns_name.clone() else {
            return Err(AccessError::conflict(
                "Couldn't tell this machine's Tailscale name.",
            ));
        };
        let port = self.tailnet_port();
        let local_url = self.tailnet_listener_url();
        let pick = pick_serve_port(&serve_config(&bin).await, &local_url).ok_or_else(|| {
            AccessError::conflict(
                "Every HTTPS port Bots tries is taken by other Tailscale Serve routes.",
            )
        })?;
        let users = vec![login.to_lowercase()];
        self.tailnet
            .open(port, &users, router)
            .await
            .map_err(AccessError::internal)?;
        if !pick.1 {
            let https = pick.0.to_string();
            run_cmd(
                &bin,
                &["serve", "--bg", &format!("--https={https}"), &local_url],
                CMD_TIMEOUT,
            )
            .await
            .map_err(|e| AccessError::internal(e.message))?;
        }
        let (https_port, users) = (pick.0, users);
        self.update(|s| {
            s.tailscale = Some(TailscaleSaved {
                enabled: true,
                port: Some(port),
                https_port: Some(https_port),
                users: Some(users),
            });
        })?;
        Ok(json!({ "url": tailnet_url(&dns, https_port) }))
    }

    pub async fn tailscale_off(&self) -> Result<(), AccessError> {
        let _turn = self.queue.lock().await;
        let saved = self.saved().tailscale.unwrap_or_default();
        if let (Some(bin), Some(https_port), Some(port)) = (
            self.cfg.tailscale_bin.as_deref(),
            saved.https_port,
            saved.port,
        ) {
            let owner = serve_owner(&serve_config(bin).await, https_port);
            if owner.as_deref() == Some(format!("http://127.0.0.1:{port}").as_str()) {
                let https = https_port.to_string();
                let _ = run_cmd(
                    bin,
                    &["serve", &format!("--https={https}"), "--set-path=/", "off"],
                    CMD_TIMEOUT,
                )
                .await;
            }
        }
        if !self.cfg.env_tailnet {
            self.tailnet.close().await;
        }
        self.update(|s| {
            s.tailscale = Some(TailscaleSaved {
                enabled: false,
                ..saved
            })
        })
    }

    /// Start the Cloudflare quick tunnel. It is public, so it needs a password first.
    pub async fn tunnel_on(self: &Arc<Self>) -> Result<Value, AccessError> {
        let _turn = self.queue.lock().await;
        self.tunnel_on_locked().await
    }

    async fn tunnel_on_locked(self: &Arc<Self>) -> Result<Value, AccessError> {
        if !self.password_required() {
            return Err(AccessError::coded(
                "needs-password",
                "A Cloudflare link is public. Set a password first.",
            ));
        }
        if let Some(url) = self.running_url() {
            return Ok(json!({ "url": url }));
        }
        let bin = match find_cloudflared(&self.cfg.home) {
            Some(bin) => bin,
            None => install_cloudflared(&self.cfg.home)
                .await
                .map_err(AccessError::internal)?,
        };
        *self.tunnel_error.lock().unwrap() = None;
        let (url, child) = start_quick_tunnel(&bin, &self.local_tunnel_url(), TUNNEL_START_TIMEOUT)
            .await
            .map_err(AccessError::internal)?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (kill, kill_rx) = oneshot::channel();
        let task = tokio::spawn(Arc::clone(self).watch_tunnel(id, child, kill_rx));
        *self.tunnel.lock().unwrap() = Some(Running {
            id,
            url: url.clone(),
            kill: Some(kill),
            task,
        });
        self.update(|s| s.tunnel = Some(TunnelSaved { enabled: true }))?;
        // `bots tunnel status` reads the link from this file.
        if let Some(dir) = self.cfg.url_file.parent() {
            let _ = tokio::fs::create_dir_all(dir).await;
        }
        tokio::fs::write(&self.cfg.url_file, format!("{url}\n"))
            .await
            .map_err(AccessError::internal)?;
        tracing::info!("cloudflare tunnel at {url}");
        Ok(json!({ "url": url }))
    }

    /// Owns the tunnel's process: reports it if it stops without being asked, and stops it when asked.
    async fn watch_tunnel(
        self: Arc<Self>,
        id: u64,
        mut child: Child,
        kill_rx: oneshot::Receiver<()>,
    ) {
        tokio::select! {
            status = child.wait() => {
                let code = status.ok().and_then(|s| s.code()).map_or("null".into(), |c| c.to_string());
                {
                    let mut tunnel = self.tunnel.lock().unwrap();
                    if tunnel.as_ref().map(|r| r.id) != Some(id) {
                        return;
                    }
                    *tunnel = None;
                }
                let _ = tokio::fs::remove_file(&self.cfg.url_file).await;
                *self.tunnel_error.lock().unwrap() = Some(format!("cloudflared stopped ({code})."));
                tracing::warn!("cloudflare tunnel stopped ({code})");
            }
            _ = kill_rx => {
                let _ = child.kill().await;
            }
        }
    }

    pub async fn tunnel_off(&self) -> Result<(), AccessError> {
        let _turn = self.queue.lock().await;
        self.stop_tunnel().await;
        let _ = tokio::fs::remove_file(&self.cfg.url_file).await;
        self.update(|s| s.tunnel = Some(TunnelSaved { enabled: false }))
    }

    /// Take the running tunnel down and wait for its process to end.
    async fn stop_tunnel(&self) {
        let running = self.tunnel.lock().unwrap().take();
        if let Some(mut running) = running {
            if let Some(kill) = running.kill.take() {
                let _ = kill.send(());
            }
            let _ = tokio::time::timeout(TUNNEL_STOP_TIMEOUT, running.task).await;
        }
    }

    /// On start: bring back what was on. A failure is logged and shown in Settings, never fatal.
    /// `router` is the app as the tailnet listener serves it.
    pub async fn boot(self: &Arc<Self>, router: Router) {
        let _turn = self.queue.lock().await;
        let saved = self.saved();
        if let Some(ts) = saved.tailscale.as_ref().filter(|t| t.enabled) {
            if !self.cfg.env_tailnet {
                let users = ts.users.clone().unwrap_or_default();
                if let Err(e) = self
                    .tailnet
                    .open(ts.port.unwrap_or(0), &users, router)
                    .await
                {
                    tracing::error!("tailnet listener failed: {e:#}");
                }
            }
        }
        if saved.tunnel.as_ref().is_some_and(|t| t.enabled) {
            if let Err(e) = self.tunnel_on_locked().await {
                *self.tunnel_error.lock().unwrap() = Some(e.message.clone());
                tracing::error!("cloudflare tunnel failed: {}", e.message);
            }
        }
    }

    /// On shutdown: stop the tunnel so cloudflared doesn't outlive the gateway.
    pub async fn stop(&self) {
        self.stop_tunnel().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> Value {
        json!({
            "BackendState": "Running",
            "Self": { "DNSName": "box.tail1.ts.net.", "UserID": 7, "CapMap": { "https": null } },
            "User": { "7": { "LoginName": "Me@Example.com" } },
        })
    }

    #[test]
    fn tailscale_status_becomes_the_few_facts_the_setup_needs() {
        let f = parse_tailscale_status(&running());
        assert_eq!(
            f,
            TailscaleFacts {
                state: "Running".into(),
                running: true,
                needs_login: false,
                auth_url: None,
                dns_name: Some("box.tail1.ts.net".into()),
                https: true,
                login: Some("Me@Example.com".into()),
            }
        );
        let login = parse_tailscale_status(
            &json!({ "BackendState": "NeedsLogin", "AuthURL": "https://login.tailscale.com/a/x", "Self": {} }),
        );
        assert!(login.needs_login);
        assert_eq!(
            login.auth_url.as_deref(),
            Some("https://login.tailscale.com/a/x")
        );
        assert!(
            !parse_tailscale_status(
                &json!({ "BackendState": "Running", "Self": { "CapMap": {} }, "CertDomains": [] })
            )
            .https
        );
        assert!(
            parse_tailscale_status(
                &json!({ "BackendState": "Running", "CertDomains": ["box.ts.net"] })
            )
            .https
        );
    }

    #[test]
    fn serve_gets_a_free_https_port_and_reuses_a_route_to_this_listener() {
        let serve = json!({
            "TCP": { "443": { "HTTPS": true }, "8443": { "HTTPS": true } },
            "Web": {
                "box:443": { "Handlers": { "/": { "Proxy": "http://127.0.0.1:8770" } } },
                "box:8443": { "Handlers": { "/": { "Proxy": "http://127.0.0.1:3773" } } },
            },
        });
        assert_eq!(
            pick_serve_port(&serve, "http://127.0.0.1:3781"),
            Some((8444, false))
        );
        assert_eq!(
            pick_serve_port(&json!({}), "http://127.0.0.1:3781"),
            Some((443, false))
        );
        let mut mine = serve.clone();
        mine["Web"]["box:8444"] =
            json!({ "Handlers": { "/": { "Proxy": "http://127.0.0.1:3781" } } });
        assert_eq!(
            pick_serve_port(&mine, "http://127.0.0.1:3781"),
            Some((8444, true))
        );
        assert_eq!(
            serve_owner(&mine, 8443).as_deref(),
            Some("http://127.0.0.1:3773")
        );
        assert_eq!(tailnet_url("box.ts.net", 443), "https://box.ts.net");
        assert_eq!(tailnet_url("box.ts.net", 8444), "https://box.ts.net:8444");
        let all_taken = json!({ "Web": { "box:443": {}, "box:8443": {}, "box:8444": {}, "box:8445": {}, "box:9443": {} } });
        assert_eq!(pick_serve_port(&all_taken, "http://127.0.0.1:3781"), None);
    }

    #[tokio::test]
    async fn passwords_are_stored_as_scrypt_hashes_and_checked_against_them() {
        let stored = hash_password("correct horse".into()).await.unwrap();
        assert!(!serde_json::to_string(&stored)
            .unwrap()
            .contains("correct horse"));
        assert!(verify_password("correct horse".into(), stored.clone()).await);
        assert!(!verify_password("correct hors".into(), stored).await);
    }

    #[test]
    fn set_at_is_an_iso_timestamp() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        let now = iso_now();
        assert!(
            now.len() == 24 && now.ends_with('Z') && now.as_bytes()[10] == b'T',
            "{now}"
        );
    }

    #[test]
    fn the_quick_link_is_found_in_cloudflared_output() {
        assert_eq!(
            find_quick_url("INF |  https://brave-otter-12.trycloudflare.com  |").as_deref(),
            Some("https://brave-otter-12.trycloudflare.com")
        );
        assert_eq!(find_quick_url("https://example.com and nothing else"), None);
        assert_eq!(
            find_quick_url("https://Bad-Case.trycloudflare.com").as_deref(),
            None
        );
    }

    #[test]
    fn the_pinned_cloudflared_assets_are_the_ones_the_node_gateway_pins() {
        let linux = cloudflared_asset("linux", "x64").unwrap();
        assert!(!linux.tgz);
        assert_eq!(
            linux.sha256,
            "ae5e8add7884b0531d66bbca4570d1de491ff6da51e4ebe7eb160cdf241d9e42"
        );
        assert!(linux.url.ends_with("/cloudflared-linux-amd64"));
        assert!(cloudflared_asset("darwin", "arm64").unwrap().tgz);
        assert!(cloudflared_asset("windows", "x64").is_none());
    }

    #[test]
    fn a_download_that_doesnt_match_its_checksum_is_refused() {
        let sum = hex::encode(Sha256::digest(b"tool"));
        assert!(verify_sha256(b"tool", &sum).is_ok());
        let err = verify_sha256(b"other", &sum).unwrap_err().to_string();
        assert!(err.contains("checksum mismatch"), "{err}");
    }

    /// An access.json written by Node (`gateway/tests/fixtures/t4/access.json`) is read and checked here.
    #[tokio::test]
    async fn a_node_written_access_json_verifies_in_rust() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("access.json");
        std::fs::write(&store, include_str!("../tests/fixtures/t4/access.json")).unwrap();
        let remote = RemoteAccess::new(
            RemoteConfig {
                home: dir.path().to_path_buf(),
                store_file: store,
                url_file: dir.path().join("tunnel-url"),
                local_port: 3780,
                env_password: String::new(),
                env_tailnet: false,
                env_tailnet_users: vec![],
                tailscale_bin: None,
            },
            Arc::new(Tailnet::new()),
        );
        assert!(remote.password_required());
        assert!(remote.password_matches("correct horse").await);
        assert!(!remote.password_matches("correct hors").await);
        let status = remote.status().await;
        assert_eq!(status["password"]["set"], true);
        assert_eq!(status["tailscale"]["users"], json!(["me@example.com"]));
        assert_eq!(status["tailscale"]["enabled"], true);
    }

    #[test]
    fn the_saved_file_keeps_the_node_gateways_field_names() {
        let state = AccessState {
            password: Some(StoredPassword {
                salt: "s".into(),
                hash: "h".into(),
                set_at: "t".into(),
            }),
            tailscale: Some(TailscaleSaved {
                enabled: true,
                port: Some(3781),
                https_port: Some(443),
                users: Some(vec!["me@example.com".into()]),
            }),
            tunnel: Some(TunnelSaved { enabled: false }),
        };
        let text = serde_json::to_value(&state).unwrap();
        assert_eq!(text["password"]["setAt"], "t");
        assert_eq!(text["tailscale"]["httpsPort"], 443);
        assert_eq!(text["tunnel"]["enabled"], false);
        // A file written by the Node gateway reads back the same way.
        let node_written = r#"{"password":{"salt":"s","hash":"h","setAt":"t"},"tailscale":{"enabled":false,"port":3781,"httpsPort":443,"users":["me@example.com"]},"tunnel":{"enabled":true}}"#;
        let read: AccessState = serde_json::from_str(node_written).unwrap();
        assert!(read.tunnel.unwrap().enabled);
        assert_eq!(
            read.tailscale.unwrap().users.unwrap(),
            vec!["me@example.com"]
        );
    }
}
