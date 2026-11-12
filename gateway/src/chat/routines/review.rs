//! Review of routines a Bot creates for itself.
//!
//! The store is `bots/.botapp/routine-review.json`, written with `storage::write_atomic`.
//! On the first start, `baseline` is set to now and every task that already exists is
//! grandfathered. Only a task first seen after that is reviewed. The default mode is
//! ask, an agent interval repeats no faster than every 15 minutes, and rejecting a
//! routine deletes it.
//!
//! `decide` does not talk to the harness. A later watcher applies the [`Action`].
//! Load and save go through [`store::ReviewFile`]. The store keeps the decision,
//! not the routine prompt.

use crate::bots::runtime_mode::{turn_mode, RuntimeMode};
use serde_json::Value;

pub(crate) mod store;

#[cfg(test)]
mod tests;

pub(crate) use store::{ReviewItem, ReviewState, ReviewStore};

/// How many routines one Bot may have, enabled and pending together.
pub(crate) const DEFAULT_MAX_ROUTINES: u32 = 10;

/// How many routines may wait for a decision at once.
pub(crate) const DEFAULT_MAX_PENDING: u32 = 5;

/// Shortest repeat for a routine a Bot created. T3 Code allows one minute.
pub(crate) const INTERVAL_FLOOR_MS: u64 = 15 * 60 * 1000;

/// Consecutive failed runs before a routine is paused.
pub(crate) const FAILURE_LIMIT: u32 = 3;

/// Whether a Bot's new routines wait for the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReviewMode {
    Ask,
    Trust,
}

impl ReviewMode {
    pub(crate) const DEFAULT: Self = Self::Ask;

    pub(crate) fn parse(raw: Option<&str>) -> Option<Self> {
        match raw {
            Some("ask") => Some(Self::Ask),
            Some("trust") => Some(Self::Trust),
            Some(_) | None => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Trust => "trust",
        }
    }
}

/// Why a routine was held or deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    AgentCreated,
    Webhook,
    RoutineCap,
    PendingCap,
}

impl Reason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::AgentCreated => "agent-created",
            Self::Webhook => "webhook",
            Self::RoutineCap => "routine-cap",
            Self::PendingCap => "pending-cap",
        }
    }
}

/// Numbers the rules use. Phase 4 reads these from settings. The defaults are the decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    pub max_routines: u32,
    pub max_pending: u32,
    pub interval_floor_ms: u64,
    pub failure_limit: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_routines: DEFAULT_MAX_ROUTINES,
            max_pending: DEFAULT_MAX_PENDING,
            interval_floor_ms: INTERVAL_FLOOR_MS,
            failure_limit: FAILURE_LIMIT,
        }
    }
}

/// The Bot the task belongs to, and the counts already taken by its other routines.
/// `enabled` and `pending` do not include the task being decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReviewContext {
    pub mode: ReviewMode,
    pub runtime_cap: RuntimeMode,
    pub enabled: u32,
    pub pending: u32,
    pub limits: Limits,
}

impl ReviewContext {
    pub(crate) fn ask(runtime_cap: RuntimeMode) -> Self {
        Self {
            mode: ReviewMode::DEFAULT,
            runtime_cap,
            enabled: 0,
            pending: 0,
            limits: Limits::default(),
        }
    }
}

/// What to do with one task. `every_ms` and `runtime_mode` are the fields to rewrite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub(crate) enum Action {
    Leave,
    Hold {
        reason: Reason,
        every_ms: Option<u64>,
        runtime_mode: Option<&'static str>,
    },
    Delete {
        reason: Reason,
    },
    Revise {
        every_ms: Option<u64>,
        runtime_mode: Option<&'static str>,
    },
    Pause,
}

/// A task a Bot created through its own tools.
pub(crate) fn agent_authored(task: &Value) -> bool {
    task["createdBy"].as_str() == Some("agent")
        && matches!(task["creationSource"].as_str(), Some("mcp" | "provider"))
}

/// `Some(floor)` when this interval repeats faster than the floor.
pub(crate) fn raised_interval(task: &Value, floor_ms: u64) -> Option<u64> {
    if task["schedule"]["type"].as_str() != Some("interval") {
        return None;
    }
    let every = task["schedule"]["everyMs"].as_u64().unwrap_or(0);
    (every < floor_ms).then_some(floor_ms)
}

/// The Bot's own mode when the task asks for something broader.
/// An unknown or missing mode is left for the harness to reject.
pub(crate) fn clamped_runtime(task: &Value, cap: RuntimeMode) -> Option<&'static str> {
    turn_mode(task["runtimeMode"].as_str(), cap)
}

/// Enabled plus pending is already at the cap, so another agent routine does not fit.
pub(crate) fn routine_cap_exceeded(enabled_plus_pending: u32, max_routines: u32) -> bool {
    enabled_plus_pending >= max_routines
}

/// Too many routines are already waiting.
pub(crate) fn pending_cap_exceeded(pending: u32, max_pending: u32) -> bool {
    pending >= max_pending
}

/// A limit of zero turns the breaker off.
pub(crate) fn failure_breaker(consecutive: u32, limit: u32) -> bool {
    limit > 0 && consecutive >= limit
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunStatus {
    Never,
    Running,
    Succeeded,
    Failed,
    Other,
}

pub(crate) fn run_status(raw: Option<&str>) -> RunStatus {
    match raw {
        Some("never") => RunStatus::Never,
        Some("running") => RunStatus::Running,
        Some("succeeded") => RunStatus::Succeeded,
        Some("failed") => RunStatus::Failed,
        Some(_) | None => RunStatus::Other,
    }
}

/// Count one finished run. Call this once per run, not once per snapshot.
/// `failed` adds one. `succeeded` clears the streak. Anything else leaves it.
pub(crate) fn note_run(store: &mut ReviewStore, task_id: &str, status: RunStatus) {
    if task_id.is_empty() {
        return;
    }
    let streak = store.failure_count(task_id);
    match status {
        RunStatus::Failed => {
            store
                .failures
                .insert(task_id.to_string(), streak.saturating_add(1));
        }
        RunStatus::Succeeded => {
            store.failures.remove(task_id);
        }
        RunStatus::Never | RunStatus::Running | RunStatus::Other => {}
    }
}

pub(crate) fn clear_failures(store: &mut ReviewStore, task_id: &str) {
    store.failures.remove(task_id);
}

struct Patch {
    every_ms: Option<u64>,
    runtime_mode: Option<&'static str>,
}

fn changes(task: &Value, ctx: &ReviewContext) -> Patch {
    Patch {
        every_ms: raised_interval(task, ctx.limits.interval_floor_ms),
        runtime_mode: clamped_runtime(task, ctx.runtime_cap),
    }
}

fn revise(patch: Patch) -> Action {
    if patch.every_ms.is_none() && patch.runtime_mode.is_none() {
        Action::Leave
    } else {
        Action::Revise {
            every_ms: patch.every_ms,
            runtime_mode: patch.runtime_mode,
        }
    }
}

fn hold_reason(task: &Value) -> Reason {
    if task["schedule"]["type"].as_str() == Some("webhook") {
        Reason::Webhook
    } else {
        Reason::AgentCreated
    }
}

fn first_sight(task: &Value, ctx: &ReviewContext, patch: Patch) -> Action {
    let occupied = ctx.enabled.saturating_add(ctx.pending);
    if routine_cap_exceeded(occupied, ctx.limits.max_routines) {
        return Action::Delete {
            reason: Reason::RoutineCap,
        };
    }
    if pending_cap_exceeded(ctx.pending, ctx.limits.max_pending) {
        return Action::Delete {
            reason: Reason::PendingCap,
        };
    }
    Action::Hold {
        reason: hold_reason(task),
        every_ms: patch.every_ms,
        runtime_mode: patch.runtime_mode,
    }
}

fn task_enabled(task: &Value) -> bool {
    task["enabled"].as_bool() == Some(true)
}

/// `None` for the Bot means the task is not one of ours.
/// An unsealed store leaves every task alone, so the first snapshot can grandfather them.
pub(crate) fn decide(task: &Value, store: &ReviewStore, ctx: Option<&ReviewContext>) -> Action {
    let Some(ctx) = ctx else {
        return Action::Leave;
    };
    if store.baseline.is_none() {
        return Action::Leave;
    }
    let Some(id) = task["id"].as_str().filter(|id| !id.is_empty()) else {
        return Action::Leave;
    };
    if store.grandfathered.contains(id) {
        return Action::Leave;
    }
    if !agent_authored(task) || ctx.mode == ReviewMode::Trust {
        return Action::Leave;
    }
    let patch = changes(task, ctx);
    match store.items.get(id).map(|item| item.state) {
        Some(ReviewState::Rejected) => Action::Leave,
        Some(ReviewState::Pending) => revise(patch),
        Some(ReviewState::Approved) => {
            if task_enabled(task)
                && failure_breaker(store.failure_count(id), ctx.limits.failure_limit)
            {
                Action::Pause
            } else {
                revise(patch)
            }
        }
        None => first_sight(task, ctx, patch),
    }
}

/// Remember a hold or a deletion. A rewrite or a pause does not change the review state.
/// The routine prompt is not copied. Record this after the change is kept, so a failed
/// write can be tried again.
pub(crate) fn record(
    store: &mut ReviewStore,
    task_id: &str,
    slug: &str,
    action: &Action,
    now: &str,
) {
    if task_id.is_empty() {
        return;
    }
    match action {
        Action::Leave | Action::Revise { .. } | Action::Pause => {}
        Action::Hold { reason, .. } => {
            store
                .items
                .entry(task_id.to_string())
                .or_insert_with(|| ReviewItem {
                    state: ReviewState::Pending,
                    slug: slug.to_string(),
                    flagged_at: now.to_string(),
                    decided_at: None,
                    reason: reason.as_str().to_string(),
                });
        }
        Action::Delete { reason } => {
            let (flagged_at, kept_slug) = match store.items.get(task_id) {
                Some(item) => {
                    let flagged_at = item.flagged_at.clone();
                    let kept_slug = if item.slug.is_empty() {
                        slug.to_string()
                    } else {
                        item.slug.clone()
                    };
                    (flagged_at, kept_slug)
                }
                None => (now.to_string(), slug.to_string()),
            };
            store.items.insert(
                task_id.to_string(),
                ReviewItem {
                    state: ReviewState::Rejected,
                    slug: kept_slug,
                    flagged_at,
                    decided_at: Some(now.to_string()),
                    reason: reason.as_str().to_string(),
                },
            );
        }
    }
}

/// The person's decision on a routine that is waiting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    Approve,
    Reject,
}

/// Approve turns the routine on and clears its failure streak.
/// Reject deletes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub(crate) enum Resolution {
    Enable,
    Delete,
    Absent,
}

pub(crate) fn resolve(
    store: &mut ReviewStore,
    task_id: &str,
    choice: Choice,
    now: &str,
) -> Resolution {
    let outcome = {
        let Some(item) = store.items.get_mut(task_id) else {
            return Resolution::Absent;
        };
        if item.state != ReviewState::Pending {
            return Resolution::Absent;
        }
        item.decided_at = Some(now.to_string());
        match choice {
            Choice::Approve => {
                item.state = ReviewState::Approved;
                Resolution::Enable
            }
            Choice::Reject => {
                item.state = ReviewState::Rejected;
                Resolution::Delete
            }
        }
    };
    clear_failures(store, task_id);
    outcome
}
