//! In-memory repository answers. `is_repo` stays separate from `status` so a gate
//! test can count the cheap question without a remote refresh.
use super::{MockHarness, RepoAnswer};
use crate::harness::{HarnessError, PullRow, RepoStatus, Repos};
use async_trait::async_trait;

impl MockHarness {
    pub(crate) fn set_is_repo(&self, is_repo: bool) {
        self.repo_answer().is_repo = is_repo;
    }

    pub(crate) fn repo_calls(&self) -> u32 {
        self.repo_answer().calls
    }

    pub(crate) fn repo_cwd(&self) -> String {
        self.repo_answer().cwd.clone()
    }

    pub(crate) fn set_status(&self, status: RepoStatus) {
        let mut answer = self.repo_answer();
        answer.is_repo = status.is_repo;
        answer.prepared = Some(status);
    }

    pub(crate) fn set_prs(&self, rows: Vec<PullRow>) {
        self.repo_answer().prs = rows;
    }

    pub(crate) fn set_status_fault(&self, kind: &str) {
        self.repo_answer().status_fault = Some(kind.to_string());
    }

    pub(crate) fn set_pr_fault(&self, kind: &str) {
        self.repo_answer().pr_fault = Some(kind.to_string());
    }

    pub(crate) fn status_calls(&self) -> u32 {
        self.repo_answer().status_calls
    }

    pub(crate) fn pr_calls(&self) -> u32 {
        self.repo_answer().pr_calls
    }

    pub(crate) fn last_remote(&self) -> bool {
        self.repo_answer().last_remote
    }

    fn repo_answer(&self) -> std::sync::MutexGuard<'_, RepoAnswer> {
        self.repo.lock().unwrap_or_else(|err| err.into_inner())
    }
}

fn fault(kind: &str) -> HarnessError {
    match kind {
        "offline" => HarnessError::unavailable("T3 closed the connection"),
        "signed-out" => HarnessError::failed("cli-unauthenticated"),
        "rate-limited" => HarnessError::failed("rate-limited"),
        other => HarnessError::failed(other),
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

    async fn status(&self, cwd: &str, remote: bool) -> Result<RepoStatus, HarnessError> {
        let mut answer = self.repo_answer();
        answer.status_calls += 1;
        answer.cwd = cwd.to_string();
        answer.last_remote = remote;
        if let Some(kind) = answer.status_fault.clone() {
            return Err(fault(&kind));
        }
        if let Some(status) = answer.prepared.clone() {
            return Ok(status);
        }
        let mut status = RepoStatus::not_a_repo();
        status.is_repo = answer.is_repo;
        if answer.is_repo {
            status.branch = "main".into();
            status.default_branch = true;
            status.has_remote = true;
            status.provider = "GitHub".into();
            status.base_url = "https://github.com".into();
        }
        Ok(status)
    }

    async fn pull_requests(&self, _project_id: &str) -> Result<Vec<PullRow>, HarnessError> {
        let mut answer = self.repo_answer();
        answer.pr_calls += 1;
        if let Some(kind) = answer.pr_fault.clone() {
            return Err(fault(&kind));
        }
        Ok(answer.prs.clone())
    }
}
