//! A short note about a routine the watcher held, changed, or removed.
//! The text names the routine and its schedule. It does not copy the prompt.

use super::{Action, Limits, Reason};
use crate::storage::write_atomic;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "routine-notices.json";
const LIMIT: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoticeBody {
    pub kind: &'static str,
    pub text: String,
}

/// What the person can read. `None` when the action does not need a note.
pub(crate) fn describe(
    name: &str,
    title: &str,
    schedule: &str,
    action: &Action,
    limits: Limits,
) -> Option<NoticeBody> {
    let text = match action {
        Action::Leave => return None,
        Action::Hold {
            every_ms,
            runtime_mode,
            ..
        } => hold_text(name, title, schedule, *every_ms, *runtime_mode),
        Action::Revise {
            every_ms,
            runtime_mode,
        } => revise_text(name, title, *every_ms, *runtime_mode)?,
        Action::Delete { reason } => delete_text(name, title, *reason, limits),
        Action::Pause => format!("{name}'s routine {title} was paused after repeated failures."),
    };
    let kind = match action {
        Action::Leave => return None,
        Action::Hold { .. } => "pending",
        Action::Revise { .. } => "revised",
        Action::Delete { .. } => "deleted",
        Action::Pause => "paused",
    };
    Some(NoticeBody { kind, text })
}

fn hold_text(
    name: &str,
    title: &str,
    schedule: &str,
    every_ms: Option<u64>,
    runtime_mode: Option<&str>,
) -> String {
    let mut text = format!("{name} wants to run {title} ({schedule}). It is waiting for your OK.");
    if let Some(ms) = every_ms {
        text.push_str(&format!(" The repeat is now {}.", every_phrase(ms)));
    }
    if let Some(mode) = runtime_mode {
        text.push_str(&format!(" It will run with {mode}."));
    }
    text
}

fn revise_text(
    name: &str,
    title: &str,
    every_ms: Option<u64>,
    runtime_mode: Option<&str>,
) -> Option<String> {
    Some(match (every_ms, runtime_mode) {
        (Some(ms), Some(mode)) => format!(
            "{name}'s routine {title} now repeats {} and runs with {mode}.",
            every_phrase(ms)
        ),
        (Some(ms), None) => {
            format!("{name}'s routine {title} now repeats {}.", every_phrase(ms))
        }
        (None, Some(mode)) => format!("{name}'s routine {title} now runs with {mode}."),
        (None, None) => return None,
    })
}

fn delete_text(name: &str, title: &str, reason: Reason, limits: Limits) -> String {
    let why = match reason {
        Reason::RoutineCap => format!(
            "A Bot can have {} routines waiting or turned on.",
            limits.max_routines
        ),
        Reason::PendingCap => format!(
            "{} routines are already waiting for your OK.",
            limits.max_pending
        ),
        Reason::AgentCreated | Reason::Webhook => "It was removed.".to_string(),
    };
    format!("{name}'s routine {title} was removed. {why}")
}

fn every_phrase(ms: u64) -> String {
    if ms > 0 && ms.is_multiple_of(3_600_000) {
        let hours = ms / 3_600_000;
        if hours == 1 {
            "every hour".to_string()
        } else {
            format!("every {hours} hours")
        }
    } else {
        let minutes = (ms / 60_000).max(1);
        if minutes == 1 {
            "every minute".to_string()
        } else {
            format!("every {minutes} minutes")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Notice {
    pub kind: String,
    pub id: String,
    pub slug: String,
    pub title: String,
    pub schedule: String,
    pub text: String,
    pub at: String,
}

#[derive(Debug)]
pub(crate) struct NoticeFile {
    path: PathBuf,
}

impl NoticeFile {
    pub(crate) fn new(botapp: &Path) -> Self {
        Self {
            path: botapp.join(FILE_NAME),
        }
    }

    pub(crate) async fn load(&self) -> Result<Vec<Notice>, String> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => serde_json::from_str(&text).map_err(|err| {
                tracing::warn!("routine notices: {err}");
                "routine-notices.json is not valid".to_string()
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(err) => Err(err.to_string()),
        }
    }

    pub(crate) async fn append(&self, notice: &Notice) -> Result<(), String> {
        let mut items = self.load().await?;
        items.insert(0, notice.clone());
        items.truncate(LIMIT);
        let text = serde_json::to_string_pretty(&items)
            .map_err(|err| format!("routine-notices.json could not be saved: {err}"))?;
        write_atomic(&self.path, format!("{text}\n").as_bytes(), Some(0o600))
            .await
            .map_err(|err| err.to_string())
    }
}
