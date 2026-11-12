//! `bots/.botapp/routine-review.json`. A missing file is an empty store.
//! A damaged file, or one from a newer hbots, is an error so a later save cannot overwrite it.

use crate::platform::error::{ApiError, ApiResult};
use crate::storage::write_atomic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "routine-review.json";

pub(crate) const STORE_VERSION: u32 = 1;

/// Saved choice for one task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ReviewState {
    Pending,
    Approved,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewItem {
    pub state: ReviewState,
    pub slug: String,
    pub flagged_at: String,
    #[serde(default)]
    pub decided_at: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewStore {
    #[serde(default = "store_version")]
    pub version: u32,
    #[serde(default)]
    pub baseline: Option<String>,
    /// Task ids present when the baseline was sealed. They are never flagged.
    #[serde(default)]
    pub grandfathered: BTreeSet<String>,
    #[serde(default)]
    pub items: BTreeMap<String, ReviewItem>,
    #[serde(default)]
    pub failures: BTreeMap<String, u32>,
    /// Last finished run already counted. A later snapshot that still shows it is not counted again.
    #[serde(default)]
    pub seen_runs: BTreeMap<String, String>,
}

fn store_version() -> u32 {
    STORE_VERSION
}

impl ReviewStore {
    pub(crate) fn empty() -> Self {
        Self {
            version: STORE_VERSION,
            baseline: None,
            grandfathered: BTreeSet::new(),
            items: BTreeMap::new(),
            failures: BTreeMap::new(),
            seen_runs: BTreeMap::new(),
        }
    }

    /// First start only. A second call keeps the original baseline and ids.
    pub(crate) fn seal_baseline<I, S>(&mut self, existing: I, now: &str)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if self.baseline.is_some() {
            return;
        }
        self.baseline = Some(now.to_string());
        for id in existing {
            let id = id.as_ref();
            if !id.is_empty() {
                self.grandfathered.insert(id.to_string());
            }
        }
    }

    pub(crate) fn failure_count(&self, task_id: &str) -> u32 {
        self.failures.get(task_id).copied().unwrap_or(0)
    }
}

#[derive(Debug)]
pub(crate) struct ReviewFile {
    path: PathBuf,
}

impl ReviewFile {
    pub(crate) fn new(botapp: &Path) -> Self {
        Self {
            path: botapp.join(FILE_NAME),
        }
    }

    pub(crate) async fn load(&self) -> ApiResult<ReviewStore> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => parse_store(&text).map_err(|message| ApiError::new(500, message)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(ReviewStore::empty()),
            Err(err) => Err(err.into()),
        }
    }

    pub(crate) async fn save(&self, store: &ReviewStore) -> ApiResult<()> {
        let text = serde_json::to_string_pretty(store).map_err(|err| {
            ApiError::new(
                500,
                format!("routine-review.json could not be saved: {err}"),
            )
        })?;
        write_atomic(&self.path, format!("{text}\n").as_bytes(), Some(0o600)).await?;
        Ok(())
    }
}

fn parse_store(text: &str) -> Result<ReviewStore, &'static str> {
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!("routine review store: {err}");
            return Err("routine-review.json is not valid");
        }
    };
    if !value.is_object() {
        return Err("routine-review.json is not valid");
    }
    if let Some(version) = value.get("version") {
        match version.as_u64() {
            Some(1) => {}
            Some(newer) if newer > 1 => {
                return Err("routine-review.json is from a newer hbots");
            }
            Some(_) | None => return Err("routine-review.json is not valid"),
        }
    }
    match serde_json::from_value(value) {
        Ok(store) => Ok(store),
        Err(err) => {
            tracing::warn!("routine review store: {err}");
            Err("routine-review.json is not valid")
        }
    }
}
