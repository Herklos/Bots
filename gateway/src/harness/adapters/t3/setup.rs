use crate::platform::cli::{port_of, print_line, service_spec, set_env_text, settings, Flags};
use anyhow::{anyhow, bail, Context, Result};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

// ---------- commands ----------

pub(crate) async fn up(flags: &Flags) -> Result<()> {
    let home = crate::platform::release::home_dir();
    std::fs::create_dir_all(&home)?;
    let settings = settings(&home);
    let port: u16 = match flags.value("port") {
        Some(p) => p.parse().context("--port takes a number")?,
        None => port_of(&settings),
    };
    let t3_url = flags
        .value("t3-url")
        .map(String::from)
        .or_else(|| settings.get("T3_URL").cloned())
        .unwrap_or_else(|| "http://127.0.0.1:3773".into())
        .trim_end_matches('/')
        .to_string();
    let token_file = settings
        .get("T3_TOKEN_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("t3-token"));

    let env_file = home.join(".env");
    if !env_file.exists() {
        let text = format!(
            "# hbots settings. Every option is read from here or from the environment.\n# HBOTS_NETWORK_ACCESS=local, lan, or tailscale overrides Settings and locks that control.\nT3_URL={t3_url}\nT3_TOKEN_FILE={}\nHOST=127.0.0.1\nPORT={port}\n",
            token_file.display()
        );
        crate::network::auth::write_private(&env_file, &text)?;
        print_line(&format!("Wrote {}", env_file.display()));
    } else {
        let mut text = std::fs::read_to_string(&env_file)?;
        let before = text.clone();
        if flags.value("port").is_some() {
            text = set_env_text(&text, "PORT", &port.to_string());
        }
        if flags.value("t3-url").is_some() {
            text = set_env_text(&text, "T3_URL", &t3_url);
        }
        if text != before {
            crate::network::auth::write_private(&env_file, &text)?;
            print_line(&format!("Updated {}", env_file.display()));
        }
    }

    let inline_token = settings
        .get("T3_TOKEN")
        .is_some_and(|token| !token.trim().is_empty());
    let token = configured_token(&settings, &token_file);
    let refresh = flags.has("refresh-token");
    let issue = match &token {
        None => {
            print_line("Asking T3 Code for a token");
            true
        }
        Some(_) if refresh => {
            print_line("Refreshing the T3 token");
            true
        }
        Some(token) => match session_can_operate(&t3_url, token).await {
            Ok(true) => false,
            Ok(false) => {
                print_line(
                    "The T3 token is missing preview:operate. Asking T3 Code for a new token.",
                );
                true
            }
            Err(error) => {
                print_line(&format!(
                    "Warning: could not read the T3 token's scopes ({error:#}). Leaving the token in place. Run `hbots up --refresh-token` to replace it."
                ));
                false
            }
        },
    };
    if issue {
        let issued = issue_t3_token()?;
        save_t3_token(&token_file, &env_file, inline_token, &issued)?;
    }

    let reachable = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(format!("{t3_url}/.well-known/t3/environment"))
        .send()
        .await
        .is_ok_and(|r| r.status().is_success());
    if !reachable {
        print_line(&format!("Warning: T3 Code doesn't answer at {t3_url}. Start it (`t3 up` or the desktop app) before creating Bots."));
    }

    if flags.disabled("service") {
        print_line("Settings are ready. Start Bots with `hbots serve`.");
        return Ok(());
    }
    let os = std::env::consts::OS;
    if !crate::platform::service::supported(os) {
        bail!("Background services work on macOS and Linux. Start Bots with `hbots serve`.");
    }
    let spec = service_spec(&home);
    let file = crate::platform::service::install(&spec, os)?;
    print_line(&format!(
        "Installed the background service ({})",
        file.display()
    ));
    let running = crate::platform::release::wait_until(Duration::from_secs(45), || {
        crate::platform::release::probe_health(port, Duration::from_secs(1)).is_some()
    });
    if !running {
        bail!(
            "The service started but the app doesn't answer on port {port}. Logs: {}",
            spec.log.display()
        );
    }
    let version = crate::platform::release::probe_health(port, Duration::from_secs(1))
        .and_then(|h| h["version"].as_str().map(String::from))
        .unwrap_or_default();
    print_line(&format!(
        "\nBots {version} is running: http://127.0.0.1:{port}"
    ));
    if os == "linux" {
        print_line("On a server, run `loginctl enable-linger $USER` so it keeps running after you log out.");
    }
    Ok(())
}

/// The token the gateway would use: `T3_TOKEN`, then the token file.
pub(crate) fn configured_token(
    settings: &HashMap<String, String>,
    token_file: &Path,
) -> Option<String> {
    settings
        .get("T3_TOKEN")
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
        .or_else(|| {
            std::fs::read_to_string(token_file)
                .ok()
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty())
        })
}

/// `GET /api/auth/session` with the saved bearer token. `Ok(false)` when the session is missing
/// `preview:operate` or T3 rejects the token. A transport or server failure is `Err`, so a good
/// token is kept when T3 Code does not answer.
pub(crate) async fn session_can_operate(t3_url: &str, token: &str) -> Result<bool> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(format!("{t3_url}/api/auth/session"))
        .bearer_auth(token)
        .send()
        .await
        .context("T3 Code did not answer /api/auth/session")?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Ok(false);
    }
    if !status.is_success() {
        bail!("T3 Code returned {status} for /api/auth/session");
    }
    let body: serde_json::Value = response.json().await.context("T3 session was not JSON")?;
    Ok(scopes_allow_preview(&body))
}

/// True when the session body's `scopes` array includes `preview:operate`.
pub(crate) fn scopes_allow_preview(body: &serde_json::Value) -> bool {
    body.get("scopes")
        .and_then(|scopes| scopes.as_array())
        .is_some_and(|scopes| {
            scopes
                .iter()
                .any(|scope| scope.as_str() == Some("preview:operate"))
        })
}

pub(crate) fn issue_t3_token() -> Result<String> {
    let t3 = find_t3().ok_or_else(|| anyhow!("No `t3` command found on your PATH or in ~/.t3/bin. Open the T3 Code app once so it writes ~/.t3/bin/t3, or get a token another way (`npx t3 auth session issue --ttl 365d --label hbots --token-only`), put it in T3_TOKEN in ~/.hbots/.env, then run `hbots up` again."))?;
    let out = std::process::Command::new(&t3)
        .args([
            "auth",
            "session",
            "issue",
            "--ttl",
            "365d",
            "--label",
            "hbots",
            "--token-only",
        ])
        .output()
        .context("running t3")?;
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || token.is_empty() {
        bail!("t3 didn't print a token.");
    }
    Ok(token)
}

pub(crate) fn save_t3_token(
    token_file: &Path,
    env_file: &Path,
    inline_token: bool,
    token: &str,
) -> Result<()> {
    crate::network::auth::write_private(token_file, &format!("{token}\n"))?;
    if inline_token {
        let text = std::fs::read_to_string(env_file).unwrap_or_default();
        crate::network::auth::write_private(env_file, &set_env_text(&text, "T3_TOKEN", token))?;
        print_line(&format!("Updated T3_TOKEN in {}", env_file.display()));
    }
    print_line(&format!(
        "Saved the T3 token to {} (valid for a year)",
        token_file.display()
    ));
    if std::env::var("T3_TOKEN").is_ok_and(|value| !value.trim().is_empty()) {
        print_line(
            "This shell's T3_TOKEN still overrides the saved token. Unset it before `hbots serve`.",
        );
    }
    Ok(())
}

/// The `t3` command: on PATH, then `~/.t3/bin/t3` (shim the desktop app writes on first open).
///
/// Current T3 Code (Nightly) does not ship a bare `Resources/t3` binary. The shim runs the app's
/// Electron binary as Node against `Resources/app.asar/apps/server/dist/bin.mjs`. Older layouts that
/// did put a `t3` next to the app are still checked as a last resort.
pub(crate) fn find_t3() -> Option<PathBuf> {
    for dir in std::env::var("PATH").unwrap_or_default().split(':') {
        let candidate = Path::new(dir).join("t3");
        if !dir.is_empty() && candidate.is_file() {
            return Some(candidate);
        }
    }
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let own = home.join(".t3/bin/t3");
    if own.is_file() {
        return Some(own);
    }
    for app in [
        "/Applications/T3 Code (Nightly).app/Contents/Resources/t3",
        "/Applications/T3 Code.app/Contents/Resources/t3",
        "/Applications/T3 Code (Nightly).app/Contents/MacOS/t3",
        "/Applications/T3 Code.app/Contents/MacOS/t3",
    ] {
        let candidate = PathBuf::from(app);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
