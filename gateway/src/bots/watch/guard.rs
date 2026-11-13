//! The watch fetcher's SSRF guard (`docs/webhook-triggers-plan.md`, Phase 4).
//!
//! The host is resolved first, the address checked against the refused ranges, and the
//! request then pinned to that exact IP, so a DNS rebinding between the check and the
//! connect cannot swap in a private address. Every redirect hop is resolved and checked
//! again the same way. `allowPrivate`, set only from the app UI, skips the refusal for
//! that one watch.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const READ_CAP: usize = 1 << 20;
pub(crate) const MAX_REDIRECTS: usize = 3;
pub(crate) const USER_AGENT: &str = "hbots-watch";

#[derive(Debug)]
pub(crate) enum Guard {
    Refused(&'static str),
    Unresolved,
}

impl Guard {
    pub(crate) fn message(&self) -> String {
        match self {
            Guard::Refused(why) => format!("This watch's host is a {why} address."),
            Guard::Unresolved => "This watch's host did not resolve.".to_string(),
        }
    }
}

/// The refused ranges: loopback, link-local (which covers 169.254.169.254),
/// RFC 1918 private, CGNAT, unspecified, and multicast. IPv6 gets the same classes.
pub(crate) fn refused(ip: &IpAddr) -> Option<&'static str> {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            match o[0] {
                0 => Some("unspecified"),
                10 => Some("private"),
                100 if (64..=127).contains(&o[1]) => Some("CGNAT"),
                127 => Some("loopback"),
                169 if o[1] == 254 => Some("link-local"),
                172 if (16..=31).contains(&o[1]) => Some("private"),
                192 if o[1] == 0 && o[2] == 2 => Some("documentation"),
                192 if o[1] == 168 => Some("private"),
                _ => None,
            }
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return refused(&IpAddr::V4(mapped));
            }
            if v6.is_loopback() {
                return Some("loopback");
            }
            if v6.is_unspecified() {
                return Some("unspecified");
            }
            let first = v6.segments()[0];
            if first & 0xff00 == 0xff00 {
                return Some("multicast");
            }
            if first & 0xffc0 == 0xfe80 {
                return Some("link-local");
            }
            if first & 0xfe00 == 0xfc00 {
                return Some("unique-local");
            }
            if first == 0x2001 && v6.segments()[1] == 0x0db8 {
                return Some("documentation");
            }
            None
        }
    }
}

/// Resolve the host and check the first answer. `None` when the address passes, or
/// the watch carries `allowPrivate`.
pub(crate) async fn resolve_checked(
    host: &str,
    port: u16,
    allow_private: bool,
) -> Result<SocketAddr, Guard> {
    let mut addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_bad| Guard::Unresolved)?;
    let addr = addrs.next().ok_or(Guard::Unresolved)?;
    if !allow_private {
        if let Some(why) = refused(&addr.ip()) {
            return Err(Guard::Refused(why));
        }
    }
    Ok(addr)
}

/// One HTTP fetch, pinned per hop. GET or HEAD only. The body is capped at 1 MiB.
pub(crate) async fn fetch(url: &str, method: &str, allow_private: bool) -> Result<Outcome, String> {
    let mut current = url.to_string();
    let mut method = method.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let outcome = fetch_once(&current, &method, allow_private).await?;
        if let Some(next) = outcome.redirect {
            let (m, location) = redirect_target(&method, &current, &next)?;
            method = m;
            current = location;
            continue;
        }
        return Ok(outcome);
    }
    Err("Too many redirects.".to_string())
}

pub(crate) fn redirect_target(
    method: &str,
    base: &str,
    location: &str,
) -> Result<(String, String), String> {
    if !matches!(method, "GET" | "HEAD") {
        return Err("A watch only follows GET and HEAD redirects.".to_string());
    }
    let origin = url::Url::parse(base)
        .and_then(|b| b.join(location))
        .map_err(|_bad| "The redirect target does not parse.".to_string())?;
    if !matches!(origin.scheme(), "http" | "https") {
        return Err("A redirect must stay on http or https.".to_string());
    }
    Ok((method.to_string(), origin.to_string()))
}

pub(crate) struct Outcome {
    pub status: u16,
    pub body: Vec<u8>,
    pub redirect: Option<String>,
}

async fn fetch_once(url: &str, method: &str, allow_private: bool) -> Result<Outcome, String> {
    let parsed = url::Url::parse(url).map_err(|_bad| "That url does not parse.".to_string())?;
    let host = parsed
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or("The url has no host.")?
        .to_string();
    let port = parsed
        .port_or_known_default()
        .ok_or("The url's port is unknown.")?;
    // Resolve, refuse, then pin this hop to that exact IP.
    let pinned = resolve_checked(&host, port, allow_private)
        .await
        .map_err(|guard| guard.message())?;
    let client = reqwest::Client::builder()
        .resolve(&host, pinned)
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .build()
        .map_err(|_bad| "The watch's HTTP client did not build.")?;
    let request = match method {
        "HEAD" => client.head(url),
        _ => client.get(url),
    };
    let response = request
        .send()
        .await
        .map_err(|_bad| "The watch's host did not answer.")?;
    if let Some(location) = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
    {
        let code = response.status().as_u16();
        if (300..400).contains(&code) {
            return Ok(Outcome {
                status: code,
                body: Vec::new(),
                redirect: Some(location.to_string()),
            });
        }
    }
    let status = response.status().as_u16();
    let mut body = Vec::new();
    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_bad| "The watch's body did not finish.")?
    {
        if body.len() + chunk.len() > READ_CAP {
            body.truncate(READ_CAP);
            break;
        }
        body.extend_from_slice(&chunk);
        if body.len() == READ_CAP {
            break;
        }
    }
    Ok(Outcome {
        status,
        body,
        redirect: None,
    })
}
