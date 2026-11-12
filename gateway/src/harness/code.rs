//! Repository facts the Code tab shows. The T3 adapter fills these.
//! Callers never see a credential in a URL.

/// One changed file. `path` is relative to the worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoFile {
    pub path: String,
    pub insertions: u64,
    pub deletions: u64,
}

/// The pull request linked to the current branch, when the status names one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkedChange {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    pub draft: bool,
}

/// Local and remote repository state. Files are capped at [`FILE_CAP`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoStatus {
    pub is_repo: bool,
    pub provider: String,
    pub base_url: String,
    pub has_remote: bool,
    pub default_branch: bool,
    pub branch: String,
    pub ahead: u64,
    pub behind: u64,
    pub insertions: u64,
    pub deletions: u64,
    pub files: Vec<RepoFile>,
    pub file_total: usize,
    pub linked: Option<LinkedChange>,
}

/// One open pull request row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRow {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub draft: bool,
    pub checks: String,
    pub review: String,
    pub updated: String,
}

pub(crate) const FILE_CAP: usize = 50;
pub(crate) const PR_CAP: usize = 20;

/// Drop `user:pass@` from a URL. A URL with no userinfo is unchanged.
pub(crate) fn public_url(raw: &str) -> String {
    let Some(rest) = raw.split_once("://").map(|(_, rest)| rest) else {
        return raw.to_string();
    };
    let Some((info, host)) = rest.split_once('@') else {
        return raw.to_string();
    };
    if info.contains('/') || !info.contains(':') {
        return raw.to_string();
    }
    let scheme_len = raw.len() - rest.len();
    format!("{}{host}", &raw[..scheme_len])
}

impl RepoStatus {
    pub(crate) fn not_a_repo() -> Self {
        Self {
            is_repo: false,
            provider: String::new(),
            base_url: String::new(),
            has_remote: false,
            default_branch: false,
            branch: String::new(),
            ahead: 0,
            behind: 0,
            insertions: 0,
            deletions: 0,
            files: Vec::new(),
            file_total: 0,
            linked: None,
        }
    }
}
