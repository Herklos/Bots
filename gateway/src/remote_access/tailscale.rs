use super::*;

pub(crate) const CMD_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

/// Tailscale Serve's HTTPS ports,.
pub(crate) const WEB_PORTS: [u16; 5] = [443, 8443, 8444, 8445, 9443];

// ---------- Tailscale ----------

/// The fields of `tailscale status --json` this file reads, reduced to what the setup needs.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct TailscaleFacts {
    pub state: String,
    pub running: bool,
    pub needs_login: bool,
    pub auth_url: Option<String>,
    pub dns_name: Option<String>,
    pub https: bool,
    pub login: Option<String>,
}

pub(crate) fn parse_tailscale_status(s: &Value) -> TailscaleFacts {
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
pub(crate) fn pick_serve_port(serve: &Value, local_url: &str) -> Option<(u16, bool)> {
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
pub(crate) fn serve_owner(serve: &Value, port: u16) -> Option<String> {
    serve["Web"]
        .as_object()?
        .iter()
        .find(|(host_port, _)| port_of(host_port) == Some(port))
        .and_then(|(_, conf)| conf["Handlers"]["/"]["Proxy"].as_str().map(String::from))
}

pub(crate) fn port_of(host_port: &str) -> Option<u16> {
    host_port.rsplit(':').next()?.parse().ok()
}

pub(crate) fn tailnet_url(dns_name: &str, port: u16) -> String {
    if port == 443 {
        format!("https://{dns_name}")
    } else {
        format!("https://{dns_name}:{port}")
    }
}

/// The `tailscale` to run: `HBOTS_TAILSCALE_PATH` (or legacy `TAILSCALE_PATH`), else PATH, else the macOS app binary.
pub(crate) fn find_tailscale() -> Option<String> {
    const MAC_APP: &str = "/Applications/Tailscale.app/Contents/MacOS/Tailscale";
    for key in ["HBOTS_TAILSCALE_PATH", "TAILSCALE_PATH"] {
        if let Some(path) = std::env::var(key).ok().filter(|p| !p.is_empty()) {
            return Some(path);
        }
    }
    if let Some(path) = find_on_path("tailscale") {
        return Some(path.to_string_lossy().into_owned());
    }
    (std::env::consts::OS == "macos" && Path::new(MAC_APP).is_file()).then(|| MAC_APP.to_string())
}

/// The result of `tailscale status`: whether it is installed, the facts, and an error if it failed.
#[derive(Debug, Default)]
pub(crate) struct TailscaleStatus {
    pub installed: bool,
    pub facts: TailscaleFacts,
    pub error: Option<String>,
}

#[derive(Debug)]
pub(crate) struct CmdError {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) message: String,
}

pub(crate) async fn run_cmd(
    bin: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, CmdError> {
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
pub(crate) async fn tailscale_status(bin: Option<&str>) -> TailscaleStatus {
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

pub(crate) fn not_running(error: String) -> TailscaleStatus {
    TailscaleStatus {
        installed: true,
        facts: TailscaleFacts {
            state: "NotRunning".into(),
            ..Default::default()
        },
        error: Some(error),
    }
}

pub(crate) async fn serve_config(bin: &str) -> Value {
    match run_cmd(bin, &["serve", "status", "--json"], STATUS_TIMEOUT).await {
        Ok(out) if out.trim().is_empty() => json!({}),
        Ok(out) => serde_json::from_str(&out).unwrap_or_else(|_| json!({})),
        Err(_) => json!({}),
    }
}

impl RemoteAccess {
    /// Turn on the Tailscale route: a listener for the signed-in user, then Serve on a free HTTPS port.
    /// `router` is the app as the tailnet listener serves it.
    pub async fn tailscale_on(&self, router: Router) -> Result<Value, AccessError> {
        let _turn = self.queue.lock().await;
        self.tailscale_on_locked(router).await
    }

    pub(crate) async fn tailscale_on_locked(&self, router: Router) -> Result<Value, AccessError> {
        if self.cfg.env_tailnet {
            return Err(AccessError::conflict(
                "Tailscale is set up in .env (HBOTS_TAILNET_PORT).",
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
        let Some(login) = f.login else {
            return Err(AccessError::conflict(
                "Couldn't tell which Tailscale user this machine belongs to.",
            ));
        };
        let Some(dns) = f.dns_name else {
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
        self.tailscale_off_locked().await
    }

    pub(crate) async fn tailscale_off_locked(&self) -> Result<(), AccessError> {
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
}
