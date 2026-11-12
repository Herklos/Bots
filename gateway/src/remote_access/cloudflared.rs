use super::*;

pub(crate) const TUNNEL_START_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) const TUNNEL_STOP_TIMEOUT: Duration = Duration::from_secs(2);

/// The same pinned cloudflared and checksums T3 Code downloads.
pub(crate) const CLOUDFLARED_VERSION: &str = "2026.10.4";

pub(crate) const CF_BASE: &str =
    "https://github.com/cloudflare/cloudflared/releases/download/2026.10.4";

// ---------- cloudflared ----------

pub(crate) struct CloudflaredAsset {
    pub(crate) url: String,
    pub(crate) sha256: &'static str,
    pub(crate) tgz: bool,
}

/// Node-style platform names (`darwin`, `linux`) and arch names (`arm64`, `x64`).
pub(crate) fn cloudflared_asset(platform: &str, arch: &str) -> Option<CloudflaredAsset> {
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
pub(crate) fn host_platform() -> (&'static str, &'static str) {
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

pub(crate) fn pinned_cloudflared(home: &Path, platform: &str, arch: &str) -> PathBuf {
    home.join("tools")
        .join("cloudflared")
        .join(CLOUDFLARED_VERSION)
        .join(format!("{platform}-{arch}"))
        .join("cloudflared")
}

pub(crate) fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// The cloudflared to run: `HBOTS_CLOUDFLARED_PATH` (or legacy `CLOUDFLARED_PATH`), else the pinned copy, else PATH.
pub(crate) fn find_cloudflared(home: &Path) -> Option<PathBuf> {
    for key in ["HBOTS_CLOUDFLARED_PATH", "CLOUDFLARED_PATH"] {
        if let Some(path) = std::env::var_os(key).filter(|p| !p.is_empty()) {
            return Some(PathBuf::from(path));
        }
    }
    let (platform, arch) = host_platform();
    let pinned = pinned_cloudflared(home, platform, arch);
    if pinned.is_file() {
        return Some(pinned);
    }
    find_on_path("cloudflared")
}

pub(crate) fn verify_sha256(bytes: &[u8], expected: &str) -> anyhow::Result<()> {
    let sum = hex::encode(Sha256::digest(bytes));
    if sum != expected {
        anyhow::bail!("cloudflared checksum mismatch: expected {expected}, got {sum}");
    }
    Ok(())
}

/// Download the pinned cloudflared, check its checksum, and install it under `home`.
pub(crate) async fn install_cloudflared(home: &Path) -> anyhow::Result<PathBuf> {
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
pub(crate) fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
pub(crate) fn set_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// The first `https://<name>.trycloudflare.com` in the text.
pub(crate) fn find_quick_url(text: &str) -> Option<String> {
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
#[derive(Debug)]
pub(crate) struct Running {
    pub(crate) id: u64,
    pub(crate) url: String,
    pub(crate) kill: Option<oneshot::Sender<()>>,
    pub(crate) task: JoinHandle<()>,
}

/// Start a quick tunnel to `local_url`. Resolves once cloudflared has a link and a registered edge
/// connection: it prints the link first, and the link doesn't work until a connection is up.
/// HTTP/2, like T3 Code's tunnels, because many networks block QUIC on port 7844.
pub(crate) async fn start_quick_tunnel(
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
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(line);
            }
        });
    } else {
        drop(tx);
    }
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
            if registered && url.is_some() {
                return url;
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

impl RemoteAccess {
    /// Start the Cloudflare quick tunnel. It is public, so it needs a password first.
    pub async fn tunnel_on(self: &Arc<Self>) -> Result<Value, AccessError> {
        let _turn = self.queue.lock().await;
        self.tunnel_on_locked().await
    }

    pub(crate) async fn tunnel_on_locked(self: &Arc<Self>) -> Result<Value, AccessError> {
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
    pub(crate) async fn watch_tunnel(
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
    pub(crate) async fn stop_tunnel(&self) {
        let running = self.tunnel.lock().unwrap().take();
        if let Some(mut running) = running {
            if let Some(kill) = running.kill.take() {
                let _ = kill.send(());
            }
            let _ = tokio::time::timeout(TUNNEL_STOP_TIMEOUT, running.task).await;
        }
    }
}
