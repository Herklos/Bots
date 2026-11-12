//! Local stand-in for the Anytype HTTP API. Nothing here dials a real app.
//! Fault switches force 409, 429, 404, 410, an archived object, a bad body, or a delay.
//! `peer_write` mutates one object the way another machine would.

mod catalog;
mod objects;
mod routes;
mod search;
mod state;

use state::MockState;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Shared = Arc<Mutex<MockState>>;

pub(super) struct Mock {
    pub url: String,
    state: Shared,
}

impl Mock {
    pub(super) async fn start() -> Self {
        let state = Arc::new(Mutex::new(MockState::new()));
        let app = routes::router(Arc::clone(&state));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            url: format!("http://{address}"),
            state,
        }
    }

    pub(super) fn set_not_granted(&self, on: bool) {
        self.state.lock().expect("mock").not_granted = on;
    }

    pub(super) fn set_malformed(&self, on: bool) {
        self.state.lock().expect("mock").malformed = on;
    }

    pub(super) fn set_etag_mismatch(&self, on: bool) {
        self.state.lock().expect("mock").etag_mismatch = on;
    }

    pub(super) fn set_archived_writes(&self, on: bool) {
        self.state.lock().expect("mock").archived_writes = on;
    }

    pub(super) fn set_status(&self, status: Option<u16>) {
        self.state.lock().expect("mock").status_override = status;
    }

    /// `retry_after_secs == 0` omits `Retry-After`, matching the live 429.
    pub(super) fn set_rate_limit(&self, times: u32, retry_after_secs: u64) {
        let mut state = self.state.lock().expect("mock");
        state.rate_limits_left = times;
        state.retry_after_secs = retry_after_secs;
    }

    pub(super) fn set_delay(&self, delay: Duration) {
        self.state.lock().expect("mock").delay = delay;
    }

    pub(super) fn object_count(&self) -> usize {
        self.state.lock().expect("mock").objects.len()
    }

    pub(super) fn type_count(&self) -> usize {
        self.state.lock().expect("mock").types.len()
    }

    pub(super) fn seed_objects(&self, count: usize) {
        self.state.lock().expect("mock").seed_objects(count);
    }

    pub(super) fn rewrite_key(&self, from: &str, to: &str) {
        self.state
            .lock()
            .expect("mock")
            .rewrites
            .insert(from.to_string(), to.to_string());
    }

    pub(super) fn seed_types(&self, count: usize) {
        self.state.lock().expect("mock").seed_types(count);
    }

    pub(super) fn seed_named(&self, name: &str, type_key: &str) {
        let mut state = self.state.lock().expect("mock");
        let body = serde_json::json!({ "name": name, "type": type_key });
        state.create_object(&body);
    }

    pub(super) fn peer_write(&self, id: &str, properties: serde_json::Value, tear: bool) -> bool {
        self.state
            .lock()
            .expect("mock")
            .peer_write(id, &properties, tear)
    }
}
