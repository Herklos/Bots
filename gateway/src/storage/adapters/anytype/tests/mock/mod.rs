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

    pub(crate) fn peer_write(&self, id: &str, properties: serde_json::Value, tear: bool) -> bool {
        self.state
            .lock()
            .expect("mock")
            .peer_write(id, &properties, tear)
    }
}
