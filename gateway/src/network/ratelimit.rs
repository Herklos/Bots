//! In-memory limits for the public webhook ingress.
//!
//! Each client address gets 120 requests a minute, with room for a burst of 30.
//! A shared bucket stops many addresses from multiplying that burst. The clock
//! is injectable so a test can trip the limit without waiting.

use axum::http::HeaderMap;
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

pub(crate) const PER_IP_PER_MINUTE: u32 = 120;
pub(crate) const PER_IP_BURST: u32 = 30;
/// Five times one address, so a single sender still hits its own burst first.
pub(crate) const GLOBAL_PER_MINUTE: u32 = 600;
pub(crate) const GLOBAL_BURST: u32 = 120;

const MAX_ADDRESSES: usize = 4096;

#[derive(Debug)]
struct Bucket {
    tokens: u32,
    updated: Instant,
}

impl Bucket {
    fn full(now: Instant, burst: u32) -> Self {
        Self {
            tokens: burst,
            updated: now,
        }
    }

    fn refill(&mut self, now: Instant, per_minute: u32, burst: u32) {
        let elapsed_ms = now.saturating_duration_since(self.updated).as_millis();
        let add = credit(elapsed_ms, per_minute);
        if add == 0 {
            return;
        }
        self.tokens = self.tokens.saturating_add(add).min(burst);
        let consumed = u128::from(add).saturating_mul(60_000) / u128::from(per_minute);
        let consumed = u64::try_from(consumed).unwrap_or(u64::MAX);
        self.updated += Duration::from_millis(consumed);
    }

    fn take(&mut self, now: Instant, per_minute: u32, burst: u32) -> bool {
        self.refill(now, per_minute, burst);
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }
}

fn credit(elapsed_ms: u128, per_minute: u32) -> u32 {
    let add = elapsed_ms.saturating_mul(u128::from(per_minute)) / 60_000;
    u32::try_from(add).unwrap_or(u32::MAX)
}

#[derive(Debug)]
struct Inner {
    ips: HashMap<String, Bucket>,
    global: Bucket,
}

/// Per-address and global token buckets. `allow` is the live clock.
#[derive(Debug)]
pub struct Limiter {
    inner: Mutex<Inner>,
}

impl Limiter {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                ips: HashMap::new(),
                global: Bucket::full(Instant::now(), GLOBAL_BURST),
            }),
        }
    }

    pub fn allow(&self, ip: &str) -> bool {
        self.allow_at(ip, Instant::now())
    }

    pub(crate) fn allow_at(&self, ip: &str, now: Instant) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        if inner.ips.len() >= MAX_ADDRESSES && !inner.ips.contains_key(ip) {
            inner.ips.retain(|_, bucket| bucket.tokens < PER_IP_BURST);
            if inner.ips.len() >= MAX_ADDRESSES {
                inner.ips.clear();
            }
        }
        let ip_bucket = inner
            .ips
            .entry(ip.to_string())
            .or_insert_with(|| Bucket::full(now, PER_IP_BURST));
        if !ip_bucket.take(now, PER_IP_PER_MINUTE, PER_IP_BURST) {
            return false;
        }
        if inner.global.take(now, GLOBAL_PER_MINUTE, GLOBAL_BURST) {
            return true;
        }
        if let Some(bucket) = inner.ips.get_mut(ip) {
            bucket.tokens = bucket.tokens.saturating_add(1).min(PER_IP_BURST);
        }
        false
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Last `x-forwarded-for` hop, otherwise the peer. Same rule as sign-in.
pub(crate) fn client_ip(headers: &HeaderMap, peer: IpAddr) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| peer.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_address_bursts_then_stops_and_another_address_does_not() {
        assert_eq!(PER_IP_PER_MINUTE, 120);
        assert_eq!(PER_IP_BURST, 30);
        let limiter = Limiter::new();
        let now = Instant::now();
        for _ in 0..PER_IP_BURST {
            assert!(limiter.allow_at("203.0.113.4", now));
        }
        assert!(!limiter.allow_at("203.0.113.4", now));
        assert!(limiter.allow_at("203.0.113.5", now));
        assert!(limiter.allow_at("203.0.113.4", now + Duration::from_secs(60)));
    }

    #[test]
    fn the_global_bucket_stops_many_addresses() {
        assert_eq!(GLOBAL_PER_MINUTE, 600);
        assert_eq!(GLOBAL_BURST, 120);
        let limiter = Limiter::new();
        let now = Instant::now();
        let mut allowed = 0u32;
        for host in 1..=10 {
            let ip = format!("198.51.100.{host}");
            for _ in 0..PER_IP_BURST {
                if limiter.allow_at(&ip, now) {
                    allowed += 1;
                }
            }
        }
        assert_eq!(allowed, GLOBAL_BURST);
    }

    #[test]
    fn the_client_address_is_the_last_forwarded_hop() {
        let mut headers = HeaderMap::new();
        let peer = "127.0.0.1".parse().unwrap();
        assert_eq!(client_ip(&headers, peer), "127.0.0.1");
        headers.insert("x-forwarded-for", "1.1.1.1, 203.0.113.9".parse().unwrap());
        assert_eq!(client_ip(&headers, peer), "203.0.113.9");
        headers.insert("x-forwarded-for", "   ".parse().unwrap());
        assert_eq!(client_ip(&headers, peer), "127.0.0.1");
    }
}
