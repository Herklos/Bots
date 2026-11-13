//! The SSRF guard: refused ranges, the preflight resolve, and redirect rules.

use super::super::*;
use std::net::IpAddr;

fn refused_addr(text: &str) -> bool {
    guard::refused(&text.parse::<IpAddr>().unwrap()).is_some()
}

#[test]
fn the_guard_refuses_private_and_loopback_ranges() {
    for text in [
        "127.0.0.1",
        "0.0.0.0",
        "10.0.0.1",
        "192.168.1.1",
        "172.16.0.1",
        "172.31.255.255",
        "169.254.169.254",
        "169.254.0.7",
        "100.64.0.1",
        "100.127.255.254",
        "192.0.2.1",
        "::1",
        "fe80::1",
        "fc00::1",
        "fd12::1",
        "ff02::1",
        "::",
        "::ffff:127.0.0.1",
    ] {
        assert!(refused_addr(text), "{text} is refused");
    }
    for text in [
        "8.8.8.8",
        "1.1.1.1",
        "93.184.216.34",
        "2606:4700:4700::1111",
    ] {
        assert!(!refused_addr(text), "{text} is a public host");
    }
}

#[tokio::test]
async fn a_local_name_is_refused_without_allow_private() {
    let refused = guard::resolve_checked("localhost", 80, false).await;
    assert!(matches!(refused, Err(guard::Guard::Refused(_))));
    // allowPrivate permits this watch only; the next watch is refused again.
    assert!(guard::resolve_checked("localhost", 80, true).await.is_ok());
    let refused_again = guard::resolve_checked("localhost", 80, false).await;
    assert!(matches!(refused_again, Err(guard::Guard::Refused(_))));
}

#[tokio::test]
async fn a_public_literal_passes_the_preflight_without_dns() {
    let allowed = guard::resolve_checked("8.8.8.8", 80, false).await;
    assert!(allowed.is_ok(), "a literal public address needs no lookup");
}

#[test]
fn redirects_stay_on_http() {
    assert!(guard::redirect_target("GET", "https://example.com/a", "/b").is_ok());
    assert!(guard::redirect_target("HEAD", "https://example.com/a", "//other.host").is_ok());
    assert!(guard::redirect_target("GET", "https://example.com/a", "ftp://x/y").is_err());
}
