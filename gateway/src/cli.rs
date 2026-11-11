//! The `hbots` command line: set up, run, check and update the gateway.
//! `hbots serve` (and no command at all, as before) returns to `main`, which starts the gateway.
use crate::{auth, release, service};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::json;
use std::{
    cmp::Ordering,
    collections::HashMap,
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const HELP: &str = "\
hbots VERSION
TAGLINE

Usage: hbots <command> [options]

  up                 Set up Bots on this machine and start it in the background.
                       --port <n>       Port for the app (default 3780)
                       --t3-url <url>   T3 Code server (default http://127.0.0.1:3773)
                       --no-service     Write the settings only; start it yourself with `hbots serve`
  serve              Run the gateway in this terminal.
  status [--json]    Show the version, the service and the gateway.
  update [version]   Install the newest release (or <version>) and restart the service.
                       --check            Only say whether there is a newer release
                       --preview          Include preview releases
                       --yes              Don't ask before restarting
                       --allow-downgrade  Allow an older version
  rollback           Go back to the version before the last update.
  service <install|uninstall|restart|status>
  version            Print the version.
  help               Print this help.

Data, settings and installed versions live in HBOTS_HOME (default ~/.hbots).
";

/// Run a command. Returns the exit code, or None when the gateway should start (`serve`, or no command).
pub(crate) async fn run(args: &[String]) -> Option<i32> {
    if args.iter().any(|a| a == "--version" || a == "-V")
        || args.first().map(String::as_str) == Some("version")
    {
        println!("hbots {}", release::VERSION);
        return Some(0);
    }
    let command = args.first()?;
    if command == "serve" {
        return None;
    }
    let (flags, rest) = parse_args(&args[1..]);
    let result = match command.as_str() {
        "help" | "--help" | "-h" => {
            print!(
                "{}",
                HELP.replace("VERSION", release::VERSION)
                    .replace("TAGLINE", &hbots_design::tokens().brand.tagline)
            );
            Ok(())
        }
        "up" => up(&flags).await,
        "status" => status(&flags),
        "update" => update(&flags, &rest).await,
        "rollback" => rollback(),
        "service" => service_command(&rest),
        other => {
            eprintln!("hbots: Unknown command: {other}. Run `hbots help`.");
            return Some(2);
        }
    };
    match result {
        Ok(()) => Some(0),
        Err(error) => {
            eprintln!("hbots: {error:#}");
            Some(1)
        }
    }
}

// ---------- arguments ----------

/// Options as the Node gateway's parser reads them: `--name value`, `--name=value`, `--name`, and `--no-name`.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Flags(HashMap<String, String>);

impl Flags {
    /// The flag is given and not turned off.
    pub(crate) fn has(&self, name: &str) -> bool {
        self.0.get(name).is_some_and(|v| v != "false")
    }
    /// The flag was given as `--no-<name>`.
    pub(crate) fn disabled(&self, name: &str) -> bool {
        self.0.get(name).is_some_and(|v| v == "false")
    }
    /// The value of a flag that takes one (`--port 3780`).
    pub(crate) fn value(&self, name: &str) -> Option<&str> {
        self.0
            .get(name)
            .map(String::as_str)
            .filter(|v| *v != "true" && *v != "false")
    }
}

/// Split the arguments after the command into options and the rest.
pub(crate) fn parse_args(argv: &[String]) -> (Flags, Vec<String>) {
    let mut flags = HashMap::new();
    let mut rest = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        let arg = &argv[i];
        if let Some(name) = arg.strip_prefix("--no-") {
            flags.insert(name.to_string(), "false".to_string());
        } else if let Some(body) = arg.strip_prefix("--") {
            if let Some((name, value)) = body.split_once('=') {
                flags.insert(name.to_string(), value.to_string());
            } else if matches!(body, "port" | "t3-url") {
                i += 1;
                flags.insert(
                    body.to_string(),
                    argv.get(i).cloned().unwrap_or_else(|| "true".into()),
                );
            } else {
                flags.insert(body.to_string(), "true".to_string());
            }
        } else {
            rest.push(arg.clone());
        }
        i += 1;
    }
    (Flags(flags), rest)
}

// ---------- settings ----------

/// The `KEY=value` lines of a `.env` file. Quotes around a value are removed.
pub(crate) fn parse_env(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.trim_start().split_once('=')?;
            let key = key.trim_end();
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            {
                return None;
            }
            let value = value.trim();
            let quoted = value.len() >= 2
                && (value.starts_with('"') && value.ends_with('"')
                    || value.starts_with('\'') && value.ends_with('\''));
            let value = if quoted {
                &value[1..value.len() - 1]
            } else {
                value
            };
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

fn is_key_line(line: &str, key: &str) -> bool {
    line.trim_start()
        .strip_prefix(key)
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

/// `.env` text with `KEY=value` set: the first such line is replaced, or the line is added. Other lines stay.
pub(crate) fn set_env_text(text: &str, key: &str, value: &str) -> String {
    let line = format!("{key}={value}");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    match lines.iter().position(|l| is_key_line(l, key)) {
        Some(i) => lines[i] = line,
        None => lines.push(line),
    }
    lines.join("\n") + "\n"
}

/// HBOTS_HOME/.env, with the real environment winning, for the settings the CLI reads.
fn settings(home: &Path) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = std::fs::read_to_string(home.join(".env"))
        .map(|text| parse_env(&text).into_iter().collect())
        .unwrap_or_default();
    for key in ["PORT", "T3_URL", "T3_TOKEN", "T3_TOKEN_FILE", "HOST"] {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                map.insert(key.to_string(), value);
            }
        }
    }
    map
}

fn port_of(settings: &HashMap<String, String>) -> u16 {
    settings
        .get("PORT")
        .and_then(|p| p.parse().ok())
        .unwrap_or(3780)
}

fn service_spec(home: &Path) -> service::ServiceSpec {
    let exe = std::env::current_exe().unwrap_or_default();
    // An installed copy runs through the `current` link, so an update only swaps the link.
    let exe = if release::installed_runtime(&exe, home) {
        release::current_binary(home)
    } else {
        exe
    };
    service::spec(exe, home, &std::env::var("PATH").unwrap_or_default())
}

fn print_line(line: &str) {
    println!("{line}");
}

// ---------- commands ----------

async fn up(flags: &Flags) -> Result<()> {
    let home = release::home_dir();
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
            "# hbots settings. Every option is read from here or from the environment.\nT3_URL={t3_url}\nT3_TOKEN_FILE={}\nHOST=127.0.0.1\nPORT={port}\n",
            token_file.display()
        );
        auth::write_private(&env_file, &text)?;
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
            auth::write_private(&env_file, &text)?;
            print_line(&format!("Updated {}", env_file.display()));
        }
    }

    let have_token = std::env::var("T3_TOKEN").is_ok_and(|t| !t.is_empty())
        || settings.contains_key("T3_TOKEN")
        || std::fs::read_to_string(&token_file).is_ok_and(|t| !t.trim().is_empty());
    if !have_token {
        let t3 = find_t3().ok_or_else(|| anyhow!("No `t3` command found on your PATH or in ~/.t3/bin. Open the T3 Code app once so it writes ~/.t3/bin/t3, or get a token another way (`npx t3 auth session issue --ttl 365d --label hbots --token-only`), put it in T3_TOKEN in ~/.hbots/.env, then run `hbots up` again."))?;
        print_line("Asking T3 Code for a token");
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
        auth::write_private(&token_file, &format!("{token}\n"))?;
        print_line(&format!(
            "Saved the T3 token to {} (valid for a year)",
            token_file.display()
        ));
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
    if !service::supported(os) {
        bail!("Background services work on macOS and Linux. Start Bots with `hbots serve`.");
    }
    let spec = service_spec(&home);
    let file = service::install(&spec, os)?;
    print_line(&format!(
        "Installed the background service ({})",
        file.display()
    ));
    let running = release::wait_until(Duration::from_secs(45), || {
        release::probe_health(port, Duration::from_secs(1)).is_some()
    });
    if !running {
        bail!(
            "The service started but the app doesn't answer on port {port}. Logs: {}",
            spec.log.display()
        );
    }
    let version = release::probe_health(port, Duration::from_secs(1))
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

/// The `t3` command: on PATH, then `~/.t3/bin/t3` (shim the desktop app writes on first open).
///
/// Current T3 Code (Nightly) does not ship a bare `Resources/t3` binary. The shim runs the app's
/// Electron binary as Node against `Resources/app.asar/apps/server/dist/bin.mjs`. Older layouts that
/// did put a `t3` next to the app are still checked as a last resort.
fn find_t3() -> Option<PathBuf> {
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

fn status(flags: &Flags) -> Result<()> {
    let home = release::home_dir();
    let settings = settings(&home);
    let port = port_of(&settings);
    let health = release::probe_health(port, Duration::from_secs(2));
    let exe = std::env::current_exe().unwrap_or_default();
    let installed = release::installed_runtime(&exe, &home);
    let spec = service_spec(&home);
    let os = std::env::consts::OS;
    let supported = service::supported(os);
    let service_installed = supported && service::installed(&spec, os);
    let t3 = settings
        .get("T3_URL")
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:3773".into());
    let gateway_version = health
        .as_ref()
        .and_then(|h| h["version"].as_str().map(String::from));
    if flags.has("json") {
        let out = json!({
            "version": release::VERSION,
            "home": home.display().to_string(),
            "installed": installed,
            "service": { "supported": supported, "installed": service_installed },
            "gateway": match &gateway_version {
                Some(v) => json!({ "running": true, "version": v, "url": format!("http://127.0.0.1:{port}") }),
                None => json!({ "running": false }),
            },
            "t3": { "url": t3 },
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    let suffix = if installed { "" } else { " (source build)" };
    print_line(&format!("hbots {}{suffix}", release::VERSION));
    print_line(&format!("Data:      {}", home.display()));
    let service_text = if !supported {
        "not supported here"
    } else if service_installed {
        "installed"
    } else {
        "not installed"
    };
    print_line(&format!("Service:   {service_text}"));
    match gateway_version {
        Some(v) => print_line(&format!(
            "Gateway:   running {v} at http://127.0.0.1:{port}"
        )),
        None => print_line("Gateway:   not running"),
    }
    print_line(&format!("T3 Code:   {t3}"));
    Ok(())
}

fn confirm(question: &str, yes: bool) -> bool {
    if yes {
        return true;
    }
    if !std::io::stdin().is_terminal() {
        return false;
    }
    print!("{question} [y/N] ");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).is_ok()
        && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

async fn update(flags: &Flags, rest: &[String]) -> Result<()> {
    let home = release::home_dir();
    let check_only = flags.has("check");
    let result = update_inner(flags, rest.first().map(String::as_str), &home).await;
    // An update writes its failures to logs/update.log, whoever started it.
    if let (Err(error), false) = (&result, check_only) {
        release::log_update(&home, &format!("Update failed: {error}"));
    }
    result
}

async fn update_inner(flags: &Flags, wanted: Option<&str>, home: &Path) -> Result<()> {
    let from = release::VERSION;
    let client = release::http_client()?;
    let source = release::ReleaseSource::from_env();
    let release = release::find_release(&client, &source, wanted, flags.has("preview"))
        .await?
        .ok_or_else(|| match wanted {
            Some(v) => anyhow!("There is no release {v}."),
            None => anyhow!("There are no releases yet."),
        })?;
    let newer = release::compare_versions(&release.version, from)? == Ordering::Greater;
    if flags.has("check") {
        if newer {
            print_line(&format!(
                "Bots {} is available (you have {from}). {}",
                release.version, release.url
            ));
        } else {
            print_line(&format!("Bots {from} is up to date."));
        }
        return Ok(());
    }
    let exe = std::env::current_exe().unwrap_or_default();
    if !release::installed_runtime(&exe, home) {
        bail!("This copy isn't an installed release, so it can't update itself. Install a release with install.sh.");
    }
    if !newer && wanted.is_none() {
        print_line(&format!("Bots {from} is up to date."));
        return Ok(());
    }
    if !newer
        && release::compare_versions(&release.version, from)? == Ordering::Less
        && !flags.has("allow-downgrade")
    {
        bail!(
            "{} is older than {from}. Pass --allow-downgrade to go back anyway.",
            release.version
        );
    }

    let log = |line: &str| {
        print_line(line);
        release::log_update(home, line);
    };
    log(&format!("Updating Bots {from} → {}", release.version));
    let dir = release::install_release(&client, home, &release, &log).await?;

    let spec = service_spec(home);
    let os = std::env::consts::OS;
    let managed = service::supported(os) && service::installed(&spec, os);
    if managed
        && !confirm(
            &format!("Restart Bots to switch from {from} to {}? Running Bot turns keep going in T3 Code.", release.version),
            flags.has("yes"),
        )
    {
        bail!("Not restarted. Run `hbots update --yes` to switch.");
    }
    let port = port_of(&settings(home));
    let restart = || service::restart(&spec, os);
    let answering = |want: Option<&str>| {
        release::probe_health(port, Duration::from_secs(2)).is_some_and(|h| {
            h["ok"] == true && want.is_none_or(|w| h["version"].as_str() == Some(w))
        })
    };
    let switch = release::Switch {
        home,
        managed,
        restart: &restart,
        answering: &answering,
        log: &log,
        start_timeout: Duration::from_secs(90),
        rollback_timeout: Duration::from_secs(60),
    };
    release::switch_to(&dir, &release.version, &switch)
}

fn rollback() -> Result<()> {
    let home = release::home_dir();
    let back = release::rollback(&home)?;
    let spec = service_spec(&home);
    let os = std::env::consts::OS;
    if service::supported(os) && service::installed(&spec, os) {
        service::restart(&spec, os)?;
    }
    let name = back
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    print_line(&format!("Switched back to {name}."));
    Ok(())
}

fn service_command(rest: &[String]) -> Result<()> {
    let home = release::home_dir();
    let spec = service_spec(&home);
    let os = std::env::consts::OS;
    match rest.first().map(String::as_str).unwrap_or("status") {
        "install" => print_line(&format!(
            "Installed {}",
            service::install(&spec, os)?.display()
        )),
        "uninstall" => {
            service::uninstall(&spec, os)?;
            print_line("Removed the background service. Your Bots and settings are kept.");
        }
        "restart" => {
            service::restart(&spec, os)?;
            print_line("Restarted.");
        }
        "status" => {
            if service::installed(&spec, os) {
                print_line(&format!("Installed ({})", spec.exe.display()));
            } else {
                print_line("Not installed. Run `hbots up`.");
            }
        }
        other => bail!("Unknown service action: {other}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn options_take_values_where_the_node_gateway_does() {
        let (flags, rest) = parse_args(&args(&[
            "--port",
            "4000",
            "--t3-url=http://x/",
            "--no-service",
            "--yes",
            "1.2.3",
        ]));
        assert_eq!(flags.value("port"), Some("4000"));
        assert_eq!(flags.value("t3-url"), Some("http://x/"));
        assert!(flags.disabled("service"));
        assert!(flags.has("yes"));
        assert!(!flags.has("preview"));
        assert_eq!(rest, args(&["1.2.3"]));
    }

    #[test]
    fn a_value_flag_at_the_end_is_not_a_value() {
        let (flags, _) = parse_args(&args(&["--port"]));
        assert_eq!(flags.value("port"), None);
    }

    #[test]
    fn env_lines_are_read_like_the_node_gateway_reads_them() {
        let text = "# comment\nPORT=3780\n  T3_URL = \"http://127.0.0.1:3773\"\nHOST='127.0.0.1'\nlower=no\nEMPTY=\n";
        let map: HashMap<String, String> = parse_env(text).into_iter().collect();
        assert_eq!(map["PORT"], "3780");
        assert_eq!(map["T3_URL"], "http://127.0.0.1:3773");
        assert_eq!(map["HOST"], "127.0.0.1");
        assert_eq!(map["EMPTY"], "");
        assert!(!map.contains_key("lower"));
    }

    #[test]
    fn setting_a_key_replaces_only_its_line() {
        let text = "PORTX=1\nPORT=3780\nHOST=127.0.0.1\n";
        assert_eq!(
            set_env_text(text, "PORT", "4000"),
            "PORTX=1\nPORT=4000\nHOST=127.0.0.1\n"
        );
        assert_eq!(set_env_text("HOST=a", "PORT", "1"), "HOST=a\nPORT=1\n");
        assert_eq!(set_env_text("", "PORT", "1"), "PORT=1\n");
    }

    #[test]
    fn help_lists_the_commands() {
        for command in ["up", "status", "update", "rollback", "service", "serve"] {
            assert!(
                HELP.contains(&format!("  {command}")),
                "{command} missing from help"
            );
        }
    }

    #[tokio::test]
    async fn help_and_version_end_without_starting_the_gateway() {
        assert_eq!(run(&args(&["help"])).await, Some(0));
        assert_eq!(run(&args(&["--version"])).await, Some(0));
        assert_eq!(run(&args(&["serve"])).await, None);
        assert_eq!(run(&[]).await, None);
        assert_eq!(run(&args(&["nonsense"])).await, Some(2));
    }
}
