//! Status and pull-request reads for an open Code tab.
//!
//! Status is kept for [`STATUS_TTL`], the pull-request list for [`PR_TTL`].
//! One request per Bot runs at a time. A failure is not cached.

use super::cache::{evaluate, SignalCache};
use super::signal::CodeMode;
use crate::harness::{public_url, Harness, HarnessError, PullRow, RepoStatus};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) const STATUS_TTL: Duration = Duration::from_secs(10);
pub(crate) const PR_TTL: Duration = Duration::from_secs(60);

#[derive(Debug)]
struct Slot<T> {
    at: Instant,
    value: T,
}

#[derive(Debug)]
pub(crate) struct ReadCache {
    status_ttl: Duration,
    pr_ttl: Duration,
    status: Mutex<HashMap<String, Slot<RepoStatus>>>,
    prs: Mutex<HashMap<String, Slot<Vec<PullRow>>>>,
    flight: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl ReadCache {
    pub(crate) fn new(status_ttl: Duration, pr_ttl: Duration) -> Self {
        Self {
            status_ttl,
            pr_ttl,
            status: Mutex::new(HashMap::new()),
            prs: Mutex::new(HashMap::new()),
            flight: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn run<T>(&self, slug: &str, fut: impl Future<Output = T>) -> T {
        let lock = self.lock_for(slug);
        let _guard = lock.lock().await;
        fut.await
    }

    fn lock_for(&self, slug: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.flight
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .entry(slug.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    fn status_of(&self, slug: &str, now: Instant) -> Option<RepoStatus> {
        let slots = self.status.lock().unwrap_or_else(|err| err.into_inner());
        let slot = slots.get(slug)?;
        fresh(slot.at, now, self.status_ttl).then(|| slot.value.clone())
    }

    fn put_status(&self, slug: &str, now: Instant, value: RepoStatus) {
        self.status
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(slug.to_string(), Slot { at: now, value });
    }

    fn prs_of(&self, key: &str, now: Instant) -> Option<Vec<PullRow>> {
        let slots = self.prs.lock().unwrap_or_else(|err| err.into_inner());
        let slot = slots.get(key)?;
        fresh(slot.at, now, self.pr_ttl).then(|| slot.value.clone())
    }

    fn put_prs(&self, key: &str, now: Instant, value: Vec<PullRow>) {
        self.prs
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(key.to_string(), Slot { at: now, value });
    }
}

impl Default for ReadCache {
    fn default() -> Self {
        Self::new(STATUS_TTL, PR_TTL)
    }
}

fn fresh(at: Instant, now: Instant, ttl: Duration) -> bool {
    match now.checked_duration_since(at) {
        Some(age) => age < ttl,
        None => true,
    }
}

/// `GET /api/bots/{slug}/code`. `open` is the Code tab. Remote refresh happens only then.
#[allow(
    clippy::too_many_arguments,
    reason = "the open-tab read takes the gate inputs and the two caches"
)]
pub(crate) async fn present(
    harness: &dyn Harness,
    signals: &SignalCache,
    reads: &ReadCache,
    now: Instant,
    slug: &str,
    cwd: &str,
    project_id: Option<&str>,
    master_on: bool,
    mode: CodeMode,
    open: bool,
) -> Value {
    reads
        .run(
            slug,
            assemble(
                harness, signals, reads, now, slug, cwd, project_id, master_on, mode, open,
            ),
        )
        .await
}

#[allow(
    clippy::too_many_arguments,
    reason = "the open-tab read takes the gate inputs and the two caches"
)]
async fn assemble(
    harness: &dyn Harness,
    signals: &SignalCache,
    reads: &ReadCache,
    now: Instant,
    slug: &str,
    cwd: &str,
    project_id: Option<&str>,
    master_on: bool,
    mode: CodeMode,
    open: bool,
) -> Value {
    let gate = evaluate(harness, signals, now, slug, cwd, master_on, mode).await;
    if !gate.enabled {
        return json!({ "enabled": false });
    }
    if !open || !gate.show {
        return json!({ "enabled": true, "signal": gate.show });
    }
    let Ok(repos) = harness.repos() else {
        return body(true, "offline", None, Vec::new());
    };
    let status = if let Some(hit) = reads.status_of(slug, now) {
        hit
    } else {
        match repos.status(cwd, true).await {
            Ok(status) => {
                reads.put_status(slug, now, status.clone());
                status
            }
            Err(err) => return body(true, &code_fault(&err), None, Vec::new()),
        }
    };
    if !status.is_repo {
        return body(true, "", None, Vec::new());
    }
    let mut fault = if status.has_remote {
        String::new()
    } else {
        "no-remote".to_string()
    };
    let prs = match project_id.filter(|id| !id.is_empty()) {
        None => Vec::new(),
        Some(project) => {
            let key = format!("{slug}\0{project}");
            if let Some(hit) = reads.prs_of(&key, now) {
                hit
            } else {
                match repos.pull_requests(project).await {
                    Ok(rows) => {
                        reads.put_prs(&key, now, rows.clone());
                        rows
                    }
                    Err(err) => {
                        let next = code_fault(&err);
                        if rank(&next) > rank(&fault) {
                            fault = next;
                        }
                        Vec::new()
                    }
                }
            }
        }
    };
    body(true, &fault, Some(status), prs)
}

fn body(signal: bool, fault: &str, status: Option<RepoStatus>, prs: Vec<PullRow>) -> Value {
    json!({
        "enabled": true,
        "signal": signal,
        "error": fault,
        "repo": status.as_ref().map(repo_json),
        "prs": prs.iter().map(pr_json).collect::<Vec<_>>(),
    })
}

fn repo_json(status: &RepoStatus) -> Value {
    json!({
        "branch": status.branch,
        "defaultBranch": status.default_branch,
        "ahead": status.ahead,
        "behind": status.behind,
        "hasRemote": status.has_remote,
        "provider": status.provider,
        "baseUrl": public_url(&status.base_url),
        "insertions": status.insertions,
        "deletions": status.deletions,
        "fileTotal": status.file_total,
        "files": status.files.iter().map(|file| json!({
            "path": file.path,
            "insertions": file.insertions,
            "deletions": file.deletions,
        })).collect::<Vec<_>>(),
        "linked": status.linked.as_ref().map(|linked| json!({
            "number": linked.number,
            "title": linked.title,
            "url": public_url(&linked.url),
            "state": linked.state,
            "draft": linked.draft,
        })),
    })
}

fn pr_json(row: &PullRow) -> Value {
    json!({
        "number": row.number,
        "title": row.title,
        "url": public_url(&row.url),
        "draft": row.draft,
        "checks": row.checks,
        "review": row.review,
        "updated": row.updated,
    })
}

fn code_fault(err: &HarnessError) -> String {
    match err {
        HarnessError::Unavailable(_) | HarnessError::Unsupported(_) => "offline".into(),
        HarnessError::Failed(msg) => classify(msg).into(),
    }
}

fn classify(msg: &str) -> &'static str {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("rate-limited") || lower.contains("rate limit") {
        "rate-limited"
    } else if lower.contains("authentication")
        || lower.contains("unauthenticated")
        || lower.contains("cli-unauthenticated")
    {
        "signed-out"
    } else {
        "offline"
    }
}

fn rank(fault: &str) -> u8 {
    match fault {
        "offline" => 4,
        "rate-limited" => 3,
        "signed-out" => 2,
        "no-remote" => 1,
        _ => 0,
    }
}
