//! A Bot cannot replace a fact the person wrote. A direct edit of the user file is put back.
use super::{
    blank_state, checked_path, file_rel, journal_text, lock_for, read_text, remove_file,
    render_facts, slug_ok, state_rel, write_state, UserFact, UserNotice, UserState,
};
use crate::bots::agent_context::plain_prompt;
use crate::bots::memory::facts::{hash_fact, local_date, parse_fact};
use crate::bots::memory::state::revision_of;
use std::collections::BTreeSet;
use std::io;
use std::path::Path;

pub(super) const OWNED_MESSAGE: &str =
    "That memory is yours to change. Ask the person to edit it in Memory.";

pub(super) const RESTORED_NOTICE: &str = "Restored 1 memory you wrote";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UserWrite {
    Add,
    Replace,
    Forget,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UserBlock {
    Owned,
    Overlap(String),
}

impl UserBlock {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::Owned => OWNED_MESSAGE.to_string(),
            Self::Overlap(text) => format!(
                "That overlaps a memory you wrote: \"{text}\". Ask the person to edit it in Memory."
            ),
        }
    }

    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Owned => "user_owned",
            Self::Overlap(_) => "overlap",
        }
    }
}

/// `Some` when this Bot write must be rejected. The rejection is recorded at most once per fact per day.
pub(crate) async fn refuse_user_change(
    root: &Path,
    slug: &str,
    kind: UserWrite,
    target: &str,
    incoming: &str,
    bot_name: &str,
    now: &str,
) -> io::Result<Option<UserBlock>> {
    if slug_ok(slug).is_err() {
        return Ok(None);
    }
    let facts = if kind == UserWrite::Other {
        Vec::new()
    } else {
        super::load(root, slug).await?
    };
    let block = match kind {
        UserWrite::Forget => {
            match_owned(&facts, target).map(|fact| (fact.hash.clone(), UserBlock::Owned))
        }
        UserWrite::Replace => match match_owned(&facts, target) {
            Some(fact) => Some((fact.hash.clone(), UserBlock::Owned)),
            None => near_duplicate(&facts, incoming),
        },
        UserWrite::Add => near_duplicate(&facts, incoming),
        UserWrite::Other => None,
    };
    let Some((hash, block)) = block else {
        return Ok(None);
    };
    if let Err(err) = note_rejection(root, slug, &hash, bot_name, now).await {
        tracing::warn!("user memory rejection for {slug} was not recorded: {err}");
    }
    Ok(Some(block))
}

/// Put the user file back when it differs from the state's canonical text. One notice per restore.
pub(crate) async fn verify_user_facts(root: &Path, slug: &str, now: &str) -> io::Result<bool> {
    if slug_ok(slug).is_err() {
        return Ok(false);
    }
    let _guard = lock_for(slug).lock_owned().await;
    let state_path = checked_path(root, &state_rel(slug)).await?;
    let Some(raw) = read_text(&state_path).await? else {
        return Ok(false);
    };
    let Ok(mut state) = serde_json::from_str::<UserState>(&raw) else {
        tracing::warn!("user memory state for {slug} was unreadable; leaving the file");
        return Ok(false);
    };
    let canonical = render_facts(&state.facts);
    let file_path = checked_path(root, &file_rel(slug)).await?;
    let current = read_text(&file_path).await?.unwrap_or_default();
    if current == canonical {
        let hash = revision_of(&canonical);
        if state.revision != hash {
            state.revision = hash;
            write_state(&state_path, &state).await?;
        }
        return Ok(false);
    }
    let before = state.clone();
    state.revision = revision_of(&canonical);
    state.notices.push(UserNotice {
        kind: "restored".to_string(),
        text: RESTORED_NOTICE.to_string(),
        at: now.to_string(),
        fact_hash: None,
    });
    let op_id = uuid::Uuid::new_v4().to_string();
    let journal_rel = format!(".botapp/memory-user/{slug}.journal/{op_id}.json");
    let journal_path = checked_path(root, &journal_rel).await?;
    let body = journal_text(
        &op_id,
        slug,
        &revision_of(&current),
        &canonical,
        &before,
        &state,
        now,
    )?;
    crate::storage::write_atomic(&journal_path, body.as_bytes(), Some(0o600)).await?;
    let wrote = async {
        crate::storage::write_atomic(&file_path, canonical.as_bytes(), Some(0o644)).await?;
        write_state(&state_path, &state).await
    }
    .await;
    if wrote.is_ok() {
        if let Err(err) = remove_file(&journal_path).await {
            tracing::warn!("user memory journal {op_id} could not be removed: {err}");
        }
        tracing::info!(slug, "restored user memory");
    }
    wrote?;
    Ok(true)
}

fn match_owned<'a>(facts: &'a [UserFact], target: &str) -> Option<&'a UserFact> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    facts.iter().find(|fact| fact_matches(fact, target))
}

fn fact_matches(fact: &UserFact, target: &str) -> bool {
    let stored = hash_fact(&fact.text);
    let raw = hash_fact(target);
    let parsed = hash_fact(&parse_fact(target).text);
    fact.hash == raw
        || fact.hash == parsed
        || stored == raw
        || stored == parsed
        || plain_prompt(&fact.text) == target
}

fn near_duplicate(facts: &[UserFact], incoming: &str) -> Option<(String, UserBlock)> {
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return None;
    }
    facts.iter().find_map(|fact| {
        token_overlap(&fact.text, incoming)
            .then(|| (fact.hash.clone(), UserBlock::Overlap(fact.text.clone())))
    })
}

/// Token Jaccard at or above 0.8. Compared as integers so 4/5 counts and 3/4 does not.
pub(super) fn token_overlap(left: &str, right: &str) -> bool {
    let left = tokens(left);
    let right = tokens(right);
    if left.is_empty() || right.is_empty() {
        return false;
    }
    let inter = left.intersection(&right).count();
    let union = left.union(&right).count();
    inter.saturating_mul(5) >= union.saturating_mul(4)
}

fn tokens(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            out.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.insert(current);
    }
    out
}

async fn note_rejection(
    root: &Path,
    slug: &str,
    fact_hash: &str,
    bot_name: &str,
    now: &str,
) -> io::Result<()> {
    let _guard = lock_for(slug).lock_owned().await;
    let state_path = checked_path(root, &state_rel(slug)).await?;
    let file_path = checked_path(root, &file_rel(slug)).await?;
    let file_text = read_text(&file_path).await?.unwrap_or_default();
    let mut state = match read_text(&state_path).await? {
        Some(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| blank_state(&file_text)),
        None => blank_state(&file_text),
    };
    let day = day_of(now);
    let already = state.notices.iter().any(|notice| {
        notice.kind == "rejected"
            && notice.fact_hash.as_deref() == Some(fact_hash)
            && day_of(&notice.at) == day
    });
    if already {
        return Ok(());
    }
    state.notices.push(UserNotice {
        kind: "rejected".to_string(),
        text: format!(
            "{} tried to change a memory you wrote",
            notice_name(bot_name, slug)
        ),
        at: now.to_string(),
        fact_hash: Some(fact_hash.to_string()),
    });
    write_state(&state_path, &state).await
}

fn day_of(at: &str) -> String {
    let day = local_date(at);
    if day.is_empty() {
        at.to_string()
    } else {
        day
    }
}

fn notice_name(name: &str, slug: &str) -> String {
    let line = name.lines().next().unwrap_or("").trim();
    let line = if line.is_empty() { slug } else { line };
    let mut out = String::new();
    for ch in line.chars().take(80) {
        if !ch.is_control() {
            out.push(ch);
        }
    }
    if out.is_empty() {
        slug.to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::token_overlap;

    #[test]
    fn overlap_includes_four_fifths_and_stops_below_it() {
        assert!(token_overlap(
            "alpha beta gamma delta",
            "alpha beta gamma delta epsilon"
        ));
        assert!(!token_overlap(
            "alpha beta gamma delta",
            "alpha beta gamma epsilon"
        ));
        assert!(token_overlap("likes tea", "Likes tea."));
        assert!(!token_overlap("likes tea", "lives in paris"));
        assert!(!token_overlap("", "likes tea"));
    }
}
