//! Local stand-in for the Anytype HTTP API. Nothing here dials a real app.
//! Fault switches force 409, 429, 404, 410, an archived object, a bad body, or a delay.
//! `peer_write` mutates one object the way another machine would.

mod catalog;
mod objects;
mod routes;
mod routes_files;
mod routes_types;
mod search;
mod state;
mod stream;

use state::MockState;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Shared = Arc<Mutex<MockState>>;

pub(crate) struct Mock {
    pub url: String,
    state: Shared,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Clone for Mock {
    fn clone(&self) -> Self {
        Self {
            url: self.url.clone(),
            state: Arc::clone(&self.state),
            shutdown: None,
        }
    }
}

impl Mock {
    pub(crate) async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        Self::from_listener(listener)
    }

    /// Serves on an already-bound listener so a test can refuse the port, then bind it again.
    pub(crate) fn from_listener(listener: tokio::net::TcpListener) -> Self {
        let state = Arc::new(Mutex::new(MockState::new()));
        let app = routes::router(Arc::clone(&state));
        let address = listener.local_addr().unwrap();
        let (shutdown, done) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = done.await;
                })
                .await;
        });
        Self {
            url: format!("http://{address}"),
            state,
            shutdown: Some(shutdown),
        }
    }

    /// Stops accepting. The port can be bound again after this returns.
    pub(crate) async fn shutdown(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    pub(crate) fn set_not_granted(&self, on: bool) {
        self.state.lock().expect("mock").not_granted = on;
    }

    pub(crate) fn set_malformed(&self, on: bool) {
        self.state.lock().expect("mock").malformed = on;
    }

    pub(crate) fn set_etag_mismatch(&self, on: bool) {
        self.state.lock().expect("mock").etag_mismatch = on;
    }

    pub(crate) fn set_archived_writes(&self, on: bool) {
        self.state.lock().expect("mock").archived_writes = on;
    }

    pub(crate) fn set_status(&self, status: Option<u16>) {
        self.state.lock().expect("mock").status_override = status;
    }

    /// `retry_after_secs == 0` omits `Retry-After`, matching the live 429.
    pub(crate) fn set_rate_limit(&self, times: u32, retry_after_secs: u64) {
        let mut state = self.state.lock().expect("mock");
        state.rate_limits_left = times;
        state.retry_after_secs = retry_after_secs;
    }

    pub(crate) fn set_delay(&self, delay: Duration) {
        self.state.lock().expect("mock").delay = delay;
    }

    pub(crate) fn object_count(&self) -> usize {
        self.state.lock().expect("mock").objects.len()
    }

    pub(crate) fn type_count(&self) -> usize {
        self.state.lock().expect("mock").types.len()
    }

    pub(crate) fn seed_objects(&self, count: usize) {
        self.state.lock().expect("mock").seed_objects(count);
    }

    pub(crate) fn rewrite_key(&self, from: &str, to: &str) {
        self.state
            .lock()
            .expect("mock")
            .rewrites
            .insert(from.to_string(), to.to_string());
    }

    pub(crate) fn seed_types(&self, count: usize) {
        self.state.lock().expect("mock").seed_types(count);
    }

    pub(crate) fn seed_named(&self, name: &str, type_key: &str) {
        let mut state = self.state.lock().expect("mock");
        let body = serde_json::json!({ "name": name, "type": type_key });
        state.create_object(&body);
    }

    /// Backdate or move `last_modified_date` without a write. Garbage collection reads it.
    pub(crate) fn set_last_modified(&self, id: &str, stamp: &str) -> bool {
        let mut state = self.state.lock().expect("mock");
        let Some(object) = state.objects.get_mut(id) else {
            return false;
        };
        object.last_modified = Some(stamp.to_string());
        true
    }

    pub(crate) fn peer_write(&self, id: &str, properties: serde_json::Value, tear: bool) -> bool {
        self.state
            .lock()
            .expect("mock")
            .peer_write(id, &properties, tear)
    }

    /// `0` refuses every new search stream with 429. Open streams stay until dropped.
    pub(crate) fn set_stream_cap(&self, cap: usize) {
        self.state.lock().expect("mock").stream_cap = cap;
    }

    /// Closes every open search stream. The client sees a clean end and may reconnect.
    pub(crate) fn drop_streams(&self) {
        self.state.lock().expect("mock").drop_streams();
    }

    /// The id stays in search and on GET, but the stream snapshot and live feed omit it.
    pub(crate) fn omit_from_stream(&self, id: &str) {
        let mut state = self.state.lock().expect("mock");
        state.stream_omit.insert(id.to_string());
        state.announce(id);
    }

    pub(crate) fn get_count(&self) -> u64 {
        self.state.lock().expect("mock").gets
    }

    pub(crate) fn reset_gets(&self) {
        self.state.lock().expect("mock").gets = 0;
    }

    pub(crate) fn last_stream_body(&self) -> serde_json::Value {
        self.state.lock().expect("mock").last_stream_body.clone()
    }
}
