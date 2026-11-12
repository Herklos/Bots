use super::*;

// ---------- update log and time ----------

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `YYYY-MM-DDTHH:MM:SSZ` for a Unix time (UTC).
pub(crate) fn iso8601(unix_secs: u64) -> String {
    let days = i64::try_from(unix_secs / 86_400).unwrap_or(i64::MAX);
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
pub(crate) fn log_update(home: &Path, line: &str) {
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
#[derive(Debug, Default)]
pub(crate) struct UpdateChecker {
    pub(super) status: UpdateStatus,
    pub(super) next_at: Option<Instant>,
    pub(super) failures: u32,
}

/// How long to wait after the `failures`-th failed check in a row.
pub(crate) fn backoff(failures: u32) -> Duration {
    CHECK_TTL
        .saturating_mul(2u32.saturating_pow(failures.min(16)))
        .min(MAX_BACKOFF)
}

impl UpdateChecker {
    pub(super) fn record(
        &mut self,
        outcome: Result<Option<Release>>,
        checked_at: String,
        now: Instant,
    ) {
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
pub(crate) struct AutoState {
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
pub(crate) struct Pending {
    pub from: String,
    pub to: String,
    pub url: String,
    pub at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Waiting {
    pub version: String,
    pub since: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Notice {
    pub kind: String,
    pub version: String,
    pub from: String,
    pub url: String,
    pub at: String,
}

/// What `consider` decided. The caller carries it out.
#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    Nothing,
    Wait,
    GaveUp,
    Install { version: String },
}

pub(super) fn with_skip(state: &mut AutoState, version: &str) {
    if !state.skip.iter().any(|v| v == version) {
        state.skip.push(version.to_string());
    }
}

pub(super) fn notice(kind: &str, version: &str, from: &str, url: &str, at: &str) -> Notice {
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
pub(crate) fn settle(
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
pub(crate) fn mark_failed(
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
pub(crate) fn consider(
    state: &mut AutoState,
    check: &UpdateStatus,
    version: &str,
    busy: usize,
    now: u64,
    at: &str,
) -> (Step, Vec<String>) {
    let mut lines = Vec::new();
    let Some(checked_at) = check.checked_at.as_deref() else {
        return (Step::Nothing, lines);
    };
    let _ = checked_at;
    if let Some(p) = state.pending.clone() {
        let started = chrono_free_ms(&p.at);
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
                "auto-update: hbots stayed busy for a day; not waiting for {target} any longer"
            ));
            state.waiting = None;
            state.notice = Some(notice("waited", &target, version, &latest.url, at));
            state.gave_up = Some(target);
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

/// Milliseconds since the Unix epoch for an ISO time written by `iso8601`. None when it doesn't parse.
pub(super) fn chrono_free_ms(iso: &str) -> Option<u64> {
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

pub(crate) fn read_auto_state(file: &Path) -> AutoState {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn write_auto_state(file: &Path, state: &AutoState) -> Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(state)? + "\n")?;
    std::fs::rename(&tmp, file)?;
    Ok(())
}

/// Start `hbots update --yes <version>` outside this process, so a restart of the service doesn't stop it.
/// On Linux systemd-run runs it in its own unit, since systemd stops every process in the service's cgroup.
pub(crate) fn start_updater(version: &str, preview: bool, home: &Path) -> Result<()> {
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
            "HBOTS_HOME",
            "HBOTS_RELEASES_URL",
            "HBOTS_RELEASE_BASE_URL",
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
        .env("HBOTS_HOME", home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    cmd.spawn().context("starting the updater")?;
    Ok(())
}
