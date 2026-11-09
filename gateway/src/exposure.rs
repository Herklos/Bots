//! How the gateway is reached from outside the machine.
//!
//! Tailscale gets its own listener on `PORT + 1`. That listener trusts `Tailscale-User-Login`
//! and nothing else: the public port strips the internal mark, so a client cannot pretend.
//! `landing` lets that identity open the front door only. `full` lets it open the API.
//! `tailscale serve` and the `cloudflared` quick tunnel are the two ways to publish the port.
use serde_json::{json, Value};
use std::process::{Child, Command};
use std::sync::Mutex;

pub const TAILSCALE_USER: &str = "tailscale-user-login";
/// Set only by the Tailscale listener, after the public listener has stripped any client copy.
pub const MARK: &str = "x-hbots-via-tailscale";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exposure {
    Off,
    Landing,
    Full,
}

impl Exposure {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "landing" => Self::Landing,
            "full" => Self::Full,
            _ => Self::Off,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Landing => "landing",
            Self::Full => "full",
        }
    }
}

pub fn tailscale_port(port: u16) -> u16 {
    port.saturating_add(1)
}

/// `login` is the `Tailscale-User-Login` header. Landing opens the pages the app needs to
/// draw its front door and to sign in; everything else still needs a session.
pub fn grants(mode: Exposure, path: &str, login: Option<&str>) -> bool {
    if login.map(str::trim).filter(|s| !s.is_empty()).is_none() {
        return false;
    }
    match mode {
        Exposure::Off => false,
        Exposure::Full => true,
        Exposure::Landing => matches!(
            path,
            "/" | "/index.html" | "/api/health" | "/api/session" | "/api/login" | "/api/logout"
        ),
    }
}

pub fn tailscale_serve(port: u16) -> Vec<String> {
    vec![
        "tailscale".into(),
        "serve".into(),
        "--bg".into(),
        "--yes".into(),
        port.to_string(),
    ]
}

pub fn cloudflared_tunnel(port: u16) -> Vec<String> {
    vec![
        "cloudflared".into(),
        "tunnel".into(),
        "--url".into(),
        format!("http://127.0.0.1:{port}"),
    ]
}

/// One published process at a time. A new one replaces the previous.
pub struct Tunnels {
    child: Mutex<Option<Child>>,
}

impl Tunnels {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
        }
    }

    pub fn start(&self, argv: &[String]) -> Result<(), String> {
        let (bin, args) = argv.split_first().ok_or_else(|| "no command".to_string())?;
        let child = Command::new(bin)
            .args(args)
            .spawn()
            .map_err(|e| format!("{bin} did not start: {e}"))?;
        let mut slot = self.child.lock().unwrap();
        if let Some(old) = slot.as_mut() {
            let _ = old.kill();
            let _ = old.wait();
        }
        *slot = Some(child);
        Ok(())
    }
}

impl Default for Tunnels {
    fn default() -> Self {
        Self::new()
    }
}

pub fn status(mode: Exposure, port: u16, password_set: bool) -> Value {
    json!({
        "exposure": mode.as_str(),
        "tailscalePort": tailscale_port(port),
        "passwordSet": password_set,
        "tailscale": tailscale_serve(port),
        "tunnel": cloudflared_tunnel(port),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landing_opens_the_front_door_and_full_opens_the_api() {
        assert!(!grants(Exposure::Full, "/api/bots", None));
        assert!(!grants(Exposure::Full, "/api/bots", Some("  ")));
        assert!(!grants(Exposure::Off, "/", Some("ada@example.com")));
        assert!(grants(Exposure::Landing, "/", Some("ada@example.com")));
        assert!(grants(
            Exposure::Landing,
            "/api/login",
            Some("ada@example.com")
        ));
        assert!(!grants(
            Exposure::Landing,
            "/api/bots",
            Some("ada@example.com")
        ));
        assert!(grants(Exposure::Full, "/api/bots", Some("ada@example.com")));
        assert_eq!(tailscale_port(3780), 3781);
        assert_eq!(tailscale_port(u16::MAX), u16::MAX);
    }

    #[test]
    fn the_publish_commands_name_the_port() {
        assert_eq!(tailscale_serve(3780)[0], "tailscale");
        assert!(tailscale_serve(3780).iter().any(|a| a == "3780"));
        assert!(cloudflared_tunnel(3780)
            .iter()
            .any(|a| a == "http://127.0.0.1:3780"));
    }
}
