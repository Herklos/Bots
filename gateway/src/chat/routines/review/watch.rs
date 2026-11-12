//! Holds a new routine a Bot created until the person approves it.
//!
//! The loop stays on the harness routine list, reconciles every full snapshot, and
//! reconnects with a pause of one second that doubles up to 30 seconds. A quiet
//! subscription is opened again after a minute so a dropped connection still gets
//! a full list.
//!
//! A routine set a few seconds ahead can still run once before it is disabled.
//! The harness has no catch-up policy and no skip-if-open flag, and this watcher
//! does not add either.
//!
//! The loop waits until the harness is connected. A harness with no routines is left alone.

use super::apply::{apply_snapshot, Apply};
use super::notice::NoticeFile;
use super::store::ReviewFile;
use crate::harness::HarnessError;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

pub(crate) const BACKOFF_MIN: Duration = Duration::from_secs(1);
pub(crate) const BACKOFF_MAX: Duration = Duration::from_secs(30);
pub(crate) const HEARTBEAT: Duration = Duration::from_secs(60);

pub(crate) fn next_backoff(current: Duration, max: Duration) -> Duration {
    current.saturating_mul(2).min(max)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Pace {
    pub min: Duration,
    pub max: Duration,
}

impl Pace {
    pub(crate) fn production() -> Self {
        Self {
            min: BACKOFF_MIN,
            max: BACKOFF_MAX,
        }
    }
}

pub(crate) type Feed = mpsc::Receiver<Result<Value, String>>;

#[derive(Debug)]
pub(crate) enum OpenFail {
    Unsupported,
    Unavailable(String),
}

#[derive(Debug)]
pub(crate) enum FeedEvent {
    Snapshot(Value),
    Ended,
    Stopped,
}

#[async_trait]
pub(crate) trait Host: Send + Sync {
    async fn supported(&self) -> bool;
    async fn stopped(&self) -> bool;
    async fn ready(&self) -> bool;
    async fn open(&self) -> Result<Feed, OpenFail>;
    async fn next_item(&self, feed: &mut Feed) -> FeedEvent;
    async fn reconcile(&self, snapshot: &Value);
    async fn pause(&self, delay: Duration);
}

pub(crate) async fn follow(host: &dyn Host, pace: Pace) {
    if !host.supported().await {
        tracing::info!("routine review is off: this harness has no routines");
        return;
    }
    let mut backoff = pace.min;
    loop {
        if host.stopped().await {
            return;
        }
        if !host.ready().await {
            if host.stopped().await {
                return;
            }
            host.pause(backoff).await;
            backoff = next_backoff(backoff, pace.max);
            continue;
        }
        let mut feed = match host.open().await {
            Ok(feed) => feed,
            Err(OpenFail::Unsupported) => return,
            Err(OpenFail::Unavailable(err)) => {
                tracing::warn!("routine review is waiting for the harness: {err}");
                if host.stopped().await {
                    return;
                }
                host.pause(backoff).await;
                backoff = next_backoff(backoff, pace.max);
                continue;
            }
        };
        backoff = pace.min;
        loop {
            if host.stopped().await {
                return;
            }
            match host.next_item(&mut feed).await {
                FeedEvent::Snapshot(snapshot) => {
                    backoff = pace.min;
                    host.reconcile(&snapshot).await;
                }
                FeedEvent::Ended => break,
                FeedEvent::Stopped => return,
            }
        }
        drop(feed);
        if host.stopped().await {
            return;
        }
        host.pause(backoff).await;
        backoff = next_backoff(backoff, pace.max);
    }
}

struct AppHost {
    state: Arc<crate::AppState>,
    applied: Mutex<BTreeSet<String>>,
}

pub(crate) fn start(state: Arc<crate::AppState>) {
    tokio::spawn(async move {
        follow(&AppHost::new(state), Pace::production()).await;
    });
}

impl AppHost {
    fn new(state: Arc<crate::AppState>) -> Self {
        Self {
            state,
            applied: Mutex::new(BTreeSet::new()),
        }
    }

    fn take_applied(&self) -> BTreeSet<String> {
        self.applied
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    fn store_applied(&self, applied: BTreeSet<String>) {
        *self.applied.lock().unwrap_or_else(|err| err.into_inner()) = applied;
    }
}

#[async_trait]
impl Host for AppHost {
    async fn supported(&self) -> bool {
        self.state.harness.routines().is_ok()
    }

    async fn stopped(&self) -> bool {
        self.state.holds.inner.closed.load(Ordering::SeqCst)
    }

    async fn ready(&self) -> bool {
        self.state.harness.connected().await
    }

    async fn open(&self) -> Result<Feed, OpenFail> {
        let routines = match self.state.harness.routines() {
            Ok(routines) => routines,
            Err(err) => {
                tracing::info!("routine review is off: {err}");
                return Err(OpenFail::Unsupported);
            }
        };
        match routines.watch().await {
            Err(HarnessError::Unsupported(_)) => Err(OpenFail::Unsupported),
            Err(err) => Err(OpenFail::Unavailable(err.to_string())),
            Ok(mut incoming) => {
                let (tx, rx) = mpsc::channel(8);
                tokio::spawn(async move {
                    while let Some(item) = incoming.recv().await {
                        let item = item.map_err(|err| err.to_string());
                        if tx.send(item).await.is_err() {
                            break;
                        }
                    }
                });
                Ok(rx)
            }
        }
    }

    async fn next_item(&self, feed: &mut Feed) -> FeedEvent {
        tokio::select! {
            _ = self.state.holds.inner.stop.notified() => FeedEvent::Stopped,
            _ = tokio::time::sleep(HEARTBEAT) => {
                tracing::info!("routine review heartbeat: subscribing again");
                FeedEvent::Ended
            }
            item = feed.recv() => match item {
                Some(Ok(snapshot)) => FeedEvent::Snapshot(snapshot),
                Some(Err(err)) => {
                    tracing::warn!("routine review stream: {err}");
                    FeedEvent::Ended
                }
                None => FeedEvent::Ended,
            },
        }
    }

    async fn reconcile(&self, snapshot: &Value) {
        let mut applied = self.take_applied();
        let bots = match self.state.bots.list().await {
            Ok(bots) => bots,
            Err(err) => {
                tracing::warn!("routine review could not list Bots: {}", err.message);
                return;
            }
        };
        let root = self.state.cfg.bots_root.join(".botapp");
        let file = ReviewFile::new(&root);
        let notices = NoticeFile::new(&root);
        let now = crate::chat::routines::stamped();
        let routines = match self.state.harness.routines() {
            Ok(routines) => routines,
            Err(_) => return,
        };
        let result = apply_snapshot(
            &mut Apply {
                routines,
                bots: &bots,
                file: &file,
                notices: &notices,
                now: &now,
                applied: &mut applied,
            },
            snapshot,
        )
        .await;
        self.store_applied(applied);
        if let Err(err) = result {
            tracing::warn!("routine review: {err}");
        }
    }

    async fn pause(&self, delay: Duration) {
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = self.state.holds.inner.stop.notified() => {}
        }
    }
}
