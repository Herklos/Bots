use super::*;

/// How long a matching chat read waits for the transcript to change.
pub(crate) const HOLD_FOR: Duration = Duration::from_secs(25);

/// How often a held read asks the existing chat read whether the transcript moved.
pub(crate) const LOOK_EVERY: Duration = Duration::from_millis(500);

/// Fields that should repaint an open chat. `now` is left out: it changes on every read
/// and is only there so the client can label days.
pub(crate) fn paint_slice(view: &Value) -> Value {
    json!({
        "messages": view.get("messages").cloned().unwrap_or(json!([])),
        "events": view.get("events").cloned().unwrap_or(json!([])),
        "requests": view.get("requests").cloned().unwrap_or(json!([])),
        "working": view.get("working").and_then(Value::as_bool).unwrap_or(false),
        "pins": view.get("pins").cloned().unwrap_or(json!([])),
        "readOnly": view.get("readOnly").and_then(Value::as_bool).unwrap_or(false),
        "threadId": view.get("threadId").and_then(Value::as_str).unwrap_or(""),
    })
}

/// A stable tag for one transcript. A child thread, when the body carries one, uses the same fields.
pub(crate) fn transcript_tag(view: &Value) -> String {
    let mut slice = paint_slice(view);
    if let Some(child) = view.get("child").filter(|child| child.is_object()) {
        slice["child"] = paint_slice(child);
    }
    let bytes = serde_json::to_vec(&slice).unwrap_or_default();
    let digest = sha2::Sha256::digest(&bytes);
    format!("\"{}\"", hex::encode(digest))
}

/// `If-None-Match` lists tags. A match includes a weak tag and a bare or quoted form.
pub(crate) fn none_match(header: Option<&str>, tag: &str) -> bool {
    let Some(header) = header else {
        return false;
    };
    let tag = tag.trim().trim_matches('"');
    header.split(',').any(|part| {
        let part = part.trim();
        if part == "*" {
            return true;
        }
        let part = part
            .strip_prefix("W/")
            .unwrap_or(part)
            .trim()
            .trim_matches('"');
        part == tag
    })
}

/// The group list repaints from these fields. There is no clock in the body.
pub(crate) fn room_tag(view: &Value) -> String {
    let slice = json!({
        "id": view.get("id").and_then(Value::as_str).unwrap_or(""),
        "title": view.get("title").and_then(Value::as_str).unwrap_or(""),
        "description": view.get("description").and_then(Value::as_str).unwrap_or(""),
        "messages": view.get("messages").cloned().unwrap_or(json!([])),
        "members": view.get("members").cloned().unwrap_or(json!([])),
        "notifications": view.get("notifications").and_then(Value::as_bool).unwrap_or(true),
    });
    let bytes = serde_json::to_vec(&slice).unwrap_or_default();
    let digest = sha2::Sha256::digest(&bytes);
    format!("\"{}\"", hex::encode(digest))
}

#[derive(Debug)]
pub(crate) struct Lane {
    pub(crate) gen: u64,
    pub(crate) wake: Arc<Notify>,
}

#[derive(Debug)]
pub(crate) struct HoldInner {
    pub(crate) lanes: Mutex<HashMap<String, Lane>>,
    pub(crate) stop: Notify,
    pub(crate) closed: AtomicBool,
}

/// One held read per chat or group. A retry replaces the previous wait. Shutdown ends it.
#[derive(Clone, Debug)]
pub(crate) struct HeldReads {
    pub(crate) inner: Arc<HoldInner>,
}

#[derive(Debug)]
pub(crate) enum Hold<T> {
    Changed(T),
    Same,
    Superseded,
    Stopped,
}

impl HeldReads {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(HoldInner {
                lanes: Mutex::new(HashMap::new()),
                stop: Notify::new(),
                closed: AtomicBool::new(false),
            }),
        }
    }

    pub(crate) fn stop(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        self.inner.stop.notify_waiters();
    }

    pub(crate) fn lanes(&self) -> std::sync::MutexGuard<'_, HashMap<String, Lane>> {
        self.inner
            .lanes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn begin(&self, key: &str) -> (u64, Arc<Notify>) {
        let mut lanes = self.lanes();
        let lane = lanes.entry(key.to_string()).or_insert_with(|| Lane {
            gen: 0,
            wake: Arc::new(Notify::new()),
        });
        lane.gen = lane.gen.wrapping_add(1);
        let gen = lane.gen;
        let wake = lane.wake.clone();
        wake.notify_waiters();
        (gen, wake)
    }

    pub(crate) fn generation(&self, key: &str) -> u64 {
        self.lanes().get(key).map(|lane| lane.gen).unwrap_or(0)
    }

    /// Waits until `load` reports a tag other than `seen`, the wait is replaced, time runs out, or shutdown.
    pub(crate) async fn hold_until_change<T, F, Fut>(
        &self,
        key: &str,
        seen: &str,
        look: Duration,
        timeout: Duration,
        mut load: F,
    ) -> Hold<T>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Option<(T, String)>>,
    {
        let (gen, wake) = self.begin(key);
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.inner.closed.load(Ordering::SeqCst) {
                return Hold::Stopped;
            }
            if self.generation(key) != gen {
                return Hold::Superseded;
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Hold::Same;
            }
            let slice = look.min(deadline - now);
            tokio::select! {
                _ = self.inner.stop.notified() => return Hold::Stopped,
                _ = wake.notified() => continue,
                _ = tokio::time::sleep(slice) => {}
            }
            if self.inner.closed.load(Ordering::SeqCst) {
                return Hold::Stopped;
            }
            if self.generation(key) != gen {
                return Hold::Superseded;
            }
            if let Some((value, tag)) = load().await {
                if tag != seen {
                    return Hold::Changed(value);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Hold::Same;
            }
        }
    }
}
