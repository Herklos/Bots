use super::*;

/// Where installed versions and their links live.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RuntimePaths {
    pub versions: PathBuf,
    pub current: PathBuf,
    pub previous: PathBuf,
}

impl RuntimePaths {
    pub(crate) fn new(home: &Path) -> Self {
        let runtime = home.join("runtime");
        Self {
            versions: runtime.join("versions"),
            current: runtime.join("current"),
            previous: runtime.join("previous"),
        }
    }
}

/// True when `exe` runs from an installed version folder, so the copy can update itself.
pub(crate) fn installed_runtime(exe: &Path, home: &Path) -> bool {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let Some(version_dir) = real(exe).parent().map(Path::to_path_buf) else {
        return false;
    };
    version_dir.parent().map(Path::to_path_buf) == Some(real(&RuntimePaths::new(home).versions))
}

// ---------- links ----------

pub(super) fn swap_link(link: &Path, target: &Path) -> Result<()> {
    // Rename over a link is atomic, so `current` is never missing.
    let temp = PathBuf::from(format!("{}.{}.tmp", link.display(), std::process::id()));
    std::fs::remove_file(&temp).ok();
    symlink(target, &temp)?;
    std::fs::rename(&temp, link)?;
    Ok(())
}

/// Where a link points, as an absolute path.
pub(crate) fn read_link(link: &Path) -> Option<PathBuf> {
    let target = std::fs::read_link(link).ok()?;
    Some(if target.is_absolute() {
        target
    } else {
        link.parent()?.join(target)
    })
}

/// Point `current` at `version_dir`, remembering the version it replaces. Returns that one.
pub(crate) fn activate(version_dir: &Path, home: &Path) -> Result<Option<PathBuf>> {
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
pub(crate) fn rollback(home: &Path) -> Result<PathBuf> {
    let paths = RuntimePaths::new(home);
    let previous = read_link(&paths.previous)
        .filter(|p| p.exists())
        .ok_or_else(|| anyhow!("There is no earlier version to go back to."))?;
    swap_link(&paths.current, &previous)?;
    std::fs::remove_file(&paths.previous).ok();
    Ok(previous)
}

/// Delete every installed version except `current` and `previous`. Returns the names removed.
pub(crate) fn prune(home: &Path) -> Result<Vec<String>> {
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
pub(crate) fn probe_health(port: u16, timeout: Duration) -> Option<Value> {
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

pub(crate) fn wait_until(timeout: Duration, mut check: impl FnMut() -> bool) -> bool {
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
pub(crate) struct Switch<'a> {
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
pub(crate) fn switch_to(version_dir: &Path, version: &str, s: &Switch<'_>) -> Result<()> {
    activate(version_dir, s.home)?;
    (s.log)(&format!("Switched to {version}"));
    if !s.managed {
        (s.log)(&format!(
            "Installed {version}. Restart hbots (`hbots serve`) to use it."
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
        (s.log)(&format!("hbots {version} is running.{extra}"));
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
            "{version} didn't start, so hbots went back to {back_name}. Logs: {}",
            s.home.join("logs/gateway.log").display()
        );
    }
    bail!("{version} didn't start, so hbots went back to {back_name}, which isn't answering either. Logs: {}", s.home.join("logs/gateway.log").display());
}
