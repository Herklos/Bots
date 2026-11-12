//! `memory_suggest`: propose a fact. It does not write memory.
use super::{scope, Deps};
use crate::bots::memory::{propose, Proposal, ProposalResult};
use serde_json::{json, Value};

#[cfg(test)]
#[path = "suggest_tests.rs"]
mod tests;

pub(super) fn defs(thread: &Value) -> Vec<Value> {
    vec![json!({
        "name": "memory_suggest",
        "description": "Propose one fact for the person to accept or dismiss. This does not write memory. Use it when you have no words the person typed, including a routine run. text is one fact of at most 200 characters. why is why it is worth keeping, at most 300 characters. A duplicate of a saved fact, or of a suggestion the person dismissed, is not stored.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "The fact to propose, one line, 200 characters or fewer." },
                "why": { "type": "string", "description": "Why this is worth keeping, one line, 300 characters or fewer." },
                "category": { "type": "string", "description": "Optional: preference, decision, commitment, person, project, lesson, fact, risk, question." },
                "threadId": thread
            },
            "required": ["text", "why", "threadId"]
        }
    })]
}

pub(super) async fn suggest(d: &Deps<'_>, args: &Value) -> Result<Value, String> {
    let (slug, _) = scope(d, args).await?;
    let now = crate::vault::now_iso();
    let outcome = propose(
        d.memory,
        &Proposal {
            slug: &slug,
            text: args["text"].as_str().unwrap_or(""),
            why: args["why"].as_str().unwrap_or(""),
            category: args["category"].as_str(),
            now: &now,
            user_root: d.bots_root,
        },
    )
    .await
    .map_err(|err| {
        json!({
            "ok": false,
            "code": err.code,
            "message": err.message,
        })
        .to_string()
    })?;
    Ok(match outcome {
        ProposalResult::Stored { id, pending } => {
            json!({ "ok": true, "id": id, "pending": pending })
        }
        ProposalResult::Dropped => json!({ "ok": true, "dropped": true }),
    })
}
