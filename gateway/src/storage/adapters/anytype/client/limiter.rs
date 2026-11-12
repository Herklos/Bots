//! Token bucket in front of every authenticated call. A 429 halves the refill rate.

use std::sync::Mutex;
use std::time::{Duration, Instant};

const MIN_RATE: f64 = 0.25;

#[derive(Debug)]
pub(super) struct Limiter {
    inner: Mutex<Bucket>,
}

#[derive(Debug)]
struct Bucket {
    rate: f64,
    burst: f64,
    tokens: f64,
    updated: Instant,
}

impl Limiter {
    pub(super) fn new(rate: f64, burst: f64) -> Self {
        let rate = rate.max(MIN_RATE);
        let burst = burst.max(1.0);
        Self {
            inner: Mutex::new(Bucket {
                rate,
                burst,
                tokens: burst,
                updated: Instant::now(),
            }),
        }
    }

    pub(super) fn rate(&self) -> f64 {
        self.inner.lock().expect("limiter").rate
    }

    /// Halve the refill rate and the burst. The floor stays above zero so a call can still leave.
    pub(super) fn shrink(&self) {
        let mut bucket = self.inner.lock().expect("limiter");
        bucket.refill();
        bucket.rate = (bucket.rate * 0.5).max(MIN_RATE);
        bucket.burst = (bucket.burst * 0.5).max(1.0);
        if bucket.tokens > bucket.burst {
            bucket.tokens = bucket.burst;
        }
    }

    pub(super) async fn acquire(&self) {
        loop {
            let wait = {
                let mut bucket = self.inner.lock().expect("limiter");
                bucket.refill();
                if bucket.tokens >= 1.0 {
                    bucket.tokens -= 1.0;
                    return;
                }
                let missing = 1.0 - bucket.tokens;
                Duration::from_secs_f64(missing / bucket.rate)
            };
            tokio::time::sleep(wait).await;
        }
    }
}

impl Bucket {
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.updated).as_secs_f64();
        self.updated = now;
        self.tokens = (self.tokens + elapsed * self.rate).min(self.burst);
    }
}
