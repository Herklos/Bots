//! Command line for the `hbots` binary.
//!
//! `serve` is the gateway. The Exact client is a separate build: a desktop app starts
//! `hbots serve` as a child, and a phone or a browser only runs the client.
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub enum Command {
    Serve,
    Help,
    Version,
    Status,
    Systemd,
    Launchd,
    Restart,
    Up,
    Rollback,
    Update {
        yes: bool,
        preview: bool,
        version: Option<String>,
    },
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    let mut rest = args.iter().map(String::as_str);
    match rest.next() {
        None | Some("serve") => {
            if rest.next().is_some() {
                return Err(format!("unexpected argument\n\n{}", help_text()));
            }
            Ok(Command::Serve)
        }
        Some("help" | "--help" | "-h") => Ok(Command::Help),
        Some("version" | "--version" | "-V") => Ok(Command::Version),
        Some("status") => Ok(Command::Status),
        Some("up") => Ok(Command::Up),
        Some("rollback") => Ok(Command::Rollback),
        Some("update") => {
            let mut yes = false;
            let mut preview = false;
            let mut version = None;
            for arg in rest {
                match arg {
                    "--yes" => yes = true,
                    "--preview" => preview = true,
                    other if other.starts_with('-') => {
                        return Err(format!("unknown flag {other}\n\n{}", help_text()));
                    }
                    other => version = Some(other.to_string()),
                }
            }
            Ok(Command::Update {
                yes,
                preview,
                version,
            })
        }
        Some("service") => match rest.next() {
            Some("systemd") => Ok(Command::Systemd),
            Some("launchd") => Ok(Command::Launchd),
            Some("restart") => Ok(Command::Restart),
            _ => Err(format!(
                "usage: hbots service systemd|launchd|restart\n\n{}",
                help_text()
            )),
        },
        Some(other) => Err(format!("unknown command {other}\n\n{}", help_text())),
    }
}

pub fn help_text() -> &'static str {
    "\
hbots — the gateway for the Exact client

  hbots serve       start the gateway (also the default)
  hbots up          start the gateway in the background
  hbots status      show the installed runtime links
  hbots update [--yes] [--preview] [version]
                    install a release; --yes switches current and rolls back
                    if the new one does not answer /api/health
  hbots rollback    point current back at the previous version
  hbots service systemd|launchd|restart
                    print a user service that runs `hbots serve`, or restart it
  hbots version     print the version
  hbots help        print this text

This binary is the server. The client is the Exact app in app/.
A VPS runs `hbots serve` alone. A desktop app starts that command as a child.
A phone or a browser runs the client only and reaches the gateway with HBOTS_URL.
"
}

/// Compare dotted versions. A leading `v` is ignored. Missing parts count as 0.
/// The release check (not in this binary yet) uses this order.
#[allow(dead_code)]
pub fn version_cmp(left: &str, right: &str) -> Ordering {
    let a = version_parts(left);
    let b = version_parts(right);
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

#[allow(dead_code)]
fn version_parts(raw: &str) -> Vec<u64> {
    raw.trim()
        .trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap_or(0))
        .collect()
}

pub fn home_dir() -> PathBuf {
    std::env::var("HBOTS_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".hbots"))
                .unwrap_or_else(|_| PathBuf::from(".hbots"))
        })
}

/// What `hbots status` prints. `current` and `previous` are the runtime links.
pub fn status_text(home: &Path) -> String {
    let runtime = home.join("runtime");
    let current = link_target(&runtime.join("current"));
    let previous = link_target(&runtime.join("previous"));
    format!(
        "home: {}\ncurrent: {}\nprevious: {}",
        home.display(),
        current,
        previous
    )
}

fn link_target(path: &Path) -> String {
    match path.read_link() {
        Ok(target) => target.display().to_string(),
        Err(_) if path.exists() => path.display().to_string(),
        Err(_) => "none".into(),
    }
}

pub fn systemd_unit(bin: &str) -> String {
    format!(
        "\
[Unit]
Description=hbots gateway

[Service]
ExecStart={bin} serve
Restart=on-failure
Environment=HBOTS_SERVICE=1

[Install]
WantedBy=default.target
"
    )
}

pub fn launchd_plist(bin: &str) -> String {
    format!(
        "\
<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<plist version=\"1.0\">
<dict>
  <key>Label</key>
  <string>hbots</string>
  <key>ProgramArguments</key>
  <array>
    <string>{bin}</string>
    <string>serve</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>EnvironmentVariables</key>
  <dict>
    <key>HBOTS_SERVICE</key>
    <string>1</string>
  </dict>
</dict>
</plist>
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn arg(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn commands() {
        assert!(matches!(parse(&[]).unwrap(), Command::Serve));
        assert!(matches!(parse(&[arg("serve")]).unwrap(), Command::Serve));
        assert!(matches!(parse(&[arg("help")]).unwrap(), Command::Help));
        assert!(matches!(
            parse(&[arg("--version")]).unwrap(),
            Command::Version
        ));
        assert!(matches!(parse(&[arg("status")]).unwrap(), Command::Status));
        assert!(matches!(
            parse(&[arg("service"), arg("systemd")]).unwrap(),
            Command::Systemd
        ));
        assert!(matches!(
            parse(&[arg("service"), arg("restart")]).unwrap(),
            Command::Restart
        ));
        assert!(matches!(parse(&[arg("up")]).unwrap(), Command::Up));
        assert!(matches!(
            parse(&[arg("rollback")]).unwrap(),
            Command::Rollback
        ));
        assert!(matches!(
            parse(&[arg("update"), arg("--yes"), arg("1.2.3")]).unwrap(),
            Command::Update {
                yes: true,
                preview: false,
                version: Some(_)
            }
        ));
        assert!(parse(&[arg("service")]).is_err());
        assert!(parse(&[arg("nope")]).is_err());
        assert!(parse(&[arg("serve"), arg("extra")]).is_err());
    }

    #[test]
    fn versions_compare_by_number() {
        assert_eq!(version_cmp("1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(version_cmp("v1.2", "1.2.0"), Ordering::Equal);
        assert_eq!(version_cmp("1.2.3", "1.10.0"), Ordering::Less);
        assert_eq!(version_cmp("2.0.0", "1.9.9"), Ordering::Greater);
    }

    #[test]
    fn status_reads_runtime_links() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("runtime");
        fs::create_dir_all(runtime.join("versions/0.2.0")).unwrap();
        std::os::unix::fs::symlink("versions/0.2.0", runtime.join("current")).unwrap();
        let text = status_text(dir.path());
        assert!(text.contains("current: versions/0.2.0"), "{text}");
        assert!(text.contains("previous: none"), "{text}");
    }

    #[test]
    fn service_files_run_serve() {
        let unit = systemd_unit("/usr/local/bin/hbots");
        assert!(unit.contains("ExecStart=/usr/local/bin/hbots serve"));
        assert!(unit.contains("WantedBy=default.target"));
        let plist = launchd_plist("/usr/local/bin/hbots");
        assert!(plist.contains("<string>/usr/local/bin/hbots</string>"));
        assert!(plist.contains("<string>serve</string>"));
        assert!(plist.contains("<key>Label</key>"));
    }
}
