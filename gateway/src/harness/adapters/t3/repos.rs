//! Repository reads.
//!
//! `vcs.refreshStatus` is the unary status call (`VcsStatusInput` plus optional `localOnly`,
//! success `VcsStatusResult`). `subscribeVcsStatus` is the live stream; a tab read does not
//! hold it. `pullRequests.list` is `PullRequestListInput` to `PullRequestListResult`.
use super::T3;
use crate::harness::{
    public_url, HarnessError, LinkedChange, PullRow, RepoFile, RepoStatus, Repos, FILE_CAP, PR_CAP,
};
use async_trait::async_trait;
use serde_json::{json, Value};

const REFRESH: &str = "vcs.refreshStatus";
const LIST: &str = "pullRequests.list";

#[async_trait]
impl Repos for T3 {
    async fn is_repo(&self, cwd: &str) -> Result<bool, HarnessError> {
        Ok(self.status(cwd, false).await?.is_repo)
    }

    async fn status(&self, cwd: &str, remote: bool) -> Result<RepoStatus, HarnessError> {
        let mut payload = json!({ "cwd": cwd });
        if !remote {
            payload["localOnly"] = json!(true);
        }
        let value = call(self, REFRESH, payload).await?;
        parse_status(&value)
    }

    async fn pull_requests(&self, project_id: &str) -> Result<Vec<PullRow>, HarnessError> {
        let value = call(
            self,
            LIST,
            json!({
                "state": "open",
                "projectId": project_id,
                "limit": PR_CAP,
            }),
        )
        .await?;
        parse_pulls(&value)
    }
}

async fn call(t3: &T3, tag: &str, payload: Value) -> Result<Value, HarnessError> {
    t3.rpc()
        .await
        .map_err(HarnessError::unavailable)?
        .call(tag, payload)
        .await
        .map_err(HarnessError::failed)
}

fn parse_status(value: &Value) -> Result<RepoStatus, HarnessError> {
    let Some(obj) = value.as_object() else {
        return Err(HarnessError::failed("repository status was not an object"));
    };
    let is_repo = obj.get("isRepo").and_then(Value::as_bool).unwrap_or(false);
    if !is_repo {
        return Ok(RepoStatus::not_a_repo());
    }
    let provider = obj
        .get("sourceControlProvider")
        .and_then(|v| v.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let base_url = public_url(
        obj.get("sourceControlProvider")
            .and_then(|v| v.get("baseUrl"))
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    let tree = obj.get("workingTree");
    let files_raw = tree.and_then(|v| v.get("files")).and_then(Value::as_array);
    let file_total = files_raw.map(Vec::len).unwrap_or(0);
    let files = files_raw
        .into_iter()
        .flatten()
        .take(FILE_CAP)
        .filter_map(file_of)
        .collect();
    Ok(RepoStatus {
        is_repo: true,
        provider,
        base_url,
        has_remote: obj
            .get("hasPrimaryRemote")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        default_branch: obj
            .get("isDefaultRef")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        branch: obj
            .get("refName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        ahead: num(obj.get("aheadCount")),
        behind: num(obj.get("behindCount")),
        insertions: num(tree.and_then(|v| v.get("insertions"))),
        deletions: num(tree.and_then(|v| v.get("deletions"))),
        files,
        file_total,
        linked: obj.get("pr").and_then(linked_of),
    })
}

fn file_of(value: &Value) -> Option<RepoFile> {
    let path = value.get("path")?.as_str()?.to_string();
    if path.is_empty() {
        return None;
    }
    Some(RepoFile {
        path,
        insertions: num(value.get("insertions")),
        deletions: num(value.get("deletions")),
    })
}

fn linked_of(value: &Value) -> Option<LinkedChange> {
    if value.is_null() {
        return None;
    }
    let number = num(value.get("number"));
    if number == 0 {
        return None;
    }
    Some(LinkedChange {
        number,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        url: public_url(value.get("url").and_then(Value::as_str).unwrap_or("")),
        state: value
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        draft: value
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_pulls(value: &Value) -> Result<Vec<PullRow>, HarnessError> {
    let Some(obj) = value.as_object() else {
        return Err(HarnessError::failed("pull request list was not an object"));
    };
    let entries = obj.get("entries").and_then(Value::as_array);
    let rows: Vec<PullRow> = entries
        .into_iter()
        .flatten()
        .filter_map(row_of)
        .take(PR_CAP)
        .collect();
    if rows.is_empty() {
        if let Some(message) = list_error(obj.get("errors")) {
            return Err(HarnessError::failed(message));
        }
    }
    Ok(rows)
}

fn row_of(value: &Value) -> Option<PullRow> {
    let number = num(value.get("number"));
    if number == 0 {
        return None;
    }
    Some(PullRow {
        number,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        url: public_url(value.get("url").and_then(Value::as_str).unwrap_or("")),
        draft: value
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        checks: value
            .get("checksState")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        review: value
            .get("reviewDecision")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        updated: value
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

fn list_error(errors: Option<&Value>) -> Option<String> {
    let items = errors?.as_array()?;
    let message = items
        .iter()
        .filter_map(|item| item.get("message").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("; ");
    if message.is_empty() {
        None
    } else {
        Some(message)
    }
}

fn num(value: Option<&Value>) -> u64 {
    value
        .and_then(Value::as_u64)
        .or_else(|| {
            value
                .and_then(Value::as_i64)
                .and_then(|n| u64::try_from(n).ok())
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_in_a_url_is_removed() {
        assert_eq!(
            public_url("https://user:token@github.com/org/repo/pull/12"),
            "https://github.com/org/repo/pull/12"
        );
        assert_eq!(
            public_url("https://github.com/org/repo"),
            "https://github.com/org/repo"
        );
        assert_eq!(
            public_url("https://github.com/org/name@2"),
            "https://github.com/org/name@2"
        );
    }

    #[test]
    fn status_keeps_fifty_files_and_strips_the_provider_url() {
        let mut files = Vec::new();
        for i in 0..60 {
            files.push(json!({ "path": format!("f{i}.rs"), "insertions": 1, "deletions": 0 }));
        }
        let status = parse_status(&json!({
            "isRepo": true,
            "sourceControlProvider": {
                "kind": "github",
                "name": "GitHub",
                "baseUrl": "https://user:token@github.com"
            },
            "hasPrimaryRemote": true,
            "isDefaultRef": false,
            "refName": "feature",
            "hasWorkingTreeChanges": true,
            "workingTree": { "files": files, "insertions": 60, "deletions": 1 },
            "hasUpstream": true,
            "aheadCount": 2,
            "behindCount": 1,
            "pr": {
                "number": 12,
                "title": "Fix",
                "url": "https://user:token@github.com/org/repo/pull/12",
                "baseRef": "main",
                "headRef": "feature",
                "state": "open",
                "isDraft": true
            }
        }))
        .unwrap();
        assert!(status.is_repo);
        assert_eq!(status.provider, "GitHub");
        assert_eq!(status.base_url, "https://github.com");
        assert_eq!(status.branch, "feature");
        assert!(!status.default_branch);
        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 1);
        assert_eq!(status.insertions, 60);
        assert_eq!(status.file_total, 60);
        assert_eq!(status.files.len(), FILE_CAP);
        let linked = status.linked.unwrap();
        assert_eq!(linked.number, 12);
        assert!(linked.draft);
        assert_eq!(linked.url, "https://github.com/org/repo/pull/12");
    }

    #[test]
    fn a_folder_that_is_not_a_repository_has_no_details() {
        let status = parse_status(&json!({ "isRepo": false })).unwrap();
        assert!(!status.is_repo);
        assert!(status.branch.is_empty());
        assert!(status.linked.is_none());
    }

    #[test]
    fn an_open_list_keeps_twenty_rows_and_a_host_error_when_empty() {
        let mut entries = Vec::new();
        for n in 1..=25 {
            entries.push(json!({
                "number": n,
                "title": format!("Change {n}"),
                "url": format!("https://user:pass@github.com/org/repo/pull/{n}"),
                "isDraft": false,
                "checksState": "failing",
                "reviewDecision": "approved",
                "updatedAt": "2026-01-02T00:00:00.000Z"
            }));
        }
        let rows = parse_pulls(&json!({
            "entries": entries,
            "errors": []
        }))
        .unwrap();
        assert_eq!(rows.len(), PR_CAP);
        assert_eq!(rows[0].checks, "failing");
        assert_eq!(rows[0].review, "approved");
        assert_eq!(rows[0].url, "https://github.com/org/repo/pull/1");

        let err = parse_pulls(&json!({
            "entries": [],
            "errors": [{ "projectId": "p", "projectTitle": "Ada", "message": "cli-unauthenticated" }]
        }))
        .unwrap_err();
        assert!(err.to_string().contains("cli-unauthenticated"));
    }
}
