use super::*;

pub(crate) fn status(flags: &Flags) -> Result<()> {
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

pub(crate) fn confirm(question: &str, yes: bool) -> bool {
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

pub(crate) async fn update(flags: &Flags, rest: &[String]) -> Result<()> {
    let home = release::home_dir();
    let check_only = flags.has("check");
    let result = update_inner(flags, rest.first().map(String::as_str), &home).await;
    // An update writes its failures to logs/update.log, whoever started it.
    if let (Err(error), false) = (&result, check_only) {
        release::log_update(&home, &format!("Update failed: {error}"));
    }
    result
}

pub(crate) async fn update_inner(flags: &Flags, wanted: Option<&str>, home: &Path) -> Result<()> {
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

pub(crate) fn rollback() -> Result<()> {
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

pub(crate) fn service_command(rest: &[String]) -> Result<()> {
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
