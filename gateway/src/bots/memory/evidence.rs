//! Evidence a memory write can trust: words the user typed, not a paste or a quote.
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(```|~~~)").unwrap());
static FORWARD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(?:-{2,}\s*forwarded message|forwarded message|begin forwarded message|-{2,}\s*mensaje reenviado|mensaje reenviado)",
    )
    .unwrap()
});

const ACKNOWLEDGEMENTS: [&str; 20] = [
    "ok",
    "okay",
    "okey",
    "yes",
    "no",
    "thanks",
    "thank you",
    "got it",
    "sure",
    "cool",
    "dale",
    "listo",
    "perfecto",
    "gracias",
    "sí",
    "si",
    "vale",
    "bien",
    "claro",
    "de acuerdo",
];

const GENERIC_ATTACHMENTS: [&str; 10] = [
    "look at this",
    "check this",
    "read this",
    "see this",
    "open this",
    "mira esto",
    "revisa esto",
    "lee esto",
    "mira este archivo",
    "revisa este archivo",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvidenceRefusal {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnerMessage {
    pub id: String,
    pub text: String,
    pub attachments: usize,
}

pub(super) fn normalize_newlines(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

/// Runs the user typed. Fences, quotes, and forwarded blocks are left out.
pub(super) fn typed_runs(text: &str) -> Vec<String> {
    let mut runs = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut fenced = false;
    let mut forwarded = false;
    let flush = |current: &mut Vec<String>, runs: &mut Vec<String>| {
        if !current.is_empty() {
            runs.push(current.join(" "));
            current.clear();
        }
    };
    for line in normalize_newlines(text).split('\n') {
        let trimmed = line.trim();
        if FENCE.is_match(trimmed) {
            fenced = !fenced;
            flush(&mut current, &mut runs);
            continue;
        }
        if fenced {
            continue;
        }
        if trimmed.starts_with('>') || trimmed.starts_with('＞') {
            flush(&mut current, &mut runs);
            continue;
        }
        if FORWARD.is_match(trimmed) {
            forwarded = true;
            flush(&mut current, &mut runs);
            continue;
        }
        if forwarded {
            if trimmed.is_empty() {
                forwarded = false;
            }
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut current, &mut runs);
            continue;
        }
        current.push(trimmed.to_string());
    }
    flush(&mut current, &mut runs);
    runs
}

pub(super) fn evidence_text(value: &str) -> String {
    let nfc: String = value.nfc().collect();
    let lower = nfc.to_lowercase();
    let mut out = String::new();
    let mut spaced = false;
    for ch in lower.chars() {
        if ch.is_alphanumeric() || ch.is_whitespace() {
            if ch.is_whitespace() {
                if !spaced && !out.is_empty() {
                    out.push(' ');
                    spaced = true;
                }
            } else {
                out.push(ch);
                spaced = false;
            }
        } else if !spaced && !out.is_empty() {
            out.push(' ');
            spaced = true;
        }
    }
    out.trim().to_string()
}

pub(crate) fn check_owner_evidence(text: &str, quote: &str) -> Result<String, EvidenceRefusal> {
    if text.trim().is_empty() || quote.trim().is_empty() {
        return Err(refusal(
            "missing",
            "Pass the user's exact words in sourceQuote.",
        ));
    }
    let normalized = evidence_text(quote);
    if normalized.is_empty() {
        return Err(refusal("missing", "sourceQuote is empty."));
    }
    if normalized.split(' ').count() < 3 {
        return Err(refusal(
            "too_short",
            "sourceQuote is too short to be evidence. Quote a full phrase.",
        ));
    }
    if ACKNOWLEDGEMENTS.contains(&normalized.as_str()) {
        return Err(refusal(
            "acknowledgement",
            "An acknowledgement is not evidence of a lasting fact.",
        ));
    }
    let runs: Vec<String> = typed_runs(text)
        .iter()
        .map(|run| evidence_text(run))
        .collect();
    if !runs.iter().any(|run| run.contains(&normalized)) {
        return Err(refusal(
            "not_typed",
            "That text was pasted, quoted, in a code block, forwarded or written by a Bot, not typed by the user. Ask the user to state the fact in their own words.",
        ));
    }
    Ok(quote.trim().to_string())
}

pub(crate) fn generic_attachment_quote(quote: &str) -> bool {
    GENERIC_ATTACHMENTS.contains(&evidence_text(quote).as_str())
}

/// Owner messages from a thread projection. Routine and group frames are not evidence.
pub(crate) fn owner_messages(items: &[Value]) -> Vec<OwnerMessage> {
    super::owner_stream::owner_stream(items)
        .into_iter()
        .map(|turn| OwnerMessage {
            id: turn.id,
            text: turn.text,
            attachments: turn.attachments,
        })
        .collect()
}

pub(crate) fn pick_message<'a>(
    messages: &'a [OwnerMessage],
    id: Option<&str>,
) -> Result<&'a OwnerMessage, String> {
    if let Some(id) = id.map(str::trim).filter(|id| !id.is_empty()) {
        return messages.iter().find(|message| message.id == id).ok_or_else(|| {
            "I couldn't find that message in this Bot's conversation. Use the message id from T3 or leave sourceMessageId out to use the latest user message.".to_string()
        });
    }
    messages.last().ok_or_else(|| {
        "I couldn't find a user message to use as evidence. Propose the fact with memory_suggest, and the person can accept it.".to_string()
    })
}

/// How a memory write shows it is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceKind {
    /// Words the person typed.
    OwnerQuote,
    /// The person accepted a suggestion. No typed quote is required.
    Accepted,
}

pub(crate) const ACCEPTED_VIA: &str = "accepted-suggestion";

pub(crate) fn accepted_quote(now: &str) -> String {
    format!("accepted by the person at {now}")
}

pub(crate) fn is_accepted_quote(quote: &str) -> bool {
    quote
        .trim()
        .strip_prefix("accepted by the person at ")
        .is_some_and(|at| !at.trim().is_empty())
}

fn refusal(code: &'static str, message: impl Into<String>) -> EvidenceRefusal {
    EvidenceRefusal {
        code,
        message: message.into(),
    }
}
