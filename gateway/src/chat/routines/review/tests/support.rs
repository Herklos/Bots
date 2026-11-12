//! The watcher, against a mock `Routines`: reconnect, a repeated snapshot,
//! a failed disable, and a harness that is offline at the start.

use super::super::apply::{apply_snapshot, Apply};
use super::super::notice::NoticeFile;
use super::super::store::{ReviewFile, ReviewStore};
use super::super::watch::{Feed, FeedEvent, Host, OpenFail};
use super::agent;
use super::NOW;
use crate::harness::{HarnessError, RoutineFeed, Routines};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) const PROMPT: &str = "secret prompt text";

pub(super) fn bot(mode: &str) -> Value {
    json!({
        "slug": "ada",
        "name": "Ada",
        "mainThreadId": "thread-ada",
        "projectId": "project-ada",
        "runtimeMode": "approval-required",
        "routineReview": mode,
    })
}

pub(super) fn task(id: &str, every_ms: u64) -> Value {
    let mut task = agent(id, every_ms);
    task["threadId"] = json!("thread-ada");
    task["projectId"] = json!("project-ada");
    task["environmentId"] = json!("env-9");
    task
}

pub(super) fn snap(tasks: Vec<Value>) -> Value {
    json!({ "tasks": tasks })
}

pub(super) struct Fake {
    pub(super) fail_enabled: AtomicU32,
    pub(super) attempts: AtomicU32,
    writes: Mutex<Vec<Value>>,
    steps: Mutex<VecDeque<Step>>,
    stop: Arc<AtomicBool>,
    pub(super) watches: AtomicU32,
}

pub(super) enum Step {
    Fail,
    Stop,
    Unsupported,
    Snaps(Vec<Value>),
}

impl Fake {
    pub(super) fn new(steps: Vec<Step>, stop: Arc<AtomicBool>) -> Self {
        Self {
            fail_enabled: AtomicU32::new(0),
            attempts: AtomicU32::new(0),
            writes: Mutex::new(Vec::new()),
            steps: Mutex::new(steps.into()),
            stop,
            watches: AtomicU32::new(0),
        }
    }

    pub(super) fn writes(&self) -> Vec<Value> {
        self.writes.lock().expect("writes").clone()
    }
}

#[async_trait]
impl Routines for Fake {
    async fn list(&self) -> Result<Value, HarnessError> {
        Ok(json!({ "tasks": [] }))
    }

    async fn upsert(&self, body: &Value) -> Result<Value, HarnessError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        self.writes.lock().expect("writes").push(body.clone());
        Ok(json!({ "task": body }))
    }

    async fn set_enabled(&self, body: &Value) -> Result<Value, HarnessError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        if self.fail_enabled.load(Ordering::SeqCst) > 0 {
            self.fail_enabled.fetch_sub(1, Ordering::SeqCst);
            return Err(HarnessError::failed("could not update the routine"));
        }
        self.writes.lock().expect("writes").push(body.clone());
        Ok(json!({ "task": body }))
    }

    async fn delete(&self, body: &Value) -> Result<Value, HarnessError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        self.writes.lock().expect("writes").push(body.clone());
        Ok(body.clone())
    }

    async fn run_now(&self, body: &Value) -> Result<Value, HarnessError> {
        Ok(body.clone())
    }

    async fn watch(&self) -> Result<RoutineFeed, HarnessError> {
        self.watches.fetch_add(1, Ordering::SeqCst);
        let step = self.steps.lock().expect("steps").pop_front();
        match step {
            Some(Step::Fail) => Err(HarnessError::unavailable("offline")),
            Some(Step::Unsupported) => Err(HarnessError::unsupported("routines")),
            Some(Step::Stop) | None => {
                self.stop.store(true, Ordering::SeqCst);
                Err(HarnessError::unavailable("stopped"))
            }
            Some(Step::Snaps(items)) => {
                let (tx, rx) = tokio::sync::mpsc::channel(8);
                tokio::spawn(async move {
                    for item in items {
                        if tx.send(Ok(item)).await.is_err() {
                            break;
                        }
                    }
                });
                Ok(rx)
            }
        }
    }
}

pub(super) struct World {
    pub(super) fake: Arc<Fake>,
    unready: AtomicU32,
    stop: Arc<AtomicBool>,
    pub(super) file: ReviewFile,
    pub(super) notices: NoticeFile,
    bots: Vec<Value>,
    applied: Mutex<BTreeSet<String>>,
    pub(super) pauses: Mutex<Vec<Duration>>,
    pub(super) supported: bool,
}

impl World {
    pub(super) async fn open(steps: Vec<Step>, unready: u32) -> (Self, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let botapp = dir.path().join(".botapp");
        let file = ReviewFile::new(&botapp);
        let mut store = ReviewStore::empty();
        store.seal_baseline(std::iter::empty::<&str>(), NOW);
        file.save(&store).await.unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let world = Self {
            fake: Arc::new(Fake::new(steps, Arc::clone(&stop))),
            unready: AtomicU32::new(unready),
            stop,
            file,
            notices: NoticeFile::new(&botapp),
            bots: vec![bot("ask")],
            applied: Mutex::new(BTreeSet::new()),
            pauses: Mutex::new(Vec::new()),
            supported: true,
        };
        (world, dir)
    }
}

#[async_trait]
impl Host for World {
    async fn supported(&self) -> bool {
        self.supported
    }

    async fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    async fn ready(&self) -> bool {
        let left = self.unready.load(Ordering::SeqCst);
        if left > 0 {
            self.unready.fetch_sub(1, Ordering::SeqCst);
            return false;
        }
        true
    }

    async fn open(&self) -> Result<Feed, OpenFail> {
        match self.fake.watch().await {
            Ok(mut incoming) => {
                let (tx, rx) = tokio::sync::mpsc::channel(8);
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
            Err(HarnessError::Unsupported(_)) => Err(OpenFail::Unsupported),
            Err(err) => Err(OpenFail::Unavailable(err.to_string())),
        }
    }

    async fn next_item(&self, feed: &mut Feed) -> FeedEvent {
        match feed.recv().await {
            Some(Ok(snapshot)) => FeedEvent::Snapshot(snapshot),
            Some(Err(_)) | None => FeedEvent::Ended,
        }
    }

    async fn reconcile(&self, snapshot: &Value) {
        let mut applied = self.applied.lock().expect("applied").clone();
        apply_snapshot(
            &mut Apply {
                routines: &*self.fake,
                bots: &self.bots,
                file: &self.file,
                notices: &self.notices,
                now: NOW,
                applied: &mut applied,
            },
            snapshot,
        )
        .await
        .expect("reconcile");
        *self.applied.lock().expect("applied") = applied;
    }

    async fn pause(&self, delay: Duration) {
        self.pauses.lock().expect("pauses").push(delay);
    }
}

#[tokio::test]
async fn a_review_harness_has_no_webhook_log() {
    let fake = Fake::new(vec![], Arc::new(AtomicBool::new(false)));
    for err in [
        fake.rotate_webhook_token("scheduled-task:1")
            .await
            .unwrap_err(),
        fake.list_deliveries("scheduled-task:1").await.unwrap_err(),
        fake.get_delivery("scheduled-task:1", "delivery:1")
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(err, HarnessError::Unsupported(_)));
    }
}
