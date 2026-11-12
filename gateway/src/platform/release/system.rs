use super::*;

// ---------- the running gateway ----------

/// The update state the gateway keeps: the checker, the automatic update and the last check it acted on.
#[derive(Debug)]
pub struct System {
    pub(super) home: PathBuf,
    pub(super) auto_file: PathBuf,
    pub(super) client: reqwest::Client,
    pub(super) source: ReleaseSource,
    pub(super) checker: tokio::sync::Mutex<UpdateChecker>,
    pub(super) auto: tokio::sync::Mutex<AutoState>,
    pub(super) last_considered: tokio::sync::Mutex<Option<String>>,
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
        let now = Instant::now();
        checker.record(
            outcome.map_err(|e| anyhow!(e.to_string())),
            iso8601(unix_now()),
            now,
        );
        checker.status.clone()
    }

    pub(super) fn persist(&self, state: &AutoState) {
        if let Err(error) = write_auto_state(&self.auto_file, state) {
            tracing::error!("auto-update: couldn't save its state: {error}");
        }
    }

    pub(super) fn log(&self, line: &str) {
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
            self.log(&line);
            self.persist(&state);
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
                ));
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
            self.log(line);
        }
        self.persist(&state);
        if let Step::Install { version } = step {
            let preview = is_preview(VERSION);
            if let Err(error) = start_updater(&version, preview, &self.home) {
                if let Some(line) =
                    mark_failed(&mut state, VERSION, &version, &error.to_string(), &at)
                {
                    self.log(&line);
                }
                self.persist(&state);
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
pub(crate) async fn start(state: Arc<AppState>) {
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

/// Bot threads a restart would interrupt. T3 Code is the harness wired today; other backends
/// may be added. `orchestration.subscribeShell` stays open, so the read waits for one
/// snapshot and then interrupts it.
pub(super) async fn busy_threads(state: &AppState) -> Result<usize> {
    read_busy(&state.t3).await
}

pub(super) const SHELL: &str = "orchestration.subscribeShell";

pub(super) async fn read_busy(t3: &crate::harness::t3::T3) -> Result<usize> {
    let client = t3.rpc().await.context("couldn't reach T3")?;
    let item = client
        .read_until(SHELL, json!({}), |item| {
            matches!(
                item.get("kind").and_then(Value::as_str),
                Some("snapshot") | Some("snapshot-required")
            )
        })
        .await
        .context("couldn't read the busy Bot threads")?;
    if item.get("kind").and_then(Value::as_str) == Some("snapshot-required") {
        bail!("T3 asked for an HTTP snapshot before the shell stream; auto-update will wait");
    }
    Ok(busy_in_snapshot(
        item.get("snapshot").unwrap_or(&Value::Null),
    ))
}

/// Threads a restart would interrupt, counted once across `threads` and `archivedThreads`.
/// Live run statuses are `queued`, `preparing`, `starting`, `running` and `waiting`.
/// A settled thread still counts when it has live activity, a pending runtime request,
/// or a pending background task.
pub(super) fn busy_in_snapshot(snapshot: &Value) -> usize {
    ["threads", "archivedThreads"]
        .into_iter()
        .filter_map(|key| snapshot.get(key).and_then(Value::as_array))
        .flatten()
        .filter(|thread| thread_is_busy(thread))
        .count()
}

pub(crate) fn thread_is_busy(thread: &Value) -> bool {
    const LIVE: &[&str] = &["queued", "preparing", "starting", "running", "waiting"];
    if thread
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| LIVE.contains(&status))
    {
        return true;
    }
    if thread
        .get("activityRunStatus")
        .and_then(Value::as_str)
        .is_some_and(|status| matches!(status, "preparing" | "starting" | "running" | "waiting"))
    {
        return true;
    }
    if thread
        .get("pendingRuntimeRequest")
        .is_some_and(|value| !value.is_null())
    {
        return true;
    }
    thread
        .get("pendingBackgroundTasks")
        .and_then(Value::as_array)
        .is_some_and(|tasks| !tasks.is_empty())
}
