//! In-memory harness for domain tests. Adapter tests that speak the wire still use `tests/mock-t3.ts`.
use crate::harness::{
    Approval, Approvals, Browser, BrowserTab, ComputerCatalog, ForkRequest, Forks, Frame, Harness,
    HarnessError, LaunchThread, MergeRequest, Previews, ProjectInfo, ProvisionedSend, Provisioning,
    Relay, Repos, Routines, ShellSnapshot, SideChatRef, ThreadSnapshot, Transcript, Turns,
    UserMessage,
};
use async_trait::async_trait;

mod forks;
use axum::response::IntoResponse;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
mod routines;

/// One `set_model` the mock actually accepted.
#[derive(Clone, Debug)]
pub(crate) struct ModelPush {
    pub project_id: Option<String>,
    pub thread_id: Option<String>,
    pub selection: Value,
}

#[derive(Debug)]
struct ForkBook {
    armed: bool,
    chats: HashMap<String, Vec<SideChatRef>>,
    forks: Vec<ForkRequest>,
    merges: Vec<MergeRequest>,
    deletes: Vec<String>,
    sends: Vec<(String, String)>,
    launches: usize,
    fail_send: bool,
    projections: HashMap<String, Value>,
}

#[derive(Debug)]
pub(crate) struct MockHarness {
    pub tabs: Vec<BrowserTab>,
    pub busy: Vec<String>,
    typed: Mutex<HashMap<String, String>>,
    model_pushes: Mutex<Vec<ModelPush>>,
    repo: Mutex<RepoAnswer>,
    /// Stand-in for the harness provider config. `None` is the usual "not wired" error.
    provider_config: Mutex<Option<Value>>,
    provider_config_reads: AtomicUsize,
    forks: Mutex<ForkBook>,
    scheduled: Mutex<std::collections::BTreeMap<String, Value>>,
    messages: Mutex<Vec<UserMessage>>,
    approvals: Mutex<Vec<Approval>>,
}

#[derive(Debug)]
struct RepoAnswer {
    is_repo: bool,
    calls: u32,
    cwd: String,
}

impl MockHarness {
    pub(crate) fn new() -> Self {
        Self {
            scheduled: Mutex::default(),
            messages: Mutex::default(),
            approvals: Mutex::default(),
            tabs: Vec::new(),
            busy: Vec::new(),
            typed: Mutex::new(HashMap::new()),
            model_pushes: Mutex::new(Vec::new()),
            repo: Mutex::new(RepoAnswer {
                is_repo: false,
                calls: 0,
                cwd: String::new(),
            }),
            provider_config: Mutex::new(None),
            provider_config_reads: AtomicUsize::new(0),
            forks: Mutex::new(ForkBook {
                armed: false,
                chats: HashMap::new(),
                forks: Vec::new(),
                merges: Vec::new(),
                deletes: Vec::new(),
                sends: Vec::new(),
                launches: 0,
                fail_send: false,
                projections: HashMap::new(),
            }),
        }
    }

    pub(crate) fn messages(&self) -> Vec<UserMessage> {
        self.messages.lock().unwrap().clone()
    }
    pub(crate) fn approvals(&self) -> Vec<Approval> {
        self.approvals.lock().unwrap().clone()
    }
    pub(crate) fn set_is_repo(&self, is_repo: bool) {
        self.repo_answer().is_repo = is_repo;
    }

    pub(crate) fn repo_calls(&self) -> u32 {
        self.repo_answer().calls
    }

    pub(crate) fn repo_cwd(&self) -> String {
        self.repo_answer().cwd.clone()
    }

    fn repo_answer(&self) -> std::sync::MutexGuard<'_, RepoAnswer> {
        self.repo.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub(crate) fn set_provider_config(&self, config: Value) {
        *self
            .provider_config
            .lock()
            .unwrap_or_else(|err| err.into_inner()) = Some(config);
    }

    pub(crate) fn provider_config_reads(&self) -> usize {
        self.provider_config_reads.load(Ordering::SeqCst)
    }

    fn book(&self) -> std::sync::MutexGuard<'_, ForkBook> {
        self.forks.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub(crate) fn model_pushes(&self) -> Vec<ModelPush> {
        self.model_pushes
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    pub(crate) fn typed(&self, tab: &str) -> String {
        self.typed
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .get(tab)
            .cloned()
            .unwrap_or_default()
    }
}

#[async_trait]
impl Repos for MockHarness {
    async fn is_repo(&self, cwd: &str) -> Result<bool, HarnessError> {
        let mut answer = self.repo_answer();
        answer.calls += 1;
        answer.cwd = cwd.to_string();
        Ok(answer.is_repo)
    }
}

fn missing(what: &'static str) -> HarnessError {
    HarnessError::unsupported(what)
}

#[async_trait]
impl Transcript for MockHarness {
    async fn read(&self, thread_id: &str) -> Result<ThreadSnapshot, HarnessError> {
        let body = self
            .book()
            .projections
            .get(thread_id)
            .cloned()
            .unwrap_or_else(|| json!({ "turnItems": [] }));
        Ok(ThreadSnapshot::new(body))
    }

    async fn activity(&self) -> Result<ShellSnapshot, HarnessError> {
        Ok(ShellSnapshot {
            threads: Vec::new(),
            http_required: false,
            busy_count: 0,
        })
    }

    async fn busy(&self) -> Result<ShellSnapshot, HarnessError> {
        self.activity().await
    }
}

#[async_trait]
impl Turns for MockHarness {
    async fn send_user(&self, message: &UserMessage) -> Result<(), HarnessError> {
        let mut book = self.book();
        if book.fail_send {
            book.fail_send = false;
            return Err(HarnessError::failed("the side message did not send"));
        }
        book.sends
            .push((message.thread_id.clone(), message.text.clone()));
        self.messages.lock().unwrap().push(message.clone());
        Ok(())
    }

    async fn interrupt(&self, _thread_id: &str, _run_id: &str) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn steer(&self, _thread_id: &str, _text: &str) -> Result<String, HarnessError> {
        Ok("mock-message".into())
    }

    async fn set_runtime_mode(&self, _thread_id: &str, _mode: &str) -> Result<(), HarnessError> {
        Ok(())
    }
}

#[async_trait]
impl Previews for MockHarness {
    async fn inbox(
        &self,
        thread_id: &str,
        names: &HashMap<String, String>,
    ) -> Result<Option<Value>, HarnessError> {
        Ok(self.read(thread_id).await?.preview(names))
    }
}

#[async_trait]
impl Approvals for MockHarness {
    async fn answer(&self, approval: &Approval) -> Result<Value, HarnessError> {
        self.approvals.lock().unwrap().push(approval.clone());
        Ok(json!({ "ok": true }))
    }
}

#[async_trait]
impl Provisioning for MockHarness {
    async fn projects(&self) -> Result<Vec<ProjectInfo>, HarnessError> {
        Ok(Vec::new())
    }

    async fn create_project(
        &self,
        _project_id: &str,
        _title: &str,
        _workspace_root: &str,
        _model: &Value,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn update_face(
        &self,
        _project_id: &str,
        _name: &str,
        _favicon: &str,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn launch(&self, _spec: &LaunchThread) -> Result<(), HarnessError> {
        self.book().launches += 1;
        Ok(())
    }

    async fn send_provisioned(
        &self,
        _thread_id: &str,
        _text: &str,
        _kind: ProvisionedSend,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn set_model(
        &self,
        project_id: Option<&str>,
        thread_id: Option<&str>,
        selection: &Value,
    ) -> Result<(), HarnessError> {
        self.model_pushes
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .push(ModelPush {
                project_id: project_id.map(str::to_string),
                thread_id: thread_id.map(str::to_string),
                selection: selection.clone(),
            });
        Ok(())
    }

    async fn provider_config(&self) -> Result<Value, HarnessError> {
        self.provider_config_reads.fetch_add(1, Ordering::SeqCst);
        self.provider_config
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
            .ok_or_else(|| missing("providers"))
    }

    async fn computers(&self) -> Result<ComputerCatalog, HarnessError> {
        Ok(ComputerCatalog {
            ok: true,
            local_label: "This computer".into(),
            broker_id: String::new(),
            links: Vec::new(),
        })
    }
}

#[async_trait]
impl Browser for MockHarness {
    async fn pages(&self, _thread_id: &str) -> Result<Vec<Value>, HarnessError> {
        Err(missing("browser pages"))
    }

    async fn frame(
        &self,
        _thread_id: &str,
        _tab_id: &str,
        _large: bool,
    ) -> Result<Vec<u8>, HarnessError> {
        Err(missing("browser stream"))
    }

    async fn take(
        &self,
        _thread_id: &str,
        _slug: &str,
        _tab_id: &str,
    ) -> Result<Vec<u8>, HarnessError> {
        Err(missing("browser stream"))
    }

    async fn control(
        &self,
        _thread_id: &str,
        _tab_id: &str,
        _action: &str,
    ) -> Result<Vec<u8>, HarnessError> {
        Err(missing("browser stream"))
    }

    async fn input(
        &self,
        _thread_id: &str,
        _tab_id: &str,
        _messages: &[Value],
    ) -> Result<Frame, HarnessError> {
        Err(missing("browser stream"))
    }

    async fn held_input(&self, _slug: &str, _tab_id: &str, _messages: &[Value]) -> Option<Vec<u8>> {
        None
    }

    async fn held_frame(&self, _slug: &str) -> Option<Vec<u8>> {
        None
    }

    async fn release(&self, _slug: &str) -> bool {
        false
    }

    async fn edit(
        &self,
        _thread_id: &str,
        _tab_id: &str,
        _kind: &str,
    ) -> Result<Frame, HarnessError> {
        Err(missing("browser stream"))
    }

    async fn tabs(&self, _thread_id: &str) -> Result<Vec<BrowserTab>, String> {
        Ok(self.tabs.clone())
    }

    async fn type_text(&self, _thread_id: &str, tab_id: &str, text: &str) -> Result<(), String> {
        if self.busy.iter().any(|tab| tab == tab_id) {
            return Err(
                "The user is controlling this tab right now (Take over). Try again when they're done."
                    .into(),
            );
        }
        self.typed
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .entry(tab_id.to_string())
            .or_default()
            .push_str(text);
        Ok(())
    }
}

#[async_trait]
impl Relay for MockHarness {
    fn http_base(&self) -> &str {
        ""
    }

    fn ws_base(&self) -> String {
        String::new()
    }

    async fn proxy_http(
        &self,
        _method: axum::http::Method,
        _path_and_query: &str,
        _headers: &axum::http::HeaderMap,
        _body: axum::body::Bytes,
    ) -> axum::response::Response {
        axum::http::StatusCode::NOT_IMPLEMENTED.into_response()
    }

    async fn proxy_ws(
        &self,
        _client: axum::extract::ws::WebSocket,
        _upstream_url: String,
        _close_sockets: std::sync::Arc<tokio::sync::Notify>,
    ) {
    }
}

#[async_trait]
impl Harness for MockHarness {
    async fn connected(&self) -> bool {
        true
    }

    fn transcript(&self) -> Result<&dyn Transcript, HarnessError> {
        Ok(self)
    }

    fn turns(&self) -> Result<&dyn Turns, HarnessError> {
        Ok(self)
    }

    fn forks(&self) -> Result<&dyn Forks, HarnessError> {
        Ok(self)
    }

    fn previews(&self) -> Result<&dyn Previews, HarnessError> {
        Ok(self)
    }

    fn routines(&self) -> Result<&dyn Routines, HarnessError> {
        Ok(self)
    }

    fn approvals(&self) -> Result<&dyn Approvals, HarnessError> {
        Ok(self)
    }

    fn provisioning(&self) -> Result<&dyn Provisioning, HarnessError> {
        Ok(self)
    }

    fn browser(&self) -> Result<&dyn Browser, HarnessError> {
        Ok(self)
    }

    fn relay(&self) -> Result<&dyn Relay, HarnessError> {
        Err(missing("the T3 passthrough"))
    }

    fn repos(&self) -> Result<&dyn Repos, HarnessError> {
        Ok(self)
    }
}
