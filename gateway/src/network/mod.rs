//! Which machines may open the gateway.
//!
//! `local` listens on loopback. `lan` listens on every IPv4 interface. `tailscale` keeps the
//! public listener on loopback and uses the existing Tailscale Serve route. A saved choice is
//! applied the next time the gateway starts.
//!
//! `HBOTS_NETWORK_ACCESS` (`local`, `lan`, or `tailscale`) wins and locks Settings. A `HOST`
//! other than the loopback value `hbots up` writes is the older bind address and also locks
//! Settings. `HOST=127.0.0.1` does not.

use std::net::{IpAddr, Ipv4Addr, UdpSocket};

pub(crate) const PASSWORD_REQUIRED: &str =
    "Set a password before opening hbots beyond this device.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Local,
    Lan,
    Tailscale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Environment,
    Host,
    Settings,
    Default,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Input {
    pub network_env: Option<String>,
    pub process_host: Option<String>,
    pub merged_host: Option<String>,
    pub saved: Option<String>,
    pub tailscale_enabled: bool,
    pub env_tailnet: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub mode: Mode,
    pub source: Source,
    pub locked: bool,
    pub bind: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Applied {
    pub bind: String,
    pub error: Option<String>,
    /// The requested mode would listen beyond this device, and no password is set.
    pub withheld: bool,
}

pub(crate) fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Local => "local",
        Mode::Lan => "lan",
        Mode::Tailscale => "tailscale",
    }
}

pub(crate) fn parse_mode(value: &str) -> Option<Mode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "local" => Some(Mode::Local),
        "lan" => Some(Mode::Lan),
        "tailscale" => Some(Mode::Tailscale),
        _ => None,
    }
}

pub(crate) fn resolve(input: &Input) -> Choice {
    if let Some(value) = input
        .network_env
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return match parse_mode(value) {
            Some(mode) => choice(mode, Source::Environment, true, None),
            None => Choice {
                mode: Mode::Local,
                source: Source::Environment,
                locked: true,
                bind: "127.0.0.1".into(),
                error: Some(format!(
                    "HBOTS_NETWORK_ACCESS must be local, lan, or tailscale (got {}). hbots is listening on this device only.",
                    clip(value)
                )),
            },
        };
    }
    if let Some(host) = explicit_host(input.process_host.as_deref(), input.merged_host.as_deref()) {
        let mode = Mode::Lan;
        let bind = match host_kind(&host) {
            HostKind::Wildcard => "0.0.0.0".into(),
            HostKind::Specific => host,
            HostKind::Loopback | HostKind::Empty => "127.0.0.1".into(),
        };
        return Choice {
            mode,
            source: Source::Host,
            locked: true,
            bind,
            error: None,
        };
    }
    if input.env_tailnet {
        return choice(Mode::Tailscale, Source::Environment, true, None);
    }
    if let Some(value) = input
        .saved
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return match parse_mode(value) {
            Some(mode) => choice(mode, Source::Settings, false, None),
            None => Choice {
                mode: Mode::Local,
                source: Source::Settings,
                locked: false,
                bind: "127.0.0.1".into(),
                error: Some(
                    "The saved network access is not local, lan, or tailscale. hbots is listening on this device only."
                        .into(),
                ),
            },
        };
    }
    if input.tailscale_enabled {
        return choice(Mode::Tailscale, Source::Settings, false, None);
    }
    choice(Mode::Local, Source::Default, false, None)
}

/// Refuse a non-local mode when no password is set. The debug `--lan` switch is the exception:
/// it sets `HOST` and `HBOTS_DEV_ALLOW_ANY_ORIGIN`, and release builds ignore that switch.
pub(crate) fn apply(choice: &Choice, has_password: bool, dev_open: bool) -> Applied {
    let remote = choice.mode != Mode::Local;
    let dev_exception = dev_open && choice.source == Source::Host;
    if remote && !has_password && !dev_exception {
        return Applied {
            bind: "127.0.0.1".into(),
            error: Some(PASSWORD_REQUIRED.into()),
            withheld: true,
        };
    }
    Applied {
        bind: choice.bind.clone(),
        error: choice.error.clone(),
        withheld: false,
    }
}

pub(crate) fn reachable_urls(bind: &str, port: u16) -> Vec<String> {
    match host_kind(bind) {
        HostKind::Loopback | HostKind::Empty => vec![format!("http://127.0.0.1:{port}")],
        HostKind::Wildcard => primary_ipv4()
            .map(|ip| vec![format!("http://{ip}:{port}")])
            .unwrap_or_default(),
        HostKind::Specific => vec![format!("http://{}:{port}", bracket(bind.trim()))],
    }
}

fn choice(mode: Mode, source: Source, locked: bool, error: Option<String>) -> Choice {
    Choice {
        mode,
        source,
        locked,
        bind: match mode {
            Mode::Lan => "0.0.0.0".into(),
            Mode::Local | Mode::Tailscale => "127.0.0.1".into(),
        },
        error,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HostKind {
    Empty,
    Loopback,
    Wildcard,
    Specific,
}

fn host_kind(host: &str) -> HostKind {
    match host
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase()
        .as_str()
    {
        "" => HostKind::Empty,
        "127.0.0.1" | "localhost" | "::1" => HostKind::Loopback,
        "0.0.0.0" | "::" | "*" => HostKind::Wildcard,
        _ => HostKind::Specific,
    }
}

/// A process `HOST`, or a file `HOST`, when it is not the loopback default `hbots up` writes.
fn explicit_host(process: Option<&str>, merged: Option<&str>) -> Option<String> {
    [process, merged]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|host| matches!(host_kind(host), HostKind::Wildcard | HostKind::Specific))
        .map(str::to_string)
}

fn clip(value: &str) -> String {
    let mut chars = value.chars();
    let head: String = chars.by_ref().take(40).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

fn bracket(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

/// The IPv4 address this machine would use to reach another network, if the routing table has one.
fn primary_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if usable_v4(ip) => Some(ip),
        _ => None,
    }
}

fn usable_v4(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    !ip.is_loopback() && !ip.is_unspecified() && !(a == 169 && b == 254)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> Input {
        Input {
            network_env: None,
            process_host: None,
            merged_host: Some("127.0.0.1".into()),
            saved: None,
            tailscale_enabled: false,
            env_tailnet: false,
        }
    }

    #[test]
    fn the_default_and_the_written_loopback_host_stay_on_this_device() {
        let choice = resolve(&input());
        assert_eq!(choice.mode, Mode::Local);
        assert!(!choice.locked);
        assert_eq!(choice.bind, "127.0.0.1");
        assert_eq!(choice.source, Source::Default);
    }

    #[test]
    fn a_saved_mode_is_used_when_the_environment_does_not_set_one() {
        let mut lan = input();
        lan.saved = Some("lan".into());
        assert_eq!(resolve(&lan).mode, Mode::Lan);
        assert_eq!(resolve(&lan).bind, "0.0.0.0");
        assert!(!resolve(&lan).locked);

        let mut tail = input();
        tail.saved = Some("tailscale".into());
        assert_eq!(resolve(&tail).mode, Mode::Tailscale);
        assert_eq!(resolve(&tail).bind, "127.0.0.1");
    }

    #[test]
    fn an_enabled_tailscale_route_is_the_mode_until_a_choice_is_saved() {
        let mut saved = input();
        saved.tailscale_enabled = true;
        let choice = resolve(&saved);
        assert_eq!(choice.mode, Mode::Tailscale);
        assert_eq!(choice.source, Source::Settings);
        assert!(!choice.locked);
    }

    #[test]
    fn the_environment_variable_locks_the_choice_and_rejects_a_bad_value() {
        let mut locked = input();
        locked.network_env = Some("LAN".into());
        locked.saved = Some("local".into());
        locked.merged_host = Some("0.0.0.0".into());
        let choice = resolve(&locked);
        assert_eq!(choice.mode, Mode::Lan);
        assert!(choice.locked);
        assert_eq!(choice.source, Source::Environment);

        let mut bad = input();
        bad.network_env = Some("vpn".into());
        let choice = resolve(&bad);
        assert_eq!(choice.mode, Mode::Local);
        assert_eq!(choice.bind, "127.0.0.1");
        assert!(choice.locked);
        assert!(choice.error.unwrap().contains("HBOTS_NETWORK_ACCESS"));
    }

    #[test]
    fn a_custom_host_locks_and_the_default_host_does_not() {
        let mut wide = input();
        wide.merged_host = Some("0.0.0.0".into());
        wide.saved = Some("local".into());
        let choice = resolve(&wide);
        assert_eq!(choice.mode, Mode::Lan);
        assert!(choice.locked);
        assert_eq!(choice.source, Source::Host);

        let mut one = input();
        one.process_host = Some("10.1.2.3".into());
        let choice = resolve(&one);
        assert_eq!(choice.bind, "10.1.2.3");
        assert!(choice.locked);

        let mut stock = input();
        stock.process_host = Some("127.0.0.1".into());
        stock.saved = Some("lan".into());
        let choice = resolve(&stock);
        assert_eq!(choice.mode, Mode::Lan);
        assert!(!choice.locked);
    }

    #[test]
    fn an_env_tailnet_locks_tailscale_unless_a_newer_setting_wins() {
        let mut env = input();
        env.env_tailnet = true;
        assert_eq!(resolve(&env).mode, Mode::Tailscale);
        assert!(resolve(&env).locked);

        let mut newer = input();
        newer.env_tailnet = true;
        newer.network_env = Some("local".into());
        assert_eq!(resolve(&newer).mode, Mode::Local);
    }

    #[test]
    fn a_remote_mode_without_a_password_stays_on_loopback() {
        let mut lan = input();
        lan.saved = Some("lan".into());
        let choice = resolve(&lan);
        let applied = apply(&choice, false, false);
        assert!(applied.withheld);
        assert_eq!(applied.bind, "127.0.0.1");

        let open = apply(&choice, true, false);
        assert!(!open.withheld);
        assert_eq!(open.bind, "0.0.0.0");

        let mut host = input();
        host.merged_host = Some("0.0.0.0".into());
        let dev = apply(&resolve(&host), false, true);
        assert!(!dev.withheld);
        assert_eq!(dev.bind, "0.0.0.0");

        let mut named = input();
        named.network_env = Some("lan".into());
        let still = apply(&resolve(&named), false, true);
        assert!(still.withheld);
    }

    #[test]
    fn urls_use_the_loopback_or_the_specific_host() {
        assert_eq!(
            reachable_urls("127.0.0.1", 3780),
            vec!["http://127.0.0.1:3780".to_string()]
        );
        assert_eq!(
            reachable_urls("10.1.2.3", 3780),
            vec!["http://10.1.2.3:3780".to_string()]
        );
        assert_eq!(
            reachable_urls("fd00::1", 3780),
            vec!["http://[fd00::1]:3780".to_string()]
        );
    }
}

pub(crate) mod access; // T4: Cloudflare Access JWTs.
pub(crate) mod auth;
pub(crate) mod exposure; // T4: Tailscale listener and the publish commands.
pub(crate) mod ratelimit;
pub(crate) mod remote_access; // T4: the scrypt password in access.json.
