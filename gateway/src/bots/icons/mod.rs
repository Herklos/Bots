//! Public site icons, cached under `HBOTS_HOME/icons`.
//!
//! A signed-in client asks for one hostname. The gateway fetches `https://<host>/favicon.ico`,
//! or the `<link rel="icon">` on that same host, and keeps the bytes. Private, loopback and
//! link-local addresses are refused, and redirects are not followed, so this is not an open
//! proxy onto the machine's own network.

use crate::platform::error::{ApiError, ApiResult};
use crate::AppState;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::PathBuf,
    time::Duration,
};
use url::Url;

#[cfg(test)]
mod tests;

const MAX_ICON: usize = 256 * 1024;

const MAX_HTML: usize = 64 * 1024;

const CACHE_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug)]
pub struct Icons {
    dir: PathBuf,
    client: reqwest::Client,
}

impl Icons {
    pub fn new(dir: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("hbots-icons")
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { dir, client }
    }

    /// The cached icon, when it is still inside the week-long window.
    pub async fn cached(&self, host: &str) -> Option<(Vec<u8>, String)> {
        let name = cache_name(host)?;
        let bytes = tokio::fs::read(self.dir.join(&name)).await.ok()?;
        let meta = tokio::fs::read_to_string(self.dir.join(format!("{name}.type")))
            .await
            .ok()?;
        let modified = tokio::fs::metadata(self.dir.join(&name))
            .await
            .ok()?
            .modified()
            .ok()?;
        if modified.elapsed().unwrap_or(CACHE_FOR) >= CACHE_FOR {
            return None;
        }
        let ctype = meta.trim();
        if image_type(ctype, &bytes).is_none() || bytes.len() > MAX_ICON {
            return None;
        }
        Some((bytes, ctype.to_string()))
    }

    pub async fn store(&self, host: &str, bytes: &[u8], ctype: &str) -> std::io::Result<()> {
        let Some(name) = cache_name(host) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "bad host",
            ));
        };
        tokio::fs::create_dir_all(&self.dir).await?;
        crate::storage::write_atomic(&self.dir.join(&name), bytes, Some(0o644)).await?;
        crate::storage::write_atomic(
            &self.dir.join(format!("{name}.type")),
            ctype.as_bytes(),
            Some(0o644),
        )
        .await
    }

    /// Resolve `host`, refuse anything that is not a public address, then fetch and cache.
    pub async fn fetch_public(&self, host: &str) -> ApiResult<(Vec<u8>, String)> {
        let host = normalize_host(host)?;
        if let Some(hit) = self.cached(&host).await {
            return Ok(hit);
        }
        assert_public(&host).await?;
        #[allow(
            clippy::map_err_ignore,
            reason = "the URL parser echoes the host the client sent"
        )]
        let origin = Url::parse(&format!("https://{host}/"))
            .map_err(|_| ApiError::bad_request("bad host"))?;
        let (bytes, ctype) = load_site(&self.client, &origin).await?;
        self.store(&host, &bytes, &ctype).await?;
        Ok((bytes, ctype))
    }
}

pub(crate) async fn get_icon(
    State(s): State<std::sync::Arc<AppState>>,
    Path(host): Path<String>,
) -> ApiResult<Response> {
    let (bytes, ctype) = s.icons.fetch_public(&host).await?;
    s.usage.note(crate::bots::usage::Event::IconFetch).await;
    #[allow(
        clippy::map_err_ignore,
        reason = "HeaderValue echoes the icon type; a bad stored type is a 500 for the client"
    )]
    let value = HeaderValue::from_str(&ctype).map_err(|_| ApiError::new(500, "bad icon type"))?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, value),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, max-age=86400"),
            ),
            (
                header::HeaderName::from_static("x-content-type-options"),
                HeaderValue::from_static("nosniff"),
            ),
        ],
        Bytes::from(bytes),
    )
        .into_response())
}

/// A hostname this cache will store. Dots are fine; slashes, spaces and `..` are not.
pub(crate) fn cache_name(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty()
        || host.len() > 253
        || host.starts_with('.')
        || host.contains("..")
        || !host.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-' || b == b':'
        })
    {
        return None;
    }
    if host.contains(':') && host.parse::<Ipv6Addr>().is_err() {
        return None;
    }
    Some(host)
}

fn normalize_host(host: &str) -> ApiResult<String> {
    cache_name(host).ok_or_else(|| ApiError::bad_request("give a public hostname"))
}

pub(crate) fn ip_is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => ipv4_is_public(v),
        IpAddr::V6(v) => ipv6_is_public(v),
    }
}

fn ipv4_is_public(v: Ipv4Addr) -> bool {
    let [a, b, _, _] = v.octets();
    if v.is_private()
        || v.is_loopback()
        || v.is_link_local()
        || v.is_broadcast()
        || v.is_documentation()
        || v.is_unspecified()
        || v.is_multicast()
        || a == 0
        || a >= 224
    {
        return false;
    }
    // 100.64.0.0/10 carrier-grade NAT, and 198.18.0.0/15 benchmark space.
    if a == 100 && (b & 0xc0) == 0x40 {
        return false;
    }
    if a == 198 && (b == 18 || b == 19) {
        return false;
    }
    true
}

fn ipv6_is_public(v: Ipv6Addr) -> bool {
    !v.is_loopback()
        && !v.is_unspecified()
        && !v.is_unicast_link_local()
        && !v.is_unique_local()
        && !v.is_multicast()
        && !matches!(v.segments()[0], 0x2001 if v.segments()[1] == 0x0db8)
}

/// Every address the name resolves to must be public. One private answer refuses the host.
pub(crate) async fn assert_public(host: &str) -> ApiResult<()> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        if ip_is_public(ip) {
            return Ok(());
        }
        return Err(ApiError::bad_request("that host is not a public site"));
    }
    let suffix = host.rsplit('.').next().unwrap_or("");
    if matches!(suffix, "local" | "localhost" | "internal" | "arpa" | "home") || host == "localhost"
    {
        return Err(ApiError::bad_request("that host is not a public site"));
    }
    #[allow(
        clippy::map_err_ignore,
        reason = "a DNS error can include the resolver's detail for a host the client named"
    )]
    let looked = tokio::net::lookup_host((host, 443))
        .await
        .map_err(|_| ApiError::new(502, "could not resolve that host"))?;
    let mut any = false;
    for addr in looked {
        any = true;
        if !ip_is_public(addr.ip()) {
            return Err(ApiError::bad_request("that host is not a public site"));
        }
    }
    if !any {
        return Err(ApiError::new(502, "could not resolve that host"));
    }
    Ok(())
}

/// The first `<link rel="…icon…">` href, quotes either way.
pub(crate) fn icon_href(html: &str) -> Option<&str> {
    let mut rest = html;
    while let Some(at) = rest.to_ascii_lowercase().find("<link") {
        rest = &rest[at + 5..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        rest = &rest[end..];
        if rel_is_icon(tag) {
            if let Some(href) = attr(tag, "href") {
                if !href.is_empty() {
                    return Some(href);
                }
            }
        }
    }
    None
}

fn rel_is_icon(tag: &str) -> bool {
    attr(tag, "rel")
        .map(|rel| {
            rel.split_whitespace().any(|part| {
                part.eq_ignore_ascii_case("icon") || part.eq_ignore_ascii_case("shortcut")
            })
        })
        .unwrap_or(false)
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let key = format!("{name}=");
    let at = lower.find(&key)?;
    let after = &tag[at + key.len()..];
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value = &after[quote.len_utf8()..];
    let end = value.find(quote)?;
    Some(&value[..end])
}

pub(crate) fn image_type(header: &str, bytes: &[u8]) -> Option<&'static str> {
    let header = header
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let from_header = match header.as_str() {
        "image/png" => Some("image/png"),
        "image/jpeg" | "image/jpg" => Some("image/jpeg"),
        "image/gif" => Some("image/gif"),
        "image/webp" => Some("image/webp"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some("image/x-icon"),
        _ => None,
    };
    if let Some(kind) = from_header {
        return Some(kind);
    }
    // An HTML error page is not an icon, even when some later bytes look like one.
    if matches!(
        header.as_str(),
        "text/html" | "text/plain" | "application/json" | "application/xhtml+xml"
    ) {
        return None;
    }
    sniff(bytes)
}

fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        Some("image/x-icon")
    } else {
        None
    }
}

/// `favicon.ico` on `origin`, else the icon linked from the page. The link must stay on that host.
pub(crate) async fn load_site(
    client: &reqwest::Client,
    origin: &Url,
) -> ApiResult<(Vec<u8>, String)> {
    if origin.scheme() != "https" && origin.scheme() != "http" {
        return Err(ApiError::bad_request("the site must be http or https"));
    }
    #[allow(
        clippy::map_err_ignore,
        reason = "the URL parser echoes the host or the page's icon link"
    )]
    let favicon = origin
        .join("favicon.ico")
        .map_err(|_| ApiError::bad_request("bad host"))?;
    if let Ok((bytes, ctype)) = pull(client, &favicon, MAX_ICON).await {
        if let Some(kind) = image_type(&ctype, &bytes) {
            return Ok((bytes, kind.to_string()));
        }
    }
    let (page, _) = pull(client, origin, MAX_HTML).await?;
    let page_text = String::from_utf8_lossy(&page);
    let href = icon_href(&page_text)
        .ok_or_else(|| ApiError::new(502, "the site has no icon"))?
        .to_string();
    if href.starts_with("data:") || href.starts_with("javascript:") {
        return Err(ApiError::new(502, "the site has no icon"));
    }
    #[allow(
        clippy::map_err_ignore,
        reason = "the URL parser echoes the host or the page's icon link"
    )]
    let icon_url = origin
        .join(&href)
        .map_err(|_| ApiError::new(502, "the site has no icon"))?;
    if icon_url.scheme() != "https" && icon_url.scheme() != "http" {
        return Err(ApiError::new(502, "the site has no icon"));
    }
    if icon_url.host_str() != origin.host_str() {
        return Err(ApiError::bad_request("the icon is on a different host"));
    }
    let (bytes, ctype) = pull(client, &icon_url, MAX_ICON).await?;
    let kind =
        image_type(&ctype, &bytes).ok_or_else(|| ApiError::new(502, "the site has no icon"))?;
    Ok((bytes, kind.to_string()))
}

async fn pull(client: &reqwest::Client, url: &Url, limit: usize) -> ApiResult<(Vec<u8>, String)> {
    let res = client.get(url.clone()).send().await.map_err(|err| {
        tracing::info!("icon fetch failed: {err}");
        ApiError::new(502, "the site did not answer")
    })?;
    if !res.status().is_success() {
        return Err(ApiError::new(502, "the site did not return an icon"));
    }
    let ctype = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    #[allow(
        clippy::map_err_ignore,
        reason = "a body error can include the site URL; the client only hears that the site did not answer"
    )]
    let bytes = res
        .bytes()
        .await
        .map_err(|_| ApiError::new(502, "the site did not answer"))?;
    if bytes.len() > limit {
        return Err(ApiError::new(502, "the icon is too large"));
    }
    Ok((bytes.to_vec(), ctype))
}
