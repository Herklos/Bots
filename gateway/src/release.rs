//! Installed `bots` releases, self-update and the automatic update (Threebot's `release.mts`,
//! `auto-update.mts` and the update checker in `release.mts`).
//!
//! Releases are GitHub release assets: `bots-<version>-<target>.tar.gz` holding one `bots` binary,
//! checked against the release's `SHA256SUMS`. An installed copy lives in
//! `$THREEBOT_HOME/runtime/versions/<version>/bots`. `runtime/current` links to the version the
//! service runs, and `runtime/previous` to the one before it, kept for a rollback. An update only
//! swaps those links and restarts the service; if the new version doesn't answer `/api/health`,
//! the old one is switched back.
use crate::{error::ApiResult, AppState};
use anyhow::{anyhow, bail, Context, Result};
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::HashMap,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::fs::{symlink, PermissionsExt},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// The release repository, as Threebot's. Override the list with THREEBOT_RELEASES_URL.
pub const REPO: &str = "RokaSwK/threebot";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const USER_AGENT_NAME: &str = "bots-updater";
/// How long a successful check is kept before the next one.
const CHECK_TTL: Duration = Duration::from_secs(15 * 60);
/// A failed check waits longer each time, up to this.
const MAX_BACKOFF: Duration = Duration::from_secs(2 * 60 * 60);
/// An update that hasn't restarted us by then failed (systemd can't tell us the updater exited).
pub const PENDING_MS: u64 = 30 * 60 * 1000;
/// How long a release waits for Bots to go quiet before the automatic update gives up on it.
pub const MAX_WAIT_MS: u64 = 24 * 60 * 60 * 1000;

// ---------- paths ----------

/// `THREEBOT_HOME`, or `~/.threebot`, as the Node gateway and the Rust gateway both read it.
pub fn home_dir() -> PathBuf {
    match std::env::var("THREEBOT_HOME") {
        Ok(h) if !h.is_empty() => PathBuf::from(h),
        _ => PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".threebot"),
    }
}

/// Where installed versions and their links live.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimePaths {
    pub versions: PathBuf,
    pub current: PathBuf,
    pub previous: PathBuf,
}

impl RuntimePaths {
    pub fn new(home: &Path) -> Self {
        let runtime = home.join("runtime");
        Self {
            versions: runtime.join("versions"),
            current: runtime.join("current"),
            previous: runtime.join("previous"),
        }
    }
}

/// True when `exe` runs from an installed version folder, so the copy can update itself.
pub fn installed_runtime(exe: &Path, home: &Path) -> bool {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let Some(version_dir) = real(exe).parent().map(Path::to_path_buf) else {
        return false;
    };
    version_dir.parent().map(Path::to_path_buf) == Some(real(&RuntimePaths::new(home).versions))
}

/// The version folder's `bots` binary, through the `current` link, so the service follows updates.
pub fn current_binary(home: &Path) -> PathBuf {
    RuntimePaths::new(home).current.join("bots")
}

// ---------- versions ----------

struct Semver<'a> {
    core: [u64; 3],
    pre: Option<&'a str>,
}

fn number(s: &str) -> Option<u64> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

fn parse_version(v: &str) -> Option<Semver<'_>> {
    let v = v.strip_prefix('v').unwrap_or(v);
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) if !pre.is_empty() => (core, Some(pre)),
        Some(_) => return None,
        None => (v, None),
    };
    let mut parts = core.split('.');
    let mut nums = [0u64; 3];
    for n in nums.iter_mut() {
        *n = number(parts.next()?)?;
    }
    parts.next().is_none().then_some(Semver { core: nums, pre })
}

/// Pre-release identifiers: numbers compare as numbers and sort before text.
fn compare_pre(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.split('.'), b.split('.'));
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                let order = match (number(p), number(q)) {
                    (Some(m), Some(n)) => m.cmp(&n),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => p.cmp(q),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// Compare two `X.Y.Z[-pre]` versions. A release comes after its own pre-releases.
pub fn compare_versions(a: &str, b: &str) -> Result<Ordering> {
    let x = parse_version(a).ok_or_else(|| anyhow!("Not a version: {a}"))?;
    let y = parse_version(b).ok_or_else(|| anyhow!("Not a version: {b}"))?;
    Ok(match x.core.cmp(&y.core) {
        Ordering::Equal => match (x.pre, y.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(p), Some(q)) => compare_pre(p, q),
        },
        other => other,
    })
}

/// Previews are versions with a pre-release tag; they follow the preview channel.
pub fn is_preview(version: &str) -> bool {
    version.contains('-')
}

/// `<arch>-<os>`, for example `x86_64-linux` or `aarch64-macos`.
pub fn target() -> String {
    format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
}

pub fn asset_name(version: &str, target: &str) -> String {
    format!("bots-{version}-{target}.tar.gz")
}

/// The hash for `name` in a `SHA256SUMS` file (`<sha256>  <name>`, `*` before a binary name allowed).
pub fn expected_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let hash = fields.next()?;
        let file = fields.next()?;
        let file = file.strip_prefix('*').unwrap_or(file);
        (file == name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

// ---------- finding a release ----------

/// A release to install: its version, where to download the archive and the sums, and what GitHub says.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub archive: String,
    pub sums: String,
    pub notes: String,
    pub url: String,
    pub published_at: Option<String>,
}

/// Where releases come from: the GitHub API list (or a mirror of it) and, optionally, a download base.
#[derive(Clone, Debug)]
pub struct ReleaseSource {
    pub api: String,
    pub base: Option<String>,
}

impl ReleaseSource {
    pub fn from_env() -> Self {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Self {
            api: get("THREEBOT_RELEASES_URL").unwrap_or_else(|| {
                format!("https://api.github.com/repos/{REPO}/releases?per_page=30")
            }),
            base: get("THREEBOT_RELEASE_BASE_URL").map(|b| b.trim_end_matches('/').to_string()),
        }
    }
}

fn tag_of(release: &Value) -> &str {
    release["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v')
}

/// Pick the release from a GitHub-API-shaped list: an exact `version`, or the newest one.
pub fn pick_release(
    releases: &[Value],
    version: Option<&str>,
    preview: bool,
    base: Option<&str>,
    target: &str,
) -> Option<Release> {
    let candidates = releases.iter().filter(|r| {
        !r["draft"].as_bool().unwrap_or(false)
            && (preview || !r["prerelease"].as_bool().unwrap_or(false))
    });
    let wanted = match version {
        Some(v) => {
            let v = v.trim_start_matches('v');
            candidates.into_iter().find(|r| tag_of(r) == v)?
        }
        None => candidates
            .filter(|r| parse_version(tag_of(r)).is_some())
            .max_by(|a, b| compare_versions(tag_of(a), tag_of(b)).unwrap_or(Ordering::Equal))?,
    };
    let v = tag_of(wanted).to_string();
    let name = asset_name(&v, target);
    let (archive, sums) = match base {
        Some(base) => (
            format!("{base}/v{v}/{name}"),
            format!("{base}/v{v}/SHA256SUMS"),
        ),
        None => {
            let find = |file: &str| -> Option<String> {
                wanted["assets"]
                    .as_array()?
                    .iter()
                    .find(|a| a["name"] == file)?["browser_download_url"]
                    .as_str()
                    .map(String::from)
            };
            (find(&name)?, find("SHA256SUMS")?)
        }
    };
    Some(Release {
        url: wanted["html_url"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("https://github.com/{REPO}/releases/tag/v{v}")),
        notes: wanted["body"].as_str().unwrap_or("").to_string(),
        published_at: wanted["published_at"].as_str().map(String::from),
        version: v,
        archive,
        sums,
    })
}

pub fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(USER_AGENT_NAME)
        .timeout(Duration::from_secs(300))
        .build()?)
}

pub async fn find_release(
    client: &reqwest::Client,
    source: &ReleaseSource,
    version: Option<&str>,
    preview: bool,
) -> Result<Option<Release>> {
    let res = client
        .get(&source.api)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await?;
    if !res.status().is_success() {
        bail!("Couldn't list releases (HTTP {})", res.status().as_u16());
    }
    let releases: Vec<Value> = res.json().await?;
    Ok(pick_release(
        &releases,
        version,
        preview,
        source.base.as_deref(),
        &target(),
    ))
}

// ---------- installing ----------

async fn download(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let res = client.get(url).send().await?;
    if !res.status().is_success() {
        bail!("Download failed: {url} (HTTP {})", res.status().as_u16());
    }
    Ok(res.bytes().await?.to_vec())
}

/// Unpack the archive into `dir` and check that its `bots` reports `version`.
fn unpack_and_check(bytes: &[u8], dir: &Path, version: &str) -> Result<()> {
    let archive = dir.join(".archive.tar.gz");
    std::fs::write(&archive, bytes)?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(dir)
        .status()
        .context("running tar")?;
    std::fs::remove_file(&archive).ok();
    if !status.success() {
        bail!("tar couldn't unpack the release");
    }
    let binary = dir.join("bots");
    if !binary.is_file() {
        bail!("The archive has no bots binary");
    }
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))?;
    let out = Command::new(&binary).arg("--version").output()?;
    let got = String::from_utf8_lossy(&out.stdout)
        .trim()
        .trim_start_matches("bots ")
        .to_string();
    if got != version {
        bail!("The archive holds version {got}, not {version}");
    }
    Ok(())
}

/// Download, verify and unpack a release into `versions/<version>`. Returns that folder.
pub async fn install_release(
    client: &reqwest::Client,
    home: &Path,
    release: &Release,
    log: &dyn Fn(&str),
) -> Result<PathBuf> {
    let paths = RuntimePaths::new(home);
    let target_dir = paths.versions.join(&release.version);
    if target_dir.join(".install-complete").exists() {
        return Ok(target_dir);
    }
    std::fs::create_dir_all(&paths.versions)?;
    log(&format!("Downloading Bots {}", release.version));
    let archive = download(client, &release.archive).await?;
    let sums = download(client, &release.sums).await?;
    let name = asset_name(&release.version, &target());
    let expected = expected_sum(&String::from_utf8_lossy(&sums), &name)
        .ok_or_else(|| anyhow!("SHA256SUMS has no entry for {name}"))?;
    let actual = hex::encode(Sha256::digest(&archive));
    if actual != expected {
        bail!("Checksum mismatch for {name}: expected {expected}, got {actual}");
    }
    log(&format!("Checksum OK ({}…)", &actual[..12]));
    let staging = paths
        .versions
        .join(format!(".staging-{}", std::process::id()));
    std::fs::remove_dir_all(&staging).ok();
    std::fs::create_dir_all(&staging)?;
    if let Err(error) = unpack_and_check(&archive, &staging, &release.version) {
        std::fs::remove_dir_all(&staging).ok();
        return Err(error);
    }
    std::fs::write(
        staging.join(".install-complete"),
        format!("{}\n", iso8601(unix_now())),
    )?;
    std::fs::remove_dir_all(&target_dir).ok();
    std::fs::rename(&staging, &target_dir)?;
    Ok(target_dir)
}

// ---------- links ----------

fn swap_link(link: &Path, target: &Path) -> Result<()> {
    // Rename over a link is atomic, so `current` is never missing.
    let temp = PathBuf::from(format!("{}.{}.tmp", link.display(), std::process::id()));
    std::fs::remove_file(&temp).ok();
    symlink(target, &temp)?;
    std::fs::rename(&temp, link)?;
    Ok(())
}

/// Where a link points, as an absolute path.
pub fn read_link(link: &Path) -> Option<PathBuf> {
    let target = std::fs::read_link(link).ok()?;
    Some(if target.is_absolute() {
        target
    } else {
        link.parent()?.join(target)
    })
}

/// Point `current` at `version_dir`, remembering the version it replaces. Returns that one.
pub fn activate(version_dir: &Path, home: &Path) -> Result<Option<PathBuf>> {
    let paths = RuntimePaths::new(home);
    std::fs::create_dir_all(paths.current.parent().unwrap_or(home))?;
    let before = read_link(&paths.current);
    if let Some(before) = &before {
        if before != version_dir {
            swap_link(&paths.previous, before)?;
        }
    }
    swap_link(&paths.current, version_dir)?;
    Ok(before)
}

/// Switch `current` back to the version before the last `activate`.
pub fn rollback(home: &Path) -> Result<PathBuf> {
    let paths = RuntimePaths::new(home);
    let previous = read_link(&paths.previous)
        .filter(|p| p.exists())
        .ok_or_else(|| anyhow!("There is no earlier version to go back to."))?;
    swap_link(&paths.current, &previous)?;
    std::fs::remove_file(&paths.previous).ok();
    Ok(previous)
}

/// Delete every installed version except `current` and `previous`. Returns the names removed.
pub fn prune(home: &Path) -> Result<Vec<String>> {
    let paths = RuntimePaths::new(home);
    let keep: Vec<PathBuf> = [read_link(&paths.current), read_link(&paths.previous)]
        .into_iter()
        .flatten()
        .collect();
    let mut removed = Vec::new();
    let Ok(entries) = std::fs::read_dir(&paths.versions) else {
        return Ok(removed);
    };
    for entry in entries {
        let entry = entry?;
        let dir = entry.path();
        if keep.contains(&dir) {
            continue;
        }
        std::fs::remove_dir_all(&dir)?;
        removed.push(entry.file_name().to_string_lossy().into_owned());
    }
    Ok(removed)
}

// ---------- health and switching ----------

/// GET /api/health on the gateway, with a plain TCP request so the CLI needs no async runtime.
pub fn probe_health(port: u16, timeout: Duration) -> Option<Value> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    stream
        .write_all(b"GET /api/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut text = String::new();
    stream.read_to_string(&mut text).ok()?;
    let (head, body) = text.split_once("\r\n\r\n")?;
    if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
        return None;
    }
    serde_json::from_str(body).ok()
}

pub fn wait_until(timeout: Duration, mut check: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + timeout;
    loop {
        if check() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// What switching to a new version needs from its surroundings.
pub struct Switch<'a> {
    pub home: &'a Path,
    /// Whether a service runs the gateway (so it must be restarted); otherwise the user does it.
    pub managed: bool,
    pub restart: &'a dyn Fn() -> Result<()>,
    /// Does the gateway answer as `Some(version)`, or as any version for `None`?
    pub answering: &'a dyn Fn(Option<&str>) -> bool,
    pub log: &'a dyn Fn(&str),
    pub start_timeout: Duration,
    pub rollback_timeout: Duration,
}

/// Point `current` at `version_dir` and restart. If `version` doesn't answer, switch back.
pub fn switch_to(version_dir: &Path, version: &str, s: &Switch<'_>) -> Result<()> {
    activate(version_dir, s.home)?;
    (s.log)(&format!("Switched to {version}"));
    if !s.managed {
        (s.log)(&format!(
            "Installed {version}. Restart Bots (`bots serve`) to use it."
        ));
        return Ok(());
    }
    (s.restart)()?;
    (s.log)(&format!("Restarted on {version}; waiting for it to answer"));
    if wait_until(s.start_timeout, || (s.answering)(Some(version))) {
        let removed = prune(s.home)?;
        let extra = if removed.is_empty() {
            String::new()
        } else {
            format!(" Removed {}.", removed.join(", "))
        };
        (s.log)(&format!("Bots {version} is running.{extra}"));
        return Ok(());
    }
    (s.log)(&format!("{version} didn't answer; rolling back"));
    let back = rollback(s.home)?;
    (s.restart)()?;
    let back_name = back
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    if wait_until(s.rollback_timeout, || (s.answering)(None)) {
        bail!(
            "{version} didn't start, so Bots went back to {back_name}. Logs: {}",
            s.home.join("logs/gateway.log").display()
        );
    }
    bail!("{version} didn't start, so Bots went back to {back_name}, which isn't answering either. Logs: {}", s.home.join("logs/gateway.log").display());
}

// ---------- update log and time ----------

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `YYYY-MM-DDTHH:MM:SSZ` for a Unix time (UTC).
pub fn iso8601(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    let (h, mi, s) = (rem / 3600, rem % 3600 / 60, rem % 60);
    // Days since 1970-01-01 to a civil date (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// logs/update.log: every update step, with the time, whoever started it.
pub fn log_update(home: &Path, line: &str) {
    let file = home.join("logs").join("update.log");
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
    {
        writeln!(f, "{} {line}", iso8601(unix_now())).ok();
    }
}

// ---------- update checks ----------

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub latest: Option<Latest>,
    pub update_available: bool,
    pub error: Option<String>,
    pub checked_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Latest {
    pub version: String,
    pub url: String,
    pub notes: String,
    pub published_at: Option<String>,
}

/// Stores the last answer; a check only reaches GitHub when the answer is older than `CHECK_TTL`.
#[derive(Default)]
pub struct UpdateChecker {
    status: UpdateStatus,
    next_at: Option<Instant>,
    failures: u32,
}

/// How long to wait after the `failures`-th failed check in a row.
pub fn backoff(failures: u32) -> Duration {
    CHECK_TTL
        .saturating_mul(2u32.saturating_pow(failures.min(16)))
        .min(MAX_BACKOFF)
}

impl UpdateChecker {
    fn record(&mut self, outcome: Result<Option<Release>>, checked_at: String, now: Instant) {
        match outcome {
            Ok(latest) => {
                let update_available = latest.as_ref().is_some_and(|r| {
                    compare_versions(&r.version, VERSION).is_ok_and(|o| o == Ordering::Greater)
                });
                self.status = UpdateStatus {
                    latest: latest.map(|r| Latest {
                        version: r.version,
                        url: r.url,
                        notes: r.notes.chars().take(4000).collect(),
                        published_at: r.published_at,
                    }),
                    update_available,
                    error: None,
                    checked_at: Some(checked_at),
                };
                self.failures = 0;
                self.next_at = Some(now + CHECK_TTL);
            }
            Err(error) => {
                self.status.error = Some(error.to_string());
                self.status.checked_at = Some(checked_at);
                self.next_at = Some(now + backoff(self.failures));
                self.failures += 1;
            }
        }
    }
}

// ---------- the automatic update ----------

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoState {
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub pending: Option<Pending>,
    #[serde(default)]
    pub waiting: Option<Waiting>,
    #[serde(default)]
    pub skip: Vec<String>,
    #[serde(default)]
    pub gave_up: Option<String>,
    #[serde(default)]
    pub notice: Option<Notice>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub from: String,
    pub to: String,
    pub url: String,
    pub at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Waiting {
    pub version: String,
    pub since: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub kind: String,
    pub version: String,
    pub from: String,
    pub url: String,
    pub at: String,
}

/// What `consider` decided. The caller carries it out.
#[derive(Debug, PartialEq)]
pub enum Step {
    Nothing,
    Wait,
    GaveUp,
    Install { version: String },
}

fn with_skip(state: &mut AutoState, version: &str) {
    if !state.skip.iter().any(|v| v == version) {
        state.skip.push(version.to_string());
    }
}

fn notice(kind: &str, version: &str, from: &str, url: &str, at: &str) -> Notice {
    Notice {
        kind: kind.into(),
        version: version.into(),
        from: from.into(),
        url: url.into(),
        at: at.into(),
    }
}

/// On start-up: an update the previous process began either worked (we run its version) or didn't.
/// Returns the log line for the outcome, if there was an update to settle.
pub fn settle(
    state: &mut AutoState,
    version: &str,
    installed: &dyn Fn(&str) -> bool,
    at: &str,
) -> Option<String> {
    let pending = state.pending.take()?;
    state.waiting = None;
    if version == pending.to {
        state.notice = Some(notice(
            "updated",
            &pending.to,
            &pending.from,
            &pending.url,
            at,
        ));
        return Some(format!(
            "auto-update: now running {} (was {})",
            pending.to, pending.from
        ));
    }
    // Still on the old version: the new one was installed and didn't start, or never got that far.
    let rolled_back = installed(&pending.to);
    with_skip(state, &pending.to);
    let kind = if rolled_back { "rolledBack" } else { "failed" };
    state.notice = Some(notice(kind, &pending.to, version, &pending.url, at));
    Some(format!(
        "auto-update: {} {}; staying on {version}",
        pending.to,
        if rolled_back {
            "didn't start, rolled back"
        } else {
            "failed"
        }
    ))
}

/// The updater ended without restarting us: don't retry that release.
pub fn mark_failed(
    state: &mut AutoState,
    version: &str,
    target: &str,
    reason: &str,
    at: &str,
) -> Option<String> {
    let pending = state.pending.take()?;
    if pending.to != target {
        state.pending = Some(pending);
        return None;
    }
    with_skip(state, target);
    state.notice = Some(notice("failed", target, version, &pending.url, at));
    Some(format!("auto-update: {target} failed: {reason}"))
}

/// Decide what to do with a fresh check. `busy` is how many Bot threads a restart would interrupt.
pub fn consider(
    state: &mut AutoState,
    check: &UpdateStatus,
    version: &str,
    busy: usize,
    now: u64,
    at: &str,
) -> (Step, Vec<String>) {
    let mut lines = Vec::new();
    if let Some(p) = state.pending.clone() {
        let started = parse_iso8601_ms(&p.at);
        if started.is_some_and(|t| now.saturating_sub(t) >= PENDING_MS) {
            lines.extend(mark_failed(state, version, &p.to, "it didn't finish", at));
        }
    }
    if !state.auto || state.pending.is_some() {
        return (Step::Nothing, lines);
    }
    let Some(latest) = check.latest.as_ref() else {
        return (Step::Nothing, lines);
    };
    if check.error.is_some() || !check.update_available {
        return (Step::Nothing, lines);
    }
    let target = latest.version.clone();
    if state.skip.contains(&target) || state.gave_up.as_deref() == Some(target.as_str()) {
        return (Step::Nothing, lines);
    }
    let since = match &state.waiting {
        Some(w) if w.version == target => w.since,
        _ => now,
    };
    if busy > 0 {
        if now.saturating_sub(since) >= MAX_WAIT_MS {
            lines.push(format!(
                "auto-update: Bots stayed busy for a day; not waiting for {target} any longer"
            ));
            state.waiting = None;
            state.gave_up = Some(target.clone());
            state.notice = Some(notice("waited", &target, version, &latest.url, at));
            return (Step::GaveUp, lines);
        }
        lines.push(format!(
            "auto-update: {target} is ready, waiting for {busy} busy Bot thread{}",
            if busy == 1 { "" } else { "s" }
        ));
        state.waiting = Some(Waiting {
            version: target,
            since,
        });
        return (Step::Wait, lines);
    }
    state.waiting = None;
    state.pending = Some(Pending {
        from: version.into(),
        to: target.clone(),
        url: latest.url.clone(),
        at: at.into(),
    });
    lines.push(format!(
        "auto-update: nothing is busy, updating {version} → {target}"
    ));
    (Step::Install { version: target }, lines)
}

/// Milliseconds since the Unix epoch for a time written by `iso8601`. None when it doesn't parse.
fn parse_iso8601_ms(iso: &str) -> Option<u64> {
    let b = iso.as_bytes();
    if b.len() != 20 || b[10] != b'T' || b[19] != b'Z' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| iso.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
    // Days from civil (H. Hinnant's algorithm), the inverse of `iso8601`.
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = if mo > 2 { mo - 3 } else { mo + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + h * 3600 + mi * 60 + s)
        .ok()
        .map(|secs| secs * 1000)
}

pub fn read_auto_state(file: &Path) -> AutoState {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn write_auto_state(file: &Path, state: &AutoState) -> Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(state)? + "\n")?;
    std::fs::rename(&tmp, file)?;
    Ok(())
}

/// Start `bots update --yes <version>` outside this process, so a restart of the service doesn't stop it.
/// On Linux systemd-run runs it in its own unit, since systemd stops every process in the service's cgroup.
pub fn start_updater(version: &str, preview: bool, home: &Path) -> Result<()> {
    let exe = std::env::current_exe()?;
    let mut args = vec!["update".to_string(), "--yes".into()];
    if preview {
        args.push("--preview".into());
    }
    args.push(version.into());
    let mut cmd = if std::env::consts::OS == "linux" {
        let mut c = Command::new("systemd-run");
        c.args(["--user", "--collect", "--quiet"]);
        for key in [
            "PATH",
            "HOME",
            "THREEBOT_HOME",
            "THREEBOT_RELEASES_URL",
            "THREEBOT_RELEASE_BASE_URL",
        ] {
            if let Ok(value) = std::env::var(key) {
                c.arg(format!("--setenv={key}={value}"));
            }
        }
        c.arg(&exe);
        c
    } else {
        Command::new(&exe)
    };
    cmd.args(&args)
        .env("THREEBOT_HOME", home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    cmd.spawn().context("starting the updater")?;
    Ok(())
}

// ---------- the running gateway ----------

/// The update state the gateway keeps: the checker, the automatic update and the last check it acted on.
pub struct System {
    home: PathBuf,
    auto_file: PathBuf,
    client: reqwest::Client,
    source: ReleaseSource,
    checker: tokio::sync::Mutex<UpdateChecker>,
    auto: tokio::sync::Mutex<AutoState>,
    last_considered: tokio::sync::Mutex<Option<String>>,
}

impl System {
    pub fn new(home: PathBuf, app_dir: PathBuf) -> Self {
        let auto_file = app_dir.join("auto-update.json");
        Self {
            auto: tokio::sync::Mutex::new(read_auto_state(&auto_file)),
            auto_file,
            client: http_client().unwrap_or_else(|_| reqwest::Client::new()),
            source: ReleaseSource::from_env(),
            checker: tokio::sync::Mutex::new(UpdateChecker::default()),
            last_considered: tokio::sync::Mutex::new(None),
            home,
        }
    }

    /// The stored answer, checked again only when it is due (or `force`).
    pub async fn check(&self, force: bool) -> UpdateStatus {
        let mut checker = self.checker.lock().await;
        let due = checker.next_at.is_none_or(|t| Instant::now() >= t);
        if !force && !due {
            return checker.status.clone();
        }
        let preview = is_preview(VERSION);
        let outcome = find_release(&self.client, &self.source, None, preview).await;
        checker.record(outcome, iso8601(unix_now()), Instant::now());
        checker.status.clone()
    }

    async fn persist(&self, state: &AutoState) {
        if let Err(error) = write_auto_state(&self.auto_file, state) {
            tracing::error!("auto-update: couldn't save its state: {error}");
        }
    }

    async fn log(&self, line: &str) {
        tracing::info!("{line}");
        log_update(&self.home, line);
    }

    /// Settle an update the previous process began. Run once at start-up.
    pub async fn settle_on_start(&self) {
        let mut state = self.auto.lock().await;
        let home = self.home.clone();
        let installed = move |v: &str| {
            RuntimePaths::new(&home)
                .versions
                .join(v)
                .join(".install-complete")
                .exists()
        };
        if let Some(line) = settle(&mut state, VERSION, &installed, &iso8601(unix_now())) {
            self.log(&line).await;
            self.persist(&state).await;
        }
    }

    /// Act on a fresh check: start an automatic update, or wait. Acts once per new check.
    pub async fn consider(&self, check: &UpdateStatus, busy: Result<usize>) {
        let mut last = self.last_considered.lock().await;
        if check.checked_at.is_none() || *last == check.checked_at {
            return;
        }
        let busy = match busy {
            Ok(n) => n,
            Err(error) => {
                self.log(&format!(
                    "auto-update: couldn't read the busy Bot threads: {error}"
                ))
                .await;
                return;
            }
        };
        *last = check.checked_at.clone();
        drop(last);
        let mut state = self.auto.lock().await;
        let now_ms = unix_now() * 1000;
        let at = iso8601(now_ms / 1000);
        let (step, lines) = consider(&mut state, check, VERSION, busy, now_ms, &at);
        for line in &lines {
            self.log(line).await;
        }
        self.persist(&state).await;
        if let Step::Install { version } = step {
            let preview = is_preview(VERSION);
            if let Err(error) = start_updater(&version, preview, &self.home) {
                if let Some(line) =
                    mark_failed(&mut state, VERSION, &version, &error.to_string(), &at)
                {
                    self.log(&line).await;
                }
                self.persist(&state).await;
            }
        }
    }

    pub async fn auto_status(&self) -> Value {
        let s = self.auto.lock().await;
        json!({ "auto": s.auto, "waiting": s.waiting, "notice": s.notice })
    }
}

/// Start-up hook: settle a finished update, and for copies that can update themselves keep the
/// stored check fresh every minute, so the update button and the automatic update need no browser.
pub async fn start(state: Arc<AppState>) {
    state.system.settle_on_start().await;
    if installed_runtime(
        &std::env::current_exe().unwrap_or_default(),
        &state.cfg.data_root,
    ) {
        tokio::spawn(async move {
            loop {
                let status = state.system.check(false).await;
                let busy = busy_threads(&state).await;
                state.system.consider(&status, busy).await;
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });
    }
}

/// Bot threads a restart would interrupt. Reading T3's thread list (`orchestration.subscribeShell`,
/// a stream) isn't ported to the Rust T3 client yet, so this is an error and nothing installs
/// automatically until it is.
async fn busy_threads(_state: &AppState) -> Result<usize> {
    bail!("reading T3's thread list (orchestration.subscribeShell) isn't ported yet")
}

// ---------- routes ----------

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/system", get(system_status))
        .route("/api/system/auto-update", put(set_auto_update))
        .route("/api/system/update-notice", delete(dismiss_notice))
        .route("/api/system/update", post(start_update))
}

async fn system_status(
    State(s): State<Arc<AppState>>,
    Query(q): Query<HashMap<String, String>>,
) -> Json<Value> {
    let update = s
        .system
        .check(q.get("check").is_some_and(|v| v == "1"))
        .await;
    let mut out = json!({
        "version": VERSION,
        "installed": installed_runtime(&std::env::current_exe().unwrap_or_default(), &s.cfg.data_root),
        "service": std::env::var("THREEBOT_SERVICE").is_ok_and(|v| v == "1"),
        "latest": update.latest,
        "updateAvailable": update.update_available,
        "error": update.error,
        "checkedAt": update.checked_at,
    });
    if let (Value::Object(base), Value::Object(auto)) = (&mut out, s.system.auto_status().await) {
        base.extend(auto);
    }
    Json(out)
}

async fn set_auto_update(State(s): State<Arc<AppState>>, body: Bytes) -> Response {
    let auto = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|v| v["auto"].as_bool());
    let Some(auto) = auto else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "auto must be true or false" })),
        )
            .into_response();
    };
    let mut state = s.system.auto.lock().await;
    state.auto = auto;
    state.waiting = None;
    state.gave_up = None;
    s.system.persist(&state).await;
    drop(state);
    Json(s.system.auto_status().await).into_response()
}

#[derive(Deserialize)]
struct NoticeAt {
    at: Option<String>,
}

async fn dismiss_notice(State(s): State<Arc<AppState>>, Query(q): Query<NoticeAt>) -> Json<Value> {
    let mut state = s.system.auto.lock().await;
    if state
        .notice
        .as_ref()
        .is_some_and(|n| q.at.as_ref().is_none_or(|at| *at == n.at))
    {
        state.notice = None;
        s.system.persist(&state).await;
    }
    drop(state);
    Json(s.system.auto_status().await)
}

async fn start_update(State(s): State<Arc<AppState>>) -> ApiResult<Response> {
    let exe = std::env::current_exe().unwrap_or_default();
    if !installed_runtime(&exe, &s.cfg.data_root) {
        return Err(crate::error::ApiError::new(
            409,
            "This copy runs from a source checkout. Install a release to update it.",
        ));
    }
    let status = s.system.check(true).await;
    let Some(latest) = status.latest.filter(|_| status.update_available) else {
        return Err(crate::error::ApiError::new(
            409,
            format!("Bots {VERSION} is up to date."),
        ));
    };
    let preview = is_preview(VERSION);
    start_updater(&latest.version, preview, &s.cfg.data_root)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "started": true, "version": latest.version })),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_like_threebot() {
        let ord = |a, b| compare_versions(a, b).unwrap();
        assert_eq!(ord("1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(ord("1.10.0", "1.9.9"), Ordering::Greater);
        assert_eq!(ord("v2.0.0", "1.99.0"), Ordering::Greater);
        assert_eq!(ord("1.0.0-preview.1", "1.0.0"), Ordering::Less);
        assert_eq!(ord("1.0.0", "1.0.0-preview.9"), Ordering::Greater);
        assert_eq!(ord("1.0.0-preview.2", "1.0.0-preview.10"), Ordering::Less);
        assert_eq!(ord("1.0.0-alpha", "1.0.0-1"), Ordering::Greater);
        assert!(compare_versions("nope", "1.0.0").is_err());
        assert!(compare_versions("1.2", "1.0.0").is_err());
        assert!(compare_versions("1.2.3.4", "1.0.0").is_err());
    }

    #[test]
    fn preview_channel_follows_the_version() {
        assert!(is_preview("1.0.0-preview.1"));
        assert!(!is_preview("1.0.0"));
    }

    #[test]
    fn sums_are_found_by_exact_name() {
        let hash = "a".repeat(64);
        let sums = format!("{hash}  bots-1.0.0-x86_64-linux.tar.gz\n{} *bots-1.0.0-aarch64-macos.tar.gz\nnot a line\n", "B".repeat(64));
        assert_eq!(
            expected_sum(&sums, "bots-1.0.0-x86_64-linux.tar.gz"),
            Some(hash)
        );
        assert_eq!(
            expected_sum(&sums, "bots-1.0.0-aarch64-macos.tar.gz"),
            Some("b".repeat(64))
        );
        assert_eq!(expected_sum(&sums, "bots-1.0.0-x86_64-linux.tar"), None);
        assert_eq!(
            expected_sum(
                &format!("{} bots-x.tar.gz", "c".repeat(63)),
                "bots-x.tar.gz"
            ),
            None
        );
    }

    #[test]
    fn asset_names() {
        assert_eq!(
            asset_name("1.2.3", "aarch64-macos"),
            "bots-1.2.3-aarch64-macos.tar.gz"
        );
    }

    fn release_list() -> Vec<Value> {
        vec![
            json!({ "tag_name": "v1.1.0", "prerelease": false, "draft": false, "body": "notes", "html_url": "https://example.test/r/1.1.0",
                    "assets": [ { "name": "bots-1.1.0-x86_64-linux.tar.gz", "browser_download_url": "https://dl.test/a.tgz" }, { "name": "SHA256SUMS", "browser_download_url": "https://dl.test/s" } ] }),
            json!({ "tag_name": "v1.2.0-preview.1", "prerelease": true, "draft": false, "assets": [ { "name": "bots-1.2.0-preview.1-x86_64-linux.tar.gz", "browser_download_url": "https://dl.test/1.2.0-preview.1.tgz" }, { "name": "SHA256SUMS", "browser_download_url": "https://dl.test/1.2.0-preview.1.sums" } ] }),
            json!({ "tag_name": "v1.3.0", "prerelease": false, "draft": true, "assets": [ { "name": "bots-1.3.0-x86_64-linux.tar.gz", "browser_download_url": "https://dl.test/1.3.0.tgz" }, { "name": "SHA256SUMS", "browser_download_url": "https://dl.test/1.3.0.sums" } ] }),
            json!({ "tag_name": "v1.0.0", "prerelease": false, "draft": false, "assets": [ { "name": "bots-1.0.0-x86_64-linux.tar.gz", "browser_download_url": "https://dl.test/1.0.0.tgz" }, { "name": "SHA256SUMS", "browser_download_url": "https://dl.test/1.0.0.sums" } ] }),
        ]
    }

    #[test]
    fn newest_stable_release_is_picked_and_drafts_and_previews_skipped() {
        let r = pick_release(&release_list(), None, false, None, "x86_64-linux").unwrap();
        assert_eq!(r.version, "1.1.0");
        assert_eq!(r.archive, "https://dl.test/a.tgz");
        assert_eq!(r.sums, "https://dl.test/s");
        assert_eq!(r.notes, "notes");
        let preview = pick_release(&release_list(), None, true, None, "x86_64-linux").unwrap();
        assert_eq!(preview.version, "1.2.0-preview.1");
        assert!(
            pick_release(&release_list(), Some("1.3.0"), false, None, "x86_64-linux").is_none()
        );
        assert_eq!(
            pick_release(&release_list(), Some("v1.0.0"), false, None, "x86_64-linux")
                .unwrap()
                .version,
            "1.0.0"
        );
    }

    #[test]
    fn a_download_base_builds_the_urls() {
        let r = pick_release(
            &release_list(),
            Some("1.1.0"),
            false,
            Some("https://mirror.test/x"),
            "x86_64-linux",
        )
        .unwrap();
        assert_eq!(
            r.archive,
            "https://mirror.test/x/v1.1.0/bots-1.1.0-x86_64-linux.tar.gz"
        );
        assert_eq!(r.sums, "https://mirror.test/x/v1.1.0/SHA256SUMS");
    }

    #[test]
    fn version_comparison_and_iso_time() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
        for secs in [0u64, 1_700_000_000, 951_782_400, 1_234_567_890] {
            assert_eq!(parse_iso8601_ms(&iso8601(secs)), Some(secs * 1000));
        }
        assert_eq!(parse_iso8601_ms("yesterday"), None);
    }

    #[test]
    fn backoff_grows_and_stops_at_the_cap() {
        assert_eq!(backoff(0), CHECK_TTL);
        assert_eq!(backoff(1), CHECK_TTL * 2);
        assert_eq!(backoff(40), MAX_BACKOFF);
    }

    #[test]
    fn links_switch_roll_back_and_prune() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let p = RuntimePaths::new(home);
        for v in ["1.0.0", "1.1.0", "1.2.0"] {
            std::fs::create_dir_all(p.versions.join(v)).unwrap();
        }
        let v1 = p.versions.join("1.0.0");
        let v2 = p.versions.join("1.1.0");
        assert_eq!(activate(&v1, home).unwrap(), None);
        assert_eq!(activate(&v2, home).unwrap(), Some(v1.clone()));
        assert_eq!(read_link(&p.current), Some(v2.clone()));
        assert_eq!(read_link(&p.previous), Some(v1.clone()));
        assert_eq!(rollback(home).unwrap(), v1);
        assert_eq!(read_link(&p.current), Some(v1.clone()));
        assert!(read_link(&p.previous).is_none());
        activate(&v2, home).unwrap();
        let removed = prune(home).unwrap();
        assert_eq!(removed, vec!["1.2.0".to_string()]);
        assert!(p.versions.join("1.0.0").exists() && p.versions.join("1.1.0").exists());
    }

    #[test]
    fn rollback_without_a_previous_version_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert!(rollback(dir.path()).is_err());
    }

    #[test]
    fn installed_runtime_means_the_binary_sits_in_a_version_folder() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let versions = RuntimePaths::new(home).versions;
        std::fs::create_dir_all(versions.join("1.0.0")).unwrap();
        std::fs::write(versions.join("1.0.0").join("bots"), "").unwrap();
        assert!(installed_runtime(
            &versions.join("1.0.0").join("bots"),
            home
        ));
        std::fs::write(home.join("bots"), "").unwrap();
        assert!(!installed_runtime(&home.join("bots"), home));
    }

    #[test]
    fn switching_keeps_the_new_version_when_it_answers() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let p = RuntimePaths::new(home);
        std::fs::create_dir_all(p.versions.join("1.0.0")).unwrap();
        std::fs::create_dir_all(p.versions.join("1.1.0")).unwrap();
        activate(&p.versions.join("1.0.0"), home).unwrap();
        let log = |_: &str| {};
        let restarts = std::cell::Cell::new(0);
        let restart = || {
            restarts.set(restarts.get() + 1);
            Ok(())
        };
        let answering = |v: Option<&str>| v.is_none() || v == Some("1.1.0");
        let s = Switch {
            home,
            managed: true,
            restart: &restart,
            answering: &answering,
            log: &log,
            start_timeout: Duration::from_millis(400),
            rollback_timeout: Duration::from_millis(400),
        };
        switch_to(&p.versions.join("1.1.0"), "1.1.0", &s).unwrap();
        assert_eq!(read_link(&p.current), Some(p.versions.join("1.1.0")));
        assert_eq!(restarts.get(), 1);
    }

    #[test]
    fn switching_rolls_back_when_the_new_version_does_not_answer() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let p = RuntimePaths::new(home);
        std::fs::create_dir_all(p.versions.join("1.0.0")).unwrap();
        std::fs::create_dir_all(p.versions.join("1.1.0")).unwrap();
        activate(&p.versions.join("1.0.0"), home).unwrap();
        let log = |_: &str| {};
        let restart = || Ok(());
        let answering = |v: Option<&str>| v != Some("1.1.0");
        let s = Switch {
            home,
            managed: true,
            restart: &restart,
            answering: &answering,
            log: &log,
            start_timeout: Duration::from_millis(400),
            rollback_timeout: Duration::from_millis(400),
        };
        let err = switch_to(&p.versions.join("1.1.0"), "1.1.0", &s).unwrap_err();
        assert!(err.to_string().contains("went back to 1.0.0"), "{err}");
        assert_eq!(read_link(&p.current), Some(p.versions.join("1.0.0")));
    }

    #[test]
    fn settling_records_what_the_last_update_did() {
        let at = "2026-11-09T10:00:00Z";
        let pending = Pending {
            from: "1.0.0".into(),
            to: "1.1.0".into(),
            url: "u".into(),
            at: at.into(),
        };
        let mut state = AutoState {
            auto: true,
            pending: Some(pending.clone()),
            ..Default::default()
        };
        let line = settle(&mut state, "1.1.0", &|_| false, at).unwrap();
        assert!(line.contains("now running 1.1.0"));
        assert_eq!(state.notice.as_ref().unwrap().kind, "updated");
        assert!(state.pending.is_none());

        let mut state = AutoState {
            pending: Some(pending.clone()),
            ..Default::default()
        };
        settle(&mut state, "1.0.0", &|v| v == "1.1.0", at).unwrap();
        assert_eq!(state.notice.as_ref().unwrap().kind, "rolledBack");
        assert_eq!(state.skip, vec!["1.1.0".to_string()]);

        let mut state = AutoState {
            pending: Some(pending),
            ..Default::default()
        };
        settle(&mut state, "1.0.0", &|_| false, at).unwrap();
        assert_eq!(state.notice.as_ref().unwrap().kind, "failed");
        assert!(settle(&mut AutoState::default(), "1.0.0", &|_| false, at).is_none());
    }

    fn fresh(version: &str, at: &str) -> UpdateStatus {
        UpdateStatus {
            latest: Some(Latest {
                version: version.into(),
                url: "https://r".into(),
                notes: String::new(),
                published_at: None,
            }),
            update_available: true,
            error: None,
            checked_at: Some(at.into()),
        }
    }

    #[test]
    fn an_automatic_update_waits_for_busy_threads_then_installs() {
        let now = 1_700_000_000_000u64;
        let at = iso8601(now / 1000);
        let check = fresh("1.1.0", &at);
        let mut state = AutoState {
            auto: true,
            ..Default::default()
        };
        let (step, _) = consider(&mut state, &check, "1.0.0", 2, now, &at);
        assert_eq!(step, Step::Wait);
        assert_eq!(state.waiting.as_ref().unwrap().since, now);
        let later = now + 60_000;
        let (step, _) = consider(&mut state, &check, "1.0.0", 0, later, &at);
        assert_eq!(
            step,
            Step::Install {
                version: "1.1.0".into()
            }
        );
        assert_eq!(state.pending.as_ref().unwrap().to, "1.1.0");
        assert!(state.waiting.is_none());
    }

    #[test]
    fn an_automatic_update_gives_up_after_a_day_and_skips_failed_releases() {
        let now = 1_700_000_000_000u64;
        let at = iso8601(now / 1000);
        let check = fresh("1.1.0", &at);
        let mut state = AutoState {
            auto: true,
            waiting: Some(Waiting {
                version: "1.1.0".into(),
                since: now - MAX_WAIT_MS,
            }),
            ..Default::default()
        };
        let (step, _) = consider(&mut state, &check, "1.0.0", 1, now, &at);
        assert_eq!(step, Step::GaveUp);
        assert_eq!(state.gave_up.as_deref(), Some("1.1.0"));
        let (step, _) = consider(&mut state, &check, "1.0.0", 0, now, &at);
        assert_eq!(step, Step::Nothing);

        let mut state = AutoState {
            auto: true,
            skip: vec!["1.1.0".into()],
            ..Default::default()
        };
        assert_eq!(
            consider(&mut state, &check, "1.0.0", 0, now, &at).0,
            Step::Nothing
        );
    }

    #[test]
    fn nothing_happens_when_auto_is_off_or_the_check_failed_or_an_update_runs() {
        let now = 1_700_000_000_000u64;
        let at = iso8601(now / 1000);
        let check = fresh("1.1.0", &at);
        let mut off = AutoState::default();
        assert_eq!(
            consider(&mut off, &check, "1.0.0", 0, now, &at).0,
            Step::Nothing
        );
        let mut on = AutoState {
            auto: true,
            ..Default::default()
        };
        let failed = UpdateStatus {
            error: Some("offline".into()),
            ..check.clone()
        };
        assert_eq!(
            consider(&mut on, &failed, "1.0.0", 0, now, &at).0,
            Step::Nothing
        );
        let mut running = AutoState {
            auto: true,
            pending: Some(Pending {
                from: "1.0.0".into(),
                to: "1.1.0".into(),
                url: "u".into(),
                at: at.clone(),
            }),
            ..Default::default()
        };
        assert_eq!(
            consider(&mut running, &check, "1.0.0", 0, now, &at).0,
            Step::Nothing
        );
    }

    #[test]
    fn a_stuck_update_is_failed_after_half_an_hour() {
        let now = 1_700_000_000_000u64;
        let started = iso8601((now - PENDING_MS - 1000) / 1000);
        let at = iso8601(now / 1000);
        let mut state = AutoState {
            auto: true,
            pending: Some(Pending {
                from: "1.0.0".into(),
                to: "1.1.0".into(),
                url: "u".into(),
                at: started,
            }),
            ..Default::default()
        };
        let check = fresh("1.1.0", &at);
        let (step, lines) = consider(&mut state, &check, "1.0.0", 0, now, &at);
        assert_eq!(step, Step::Nothing);
        assert!(lines.iter().any(|l| l.contains("didn't finish")));
        assert_eq!(state.notice.as_ref().unwrap().kind, "failed");
        assert!(state.skip.contains(&"1.1.0".to_string()));
    }

    /// A fake release server: the GitHub-style release list, one archive and its SHA256SUMS.
    async fn fake_release_server(version: &str) -> (String, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("bots"),
            format!("#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'bots {version}'; fi\n"),
        )
        .unwrap();
        let archive_name = asset_name(version, &target());
        let archive = dir.path().join(&archive_name);
        let ok = Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&src)
            .arg("bots")
            .status()
            .unwrap();
        assert!(ok.success());
        let bytes = std::fs::read(&archive).unwrap();
        let sums = format!("{}  {archive_name}\n", hex::encode(Sha256::digest(&bytes)));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let releases = json!([{
            "tag_name": format!("v{version}"),
            "prerelease": false,
            "draft": false,
            "body": "notes",
            "assets": [
                { "name": archive_name, "browser_download_url": format!("{base}/dl/{archive_name}") },
                { "name": "SHA256SUMS", "browser_download_url": format!("{base}/dl/SHA256SUMS") }
            ]
        }]);
        let archive_file = archive_name.clone();
        let app = axum::Router::new()
            .route(
                "/releases",
                get(move || {
                    let r = releases.clone();
                    async move { Json(r) }
                }),
            )
            .route(
                "/dl/{file}",
                get(
                    move |axum::extract::Path(file): axum::extract::Path<String>| {
                        let archive_file = archive_file.clone();
                        let sums = sums.clone();
                        let bytes = bytes.clone();
                        async move {
                            if file == "SHA256SUMS" {
                                sums.into_bytes()
                            } else if file == archive_file {
                                bytes
                            } else {
                                Vec::new()
                            }
                        }
                    },
                ),
            );
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("{base}/releases"), dir)
    }

    #[tokio::test]
    async fn an_update_from_a_release_server_switches_and_rolls_back_when_health_fails() {
        let (api, _keep) = fake_release_server("0.2.0").await;
        let client = http_client().unwrap();
        let source = ReleaseSource { api, base: None };
        let release = find_release(&client, &source, None, false)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(release.version, "0.2.0");

        let home_dir = tempfile::tempdir().unwrap();
        let home = home_dir.path();
        let p = RuntimePaths::new(home);
        std::fs::create_dir_all(p.versions.join("0.1.0")).unwrap();
        activate(&p.versions.join("0.1.0"), home).unwrap();

        let log = |_: &str| {};
        let installed = install_release(&client, home, &release, &log)
            .await
            .unwrap();
        assert_eq!(installed, p.versions.join("0.2.0"));
        assert!(installed.join("bots").is_file());

        let restart = || Ok(());
        // The new version never answers, so the switch must roll back to 0.1.0.
        let refuses_new = |v: Option<&str>| v != Some("0.2.0");
        let switch = Switch {
            home,
            managed: true,
            restart: &restart,
            answering: &refuses_new,
            log: &log,
            start_timeout: Duration::from_millis(400),
            rollback_timeout: Duration::from_millis(400),
        };
        assert!(switch_to(&installed, "0.2.0", &switch).is_err());
        assert_eq!(read_link(&p.current), Some(p.versions.join("0.1.0")));

        // A healthy new version keeps the switch.
        let answers = |_: Option<&str>| true;
        let switch = Switch {
            answering: &answers,
            ..switch
        };
        switch_to(&installed, "0.2.0", &switch).unwrap();
        assert_eq!(read_link(&p.current), Some(installed.clone()));
        assert_eq!(read_link(&p.previous), Some(p.versions.join("0.1.0")));
    }

    #[test]
    fn the_state_file_round_trips_in_camel_case() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("auto-update.json");
        assert_eq!(read_auto_state(&file), AutoState::default());
        let state = AutoState {
            auto: true,
            gave_up: Some("2.0.0".into()),
            ..Default::default()
        };
        write_auto_state(&file, &state).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"gaveUp\""), "{text}");
        assert_eq!(read_auto_state(&file), state);
    }
}
