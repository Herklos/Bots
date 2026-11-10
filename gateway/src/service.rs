//! The gateway as a background service, a launchd agent on macOS and a
//! systemd user unit on Linux, both running `hbots serve`. They run the `current` link, so an update
//! only swaps the link and restarts the service. The label and unit name are `hbots`, which
//! `hbots service restart` and the updater already use.
use anyhow::{bail, Context, Result};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub const LAUNCHD_LABEL: &str = "hbots";
pub const SYSTEMD_UNIT: &str = "hbots.service";

/// What the service runs, and with which environment.
#[derive(Clone, Debug, PartialEq)]
pub struct ServiceSpec {
    pub exe: PathBuf,
    pub home: PathBuf,
    pub path: String,
    pub label: String,
    pub unit: String,
    pub log: PathBuf,
}

/// The service for `exe` (the binary the service starts) under `home`. `user_path` is the user's PATH:
/// a service starts with a bare one, and `t3` and `tailscale` must still be found.
pub fn spec(exe: PathBuf, home: &Path, user_path: &str) -> ServiceSpec {
    let mut dirs: Vec<&str> = user_path.split(':').filter(|d| !d.is_empty()).collect();
    dirs.extend(["/usr/local/bin", "/usr/bin", "/bin"]);
    let mut seen: Vec<&str> = Vec::new();
    for d in dirs {
        if !seen.contains(&d) {
            seen.push(d);
        }
    }
    ServiceSpec {
        exe,
        home: home.to_path_buf(),
        path: seen.join(":"),
        label: LAUNCHD_LABEL.into(),
        unit: SYSTEMD_UNIT.into(),
        log: home.join("logs").join("gateway.log"),
    }
}

/// The environment the service runs with.
pub fn environment(spec: &ServiceSpec) -> Vec<(String, String)> {
    vec![
        ("PATH".into(), spec.path.clone()),
        ("HBOTS_HOME".into(), spec.home.display().to_string()),
        ("HBOTS_SERVICE".into(), "1".into()),
    ]
}

pub fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The launchd agent property list.
pub fn launchd_plist(spec: &ServiceSpec) -> String {
    let env: String = environment(spec)
        .iter()
        .map(|(k, v)| {
            format!(
                "      <key>{}</key><string>{}</string>\n",
                xml_escape(k),
                xml_escape(v)
            )
        })
        .collect();
    let log = xml_escape(&spec.log.display().to_string());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key><string>{label}</string>
    <key>ProgramArguments</key>
    <array>
      <string>{exe}</string>
      <string>serve</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
{env}    </dict>
    <key>WorkingDirectory</key><string>{home}</string>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>
    <key>ThrottleInterval</key><integer>5</integer>
    <key>ProcessType</key><string>Interactive</string>
    <key>StandardOutPath</key><string>{log}</string>
    <key>StandardErrorPath</key><string>{log}</string>
  </dict>
</plist>
"#,
        label = xml_escape(&spec.label),
        exe = xml_escape(&spec.exe.display().to_string()),
        home = xml_escape(&spec.home.display().to_string()),
    )
}

/// Quote one word of a systemd `ExecStart=` or `Environment=` line when it needs it.
fn unit_word(value: &str) -> String {
    if value
        .chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\\')
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

/// The systemd user unit.
pub fn systemd_unit(spec: &ServiceSpec) -> String {
    let exec = [spec.exe.display().to_string(), "serve".into()]
        .iter()
        .map(|w| unit_word(w))
        .collect::<Vec<_>>()
        .join(" ");
    let env: String = environment(spec)
        .iter()
        .map(|(k, v)| format!("Environment={}\n", unit_word(&format!("{k}={v}"))))
        .collect();
    let log = spec.log.display();
    format!(
        "[Unit]\nDescription=hbots gateway\nAfter=network-online.target\n\n[Service]\nExecStart={exec}\nWorkingDirectory={home}\n{env}Restart=always\nRestartSec=5\nStandardOutput=append:{log}\nStandardError=append:{log}\n\n[Install]\nWantedBy=default.target\n",
        home = unit_word(&spec.home.display().to_string()),
    )
}

/// Where the service file for `os` ("macos" or "linux") goes, or None where services aren't supported.
pub fn service_file(spec: &ServiceSpec, os: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
    match os {
        "macos" => Some(
            home.join("Library/LaunchAgents")
                .join(format!("{}.plist", spec.label)),
        ),
        "linux" => {
            let config = std::env::var("XDG_CONFIG_HOME")
                .ok()
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            Some(config.join("systemd/user").join(&spec.unit))
        }
        _ => None,
    }
}

pub fn supported(os: &str) -> bool {
    os == "macos" || os == "linux"
}

/// The current user's id, from `id -u` (no libc dependency for one number).
fn uid() -> Result<String> {
    let out = Command::new("id")
        .arg("-u")
        .output()
        .context("running id -u")?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run a command and fail with its output when it doesn't succeed.
fn run(program: &str, args: &[&str]) -> Result<()> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("running {program}"))?;
    if !out.status.success() {
        bail!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Write the service file and start the service. Returns the file.
pub fn install(spec: &ServiceSpec, os: &str) -> Result<PathBuf> {
    if !supported(os) {
        bail!("Background services are supported on macOS and Linux.");
    }
    let file = service_file(spec, os).context("no service file for this system")?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if let Some(dir) = spec.log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if os == "macos" {
        let domain = format!("gui/{}", uid()?);
        let target = format!("{domain}/{}", spec.label);
        let _ = run("launchctl", &["bootout", &target]);
        std::fs::write(&file, launchd_plist(spec))?;
        let _ = run("launchctl", &["enable", &target]);
        run(
            "launchctl",
            &["bootstrap", &domain, &file.display().to_string()],
        )?;
    } else {
        std::fs::write(&file, systemd_unit(spec))?;
        run("systemctl", &["--user", "daemon-reload"])?;
        run("systemctl", &["--user", "enable", "--now", &spec.unit])?;
    }
    Ok(file)
}

/// Stop the service and remove its file. Settings and Bots are kept.
pub fn uninstall(spec: &ServiceSpec, os: &str) -> Result<()> {
    if os == "macos" {
        let target = format!("gui/{}/{}", uid()?, spec.label);
        let _ = run("launchctl", &["bootout", &target]);
    } else if os == "linux" {
        let _ = run("systemctl", &["--user", "disable", "--now", &spec.unit]);
    }
    if let Some(file) = service_file(spec, os) {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    if os == "linux" {
        let _ = run("systemctl", &["--user", "daemon-reload"]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ServiceSpec {
        spec(
            PathBuf::from("/home/me/.hbots/runtime/current/hbots"),
            Path::new("/home/me/.hbots"),
            "/opt/bin:/usr/bin:/opt/bin",
        )
    }

    #[test]
    fn the_path_keeps_the_users_entries_once_and_adds_the_system_ones() {
        let s = sample();
        assert_eq!(s.path, "/opt/bin:/usr/bin:/usr/local/bin:/bin");
        assert_eq!(s.log, PathBuf::from("/home/me/.hbots/logs/gateway.log"));
        let env = environment(&s);
        assert!(env.contains(&("HBOTS_SERVICE".into(), "1".into())));
        assert!(env.contains(&("HBOTS_HOME".into(), "/home/me/.hbots".into())));
    }

    #[test]
    fn the_plist_runs_serve_from_the_current_link_and_escapes_values() {
        let text = launchd_plist(&sample());
        assert!(text.contains("<string>hbots</string>"));
        assert!(text.contains(
            "<string>/home/me/.hbots/runtime/current/hbots</string>\n      <string>serve</string>"
        ));
        assert!(text.contains("<key>KeepAlive</key><true/>"));
        assert!(text.contains(
            "<key>StandardErrorPath</key><string>/home/me/.hbots/logs/gateway.log</string>"
        ));
        let odd = spec(PathBuf::from("/a&b/<hbots>"), Path::new("/h"), "/usr/bin");
        let text = launchd_plist(&odd);
        assert!(text.contains("/a&amp;b/&lt;hbots&gt;"), "{text}");
        assert!(!text.contains("/a&b"));
    }

    #[test]
    fn the_unit_quotes_words_with_spaces() {
        let text = systemd_unit(&sample());
        assert!(
            text.contains("ExecStart=/home/me/.hbots/runtime/current/hbots serve\n"),
            "{text}"
        );
        assert!(text.contains("Environment=PATH=/opt/bin:/usr/bin:/usr/local/bin:/bin\n"));
        assert!(text.contains("Environment=HBOTS_SERVICE=1\n"));
        assert!(text.contains("Restart=always\n") && text.contains("WantedBy=default.target"));
        let spaced = spec(PathBuf::from("/My Apps/hbots"), Path::new("/h"), "/usr/bin");
        assert!(systemd_unit(&spaced).contains("ExecStart=\"/My Apps/hbots\" serve\n"));
    }

    #[test]
    fn service_files_follow_each_system() {
        let s = sample();
        let mac = service_file(&s, "macos").unwrap();
        assert!(mac.ends_with("Library/LaunchAgents/hbots.plist"), "{mac:?}");
        let linux = service_file(&s, "linux").unwrap();
        assert!(linux.ends_with("systemd/user/hbots.service"), "{linux:?}");
        assert!(service_file(&s, "windows").is_none());
        assert!(supported("linux") && supported("macos") && !supported("windows"));
    }

    #[test]
    fn install_refuses_unsupported_systems_before_writing_anything() {
        let dir = tempfile::tempdir().unwrap();
        let s = spec(PathBuf::from("/x/hbots"), dir.path(), "/usr/bin");
        assert!(install(&s, "windows").is_err());
    }
}
