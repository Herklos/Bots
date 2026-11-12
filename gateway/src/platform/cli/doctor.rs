//! `hbots doctor`: a redacted diagnostics report.

use super::*;
use crate::harness::providers::ProviderHealth;
use crate::harness::t3::setup::{t3_reachable, token_check, TokenCheck};
use crate::platform::redact::{redact, Secrets};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[path = "doctor_host.rs"]
mod host;
use host::{free_bytes, tail_lines};

const LOG_LINES: usize = 200;
const LOG_BYTES: u64 = 512 * 1024;

const PROCESS_KEYS: &[&str] = &[
    "PORT",
    "HOST",
    "HOME",
    "T3_URL",
    "T3_TOKEN",
    "T3_TOKEN_FILE",
    "HBOTS_NETWORK_ACCESS",
    "HBOTS_PASSWORD",
    "BOT_PASSWORD",
    "HBOTS_ROOT",
    "HBOTS_TAILNET_PORT",
    "TAILNET_PORT",
];

pub(crate) async fn doctor(flags: &Flags) -> Result<()> {
    let home = crate::platform::release::home_dir();
    let file = file_env(&home);
    let process = process_env();
    let view = EnvView {
        file: &file,
        process: &process,
    };
    let secrets = secrets_from(&home, &view);
    let report = collect(&home, &view, &secrets).await;
    let text = if flags.has("json") {
        render_json(&report)
    } else {
        render_text(&report)
    };
    print_line(&redact(&text, &secrets));
    Ok(())
}

struct EnvView<'a> {
    file: &'a HashMap<String, String>,
    process: &'a HashMap<String, String>,
}

impl EnvView<'_> {
    fn get(&self, name: &str) -> Option<&str> {
        self.process
            .get(name)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                self.file
                    .get(name)
                    .map(String::as_str)
                    .filter(|value| !value.is_empty())
            })
    }
}

struct Report {
    version: String,
    install_path: String,
    installed: bool,
    service_supported: bool,
    service_installed: bool,
    service_state: String,
    port: u16,
    network: String,
    network_error: Option<String>,
    gateway_running: bool,
    gateway_version: Option<String>,
    t3_url: String,
    t3_reachable: bool,
    token: TokenReport,
    providers: Vec<ProviderHealth>,
    providers_error: Option<String>,
    storage_path: String,
    free_bytes: Option<u64>,
    logs: Vec<String>,
}

enum TokenReport {
    Missing,
    Unchecked { reason: String },
    Accepted { scopes: Vec<String> },
    Rejected { scopes: Vec<String> },
}

fn render_text(report: &Report) -> String {
    let mut lines = Vec::new();
    let suffix = if report.installed {
        ""
    } else {
        " (source build)"
    };
    lines.push(format!("hbots {}{suffix}", report.version));
    lines.push(format!("Install:   {}", report.install_path));
    lines.push(format!("Service:   {}", report.service_state));
    match report.gateway_version.as_deref() {
        Some(version) if report.gateway_running => lines.push(format!(
            "Gateway:   running {version} at http://127.0.0.1:{}",
            report.port
        )),
        _ => lines.push(format!("Gateway:   not running on port {}", report.port)),
    }
    let network = match &report.network_error {
        Some(error) => format!("{} ({error})", report.network),
        None => report.network.clone(),
    };
    lines.push(format!("Network:   {network}"));
    let reach = if report.t3_reachable {
        "reachable"
    } else {
        "not reachable"
    };
    lines.push(format!("T3 Code:   {} ({reach})", report.t3_url));
    lines.push(format!("Token:     {}", token_line(&report.token)));
    lines.push(format!("Providers: {}", provider_line(report)));
    lines.push(format!(
        "Storage:   {}, {}",
        report.storage_path,
        format_free(report.free_bytes)
    ));
    if report.logs.is_empty() {
        lines.push("Logs:      none".into());
    } else {
        lines.push(format!("Logs:      last {} lines", report.logs.len()));
        lines.extend(report.logs.iter().cloned());
    }
    lines.join("\n")
}

fn token_line(token: &TokenReport) -> String {
    match token {
        TokenReport::Missing => "not set".into(),
        TokenReport::Unchecked { reason } => format!("present, not checked ({reason})"),
        TokenReport::Accepted { scopes } if scopes.is_empty() => "valid, no scopes".into(),
        TokenReport::Accepted { scopes } => format!("valid, scopes: {}", scopes.join(", ")),
        TokenReport::Rejected { scopes } if scopes.is_empty() => "rejected".into(),
        TokenReport::Rejected { scopes } => format!("rejected, scopes: {}", scopes.join(", ")),
    }
}

fn provider_line(report: &Report) -> String {
    if report.providers.is_empty() {
        return report
            .providers_error
            .clone()
            .unwrap_or_else(|| "none".into());
    }
    let ready = ready_count(&report.providers);
    let parts: Vec<String> = report
        .providers
        .iter()
        .map(|row| format!("{} {}", row.display_name, row.status))
        .collect();
    format!("{ready} ready ({})", parts.join(", "))
}

fn ready_count(rows: &[ProviderHealth]) -> usize {
    rows.iter()
        .filter(|row| row.status == "ready" && row.enabled && row.installed)
        .count()
}

fn format_free(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "free space unknown".into();
    };
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B free")
    } else {
        format!("{value:.1} {} free", UNITS[unit])
    }
}

fn render_json(report: &Report) -> String {
    serde_json::to_string_pretty(&report_value(report)).unwrap_or_else(|_| "{}".into())
}

fn report_value(report: &Report) -> Value {
    let (present, valid, scopes, token_error) = match &report.token {
        TokenReport::Missing => (false, Value::Null, Vec::new(), None),
        TokenReport::Unchecked { reason } => (true, Value::Null, Vec::new(), Some(reason.clone())),
        TokenReport::Accepted { scopes } => (true, json!(true), scopes.clone(), None),
        TokenReport::Rejected { scopes } => (true, json!(false), scopes.clone(), None),
    };
    let mut token = json!({
        "present": present,
        "valid": valid,
        "scopes": scopes,
    });
    if let Some(error) = token_error {
        token["error"] = json!(error);
    }
    let list: Vec<Value> = report
        .providers
        .iter()
        .map(|row| {
            json!({
                "instanceId": row.instance_id,
                "displayName": row.display_name,
                "enabled": row.enabled,
                "installed": row.installed,
                "status": row.status,
                "models": row.models,
            })
        })
        .collect();
    let mut providers = json!({
        "ok": report.providers_error.is_none(),
        "ready": ready_count(&report.providers),
        "list": list,
    });
    if let Some(error) = &report.providers_error {
        providers["error"] = json!(error);
    }
    let gateway = json!({
        "running": report.gateway_running,
        "port": report.port,
        "version": report.gateway_version,
        "network": report.network,
        "networkError": report.network_error,
    });
    let body = json!({
        "version": report.version,
        "installPath": report.install_path,
        "installed": report.installed,
        "service": {
            "supported": report.service_supported,
            "installed": report.service_installed,
            "state": report.service_state,
        },
        "gateway": gateway,
        "t3": {
            "url": report.t3_url,
            "reachable": report.t3_reachable,
            "token": token,
        },
        "providers": providers,
        "storage": {
            "path": report.storage_path,
            "freeBytes": report.free_bytes,
        },
        "logs": report.logs,
    });
    body
}

/// The same redacted report `hbots doctor` prints, for Settings and Ask a Bot.
pub(crate) async fn bundle() -> (String, Value) {
    let home = crate::platform::release::home_dir();
    let file = file_env(&home);
    let process = process_env();
    let view = EnvView {
        file: &file,
        process: &process,
    };
    let secrets = secrets_from(&home, &view);
    let report = collect(&home, &view, &secrets).await;
    let value = report_value(&report);
    let text = redact(&render_text(&report), &secrets);
    let encoded = redact(
        &serde_json::to_string(&value).unwrap_or_else(|_| "{}".into()),
        &secrets,
    );
    let value = serde_json::from_str(&encoded).unwrap_or(value);
    (text, value)
}

async fn collect(home: &Path, view: &EnvView<'_>, secrets: &Secrets) -> Report {
    let exe = std::env::current_exe().unwrap_or_default();
    let installed = crate::platform::release::installed_runtime(&exe, home);
    let (service_supported, service_installed, service_state) = service_state(home);
    let port = view
        .get("PORT")
        .and_then(|value| value.parse().ok())
        .unwrap_or(3780);
    let choice = network_choice(home, view);
    let health = crate::platform::release::probe_health(port, Duration::from_secs(2));
    let t3_url = view
        .get("T3_URL")
        .unwrap_or("http://127.0.0.1:3773")
        .trim_end_matches('/')
        .to_string();
    let reachable = t3_reachable(&t3_url).await;
    let (token, providers, providers_error) = remote(&t3_url, reachable, secrets).await;
    let storage = storage_path(home, view);
    Report {
        version: crate::platform::release::VERSION.into(),
        install_path: exe.display().to_string(),
        installed,
        service_supported,
        service_installed,
        service_state: service_state.into(),
        port,
        network: crate::network::mode_name(choice.mode).into(),
        network_error: choice.error,
        gateway_running: health.is_some(),
        gateway_version: health
            .as_ref()
            .and_then(|body| body["version"].as_str().map(str::to_string)),
        t3_url,
        t3_reachable: reachable,
        token,
        providers,
        providers_error,
        storage_path: storage.display().to_string(),
        free_bytes: free_bytes(&storage),
        logs: tail_lines(&home.join("logs").join("gateway.log"), LOG_LINES, LOG_BYTES),
    }
}

async fn remote(
    t3_url: &str,
    reachable: bool,
    secrets: &Secrets,
) -> (TokenReport, Vec<ProviderHealth>, Option<String>) {
    let Some(token) = secrets.token.as_deref() else {
        return (
            TokenReport::Missing,
            Vec::new(),
            Some("not checked (no T3 token)".into()),
        );
    };
    if !reachable {
        return (
            TokenReport::Unchecked {
                reason: "T3 Code did not answer".into(),
            },
            Vec::new(),
            Some("not checked (T3 Code did not answer)".into()),
        );
    }
    let token_report = match token_check(t3_url, token).await {
        Ok(TokenCheck {
            valid: true,
            scopes,
        }) => TokenReport::Accepted { scopes },
        Ok(TokenCheck { scopes, .. }) => TokenReport::Rejected { scopes },
        Err(error) => TokenReport::Unchecked {
            reason: clip(&format!("{error:#}")),
        },
    };
    if matches!(token_report, TokenReport::Rejected { .. }) {
        return (
            token_report,
            Vec::new(),
            Some("not checked (T3 token was rejected)".into()),
        );
    }
    let t3 = crate::harness::t3::T3::new(t3_url.to_string(), token.to_string());
    match crate::harness::t3::provider_health(&t3).await {
        Ok(rows) => (token_report, rows, None),
        Err(error) => (token_report, Vec::new(), Some(clip(&error.to_string()))),
    }
}

fn clip(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(180) {
        Some((index, _)) => format!("{}...", &flat[..index]),
        None => flat,
    }
}

fn service_state(home: &Path) -> (bool, bool, &'static str) {
    let spec = service_spec(home);
    let os = std::env::consts::OS;
    let supported = crate::platform::service::supported(os);
    let installed = supported && crate::platform::service::installed(&spec, os);
    let state = if !supported {
        "not supported here"
    } else if installed {
        "installed"
    } else {
        "not installed"
    };
    (supported, installed, state)
}

fn file_env(home: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(home.join(".env"))
        .map(|text| parse_env(&text).into_iter().collect())
        .unwrap_or_default()
}

fn process_env() -> HashMap<String, String> {
    PROCESS_KEYS
        .iter()
        .filter_map(|key| {
            let value = std::env::var(key).ok()?;
            (!value.is_empty()).then_some(((*key).to_string(), value))
        })
        .collect()
}

fn secrets_from(home: &Path, view: &EnvView<'_>) -> Secrets {
    let mut extras = Vec::new();
    let access = access_state(home, view);
    if let Some(stored) = access.password {
        extras.push(stored.salt);
        extras.push(stored.hash);
    }
    let session_secret = storage_path(home, view)
        .join(".botapp")
        .join("session-secret");
    if let Ok(secret) = std::fs::read_to_string(session_secret) {
        let secret = secret.trim().to_string();
        if !secret.is_empty() {
            extras.push(secret);
        }
    }
    Secrets {
        token: load_token(home, view),
        password: view
            .get("HBOTS_PASSWORD")
            .or_else(|| view.get("BOT_PASSWORD"))
            .map(str::to_string),
        extras,
        home: view.get("HOME").map(str::to_string),
    }
}

fn load_token(home: &Path, view: &EnvView<'_>) -> Option<String> {
    if let Some(token) = view
        .get("T3_TOKEN")
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        return Some(token.to_string());
    }
    let path = view
        .get("T3_TOKEN_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".t3-token"));
    std::fs::read_to_string(path)
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

fn storage_path(home: &Path, view: &EnvView<'_>) -> PathBuf {
    let env = crate::storage::settings::ProcessEnv {
        root: view.get("HBOTS_ROOT").map(str::to_string),
        ..crate::storage::settings::ProcessEnv::default()
    };
    crate::storage::settings::resolve_root(home, &env)
}

fn access_state(home: &Path, view: &EnvView<'_>) -> crate::network::remote_access::AccessState {
    let path = storage_path(home, view).join(".botapp").join("access.json");
    crate::network::remote_access::read_state(&path)
}

fn network_choice(home: &Path, view: &EnvView<'_>) -> crate::network::Choice {
    let access = access_state(home, view);
    let tailnet = view
        .get("HBOTS_TAILNET_PORT")
        .or_else(|| view.get("TAILNET_PORT"))
        .and_then(|port| port.trim().parse::<u16>().ok())
        .unwrap_or(0);
    crate::network::resolve(&crate::network::Input {
        network_env: view.get("HBOTS_NETWORK_ACCESS").map(str::to_string),
        process_host: view.process.get("HOST").cloned(),
        merged_host: view.get("HOST").map(str::to_string),
        saved: access.network,
        tailscale_enabled: access.tailscale.as_ref().is_some_and(|saved| saved.enabled),
        env_tailnet: tailnet > 0,
    })
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
