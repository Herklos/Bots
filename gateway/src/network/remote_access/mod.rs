//! One-click ways to reach Bots from a phone, ported from `server/remote-access.mts` (modelled on
//! T3 Code's):
//! - Tailscale: `tailscale serve` on a free HTTPS port, in front of a loopback-only listener that
//!   trusts the signed-in Tailscale user. Private to the tailnet.
//! - Cloudflare: a quick tunnel (`cloudflared tunnel --url`), a public https://*.trycloudflare.com
//!   link with no account. Public, so it only starts once the app has a password.
//!
//! The choices persist in `access.json` (the Node gateway's format, so either gateway reads it)
//! and are applied again when the gateway starts. The app password is stored as a scrypt hash.

use crate::network::exposure::Tailnet;
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

mod cloudflared;
mod password;
mod state;
mod tailscale;
#[cfg(test)]
mod tests;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use cloudflared::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use password::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use state::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use tailscale::*;

#[derive(Debug)]
pub struct RemoteAccess {
    cfg: RemoteConfig,
    tailnet: Arc<Tailnet>,
    /// One change at a time: two quick clicks must not start two tunnels, or drop the password mid-start.
    queue: AsyncMutex<()>,
    tunnel: Mutex<Option<Running>>,
    tunnel_error: Mutex<Option<String>>,
    listener: Mutex<ListenerNote>,
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
            listener: Mutex::new(ListenerNote::default()),
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

    pub fn network_choice(&self) -> crate::network::Choice {
        let saved = self.saved();
        crate::network::resolve(&crate::network::Input {
            network_env: self.cfg.network_env.clone(),
            process_host: self.cfg.process_host.clone(),
            merged_host: self.cfg.merged_host.clone(),
            saved: saved.network,
            tailscale_enabled: saved.tailscale.as_ref().is_some_and(|t| t.enabled),
            env_tailnet: self.cfg.env_tailnet,
        })
    }

    /// Remember the listener this process actually bound, and why a wider one was refused.
    pub fn note_listener(&self, bound: &str, error: Option<String>, withheld: bool) {
        *self.listener.lock().unwrap() = ListenerNote {
            bound: bound.to_string(),
            error,
            withheld,
        };
    }

    fn network_requires_password(&self) -> bool {
        let choice = self.network_choice();
        choice.mode != crate::network::Mode::Local
            && !(crate::platform::config::dev_allow_any_origin()
                && choice.source == crate::network::Source::Host)
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
        let choice = self.network_choice();
        let note = self.listener.lock().unwrap().clone();
        let applied = crate::network::apply(
            &choice,
            self.password_required(),
            crate::platform::config::dev_allow_any_origin(),
        );
        let restart_required =
            !note.bound.is_empty() && !applied.withheld && applied.bind != note.bound;
        let listening = if note.bound.is_empty() {
            String::new()
        } else {
            format!("{}:{}", note.bound, self.cfg.local_port)
        };
        let urls = if choice.mode == crate::network::Mode::Tailscale {
            tailscale
                .get("url")
                .and_then(|url| url.as_str())
                .map(|url| vec![url.to_string()])
                .unwrap_or_default()
        } else if note.bound.is_empty() {
            Vec::new()
        } else {
            crate::network::reachable_urls(&note.bound, self.cfg.local_port)
        };
        let error = if applied.withheld {
            applied.error.clone()
        } else {
            note.error
                .clone()
                .filter(|err| err != crate::network::PASSWORD_REQUIRED)
        };
        json!({
            "password": { "set": self.password_required(), "fromEnv": self.password_from_env() },
            "tailscale": Value::Object(tailscale),
            "tunnel": {
                "enabled": saved.tunnel.as_ref().is_some_and(|t| t.enabled),
                "url": self.running_url(),
                "error": self.tunnel_error.lock().unwrap().clone(),
                "available": find_cloudflared(&self.cfg.home).is_some(),
            },
            "network": {
                "mode": crate::network::mode_name(choice.mode),
                "locked": choice.locked,
                "restartRequired": restart_required,
                "listening": listening,
                "urls": urls.join("\n"),
                "error": error,
                "http": choice.mode == crate::network::Mode::Lan,
            },
        })
    }

    /// Save `local`, `lan`, or `tailscale`. The listener changes on the next start. Tailscale Serve
    /// is turned on or off now, because that route is not the public socket.
    pub async fn set_network(&self, mode: &str, router: Router) -> Result<(), AccessError> {
        let _turn = self.queue.lock().await;
        if self.network_choice().locked {
            return Err(AccessError::conflict(
                "Network access is set by the environment.",
            ));
        }
        let mode = crate::network::parse_mode(mode).ok_or_else(|| {
            AccessError::bad_request("Network access must be local, lan, or tailscale.")
        })?;
        if mode != crate::network::Mode::Local && !self.password_required() {
            return Err(AccessError::coded(
                "needs-password",
                crate::network::PASSWORD_REQUIRED,
            ));
        }
        match mode {
            crate::network::Mode::Tailscale => {
                self.tailscale_on_locked(router).await?;
            }
            crate::network::Mode::Local | crate::network::Mode::Lan => {
                if !self.cfg.env_tailnet
                    && self.saved().tailscale.as_ref().is_some_and(|t| t.enabled)
                {
                    self.tailscale_off_locked().await?;
                }
            }
        }
        let name = crate::network::mode_name(mode);
        self.update(|state| state.network = Some(name.to_string()))
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
            if self.network_requires_password() {
                return Err(AccessError::conflict(
                    "Turn off access beyond this device before removing the password.",
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

    /// On start: bring back what was on. A failure is logged and shown in Settings, never fatal.
    /// `router` is the app as the tailnet listener serves it.
    pub async fn boot(self: &Arc<Self>, router: Router) {
        let _turn = self.queue.lock().await;
        let saved = self.saved();
        let withheld = self.listener.lock().unwrap().withheld;
        let skip_tailscale =
            withheld && self.network_choice().mode == crate::network::Mode::Tailscale;
        if !skip_tailscale {
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
