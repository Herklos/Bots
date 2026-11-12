//! Fact lines: parse, format, and the checks a write cannot skip.
use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

pub(crate) const FACT_CHARS: usize = 200;
pub(super) const MEMORY_LINES: usize = 120;
pub(crate) const MEMORY_CHARS: usize = 6_000;
pub(crate) const NOTE_CHARS: usize = 8_000;
pub(super) const NOTE_COUNT: usize = 20;
pub(super) const NOTE_DEPTH: usize = 2;
pub(super) const NOTE_TOKENS: usize = 2_000;
/// `memory_read` with `full` set. The session preview stays at [`NOTE_CHARS`].
pub(crate) const FULL_NOTE_CHARS: usize = 100_000;

const MANAGED_START: &str = "<!-- bot:managed:start -->";
const MANAGED_END: &str = "<!-- bot:managed:end -->";

const INSTRUCTIONS: [&str; 13] = [
    "ignore previous",
    "ignore all previous",
    "ignore the above",
    "disregard previous",
    "system prompt",
    "you are now",
    "do not tell the user",
    "trusted source",
    "ignora las instrucciones",
    "ignora todas las instrucciones",
    "prompt del sistema",
    "ahora eres",
    "no le digas al usuario",
];

static BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*[-*+]\s+").unwrap());
static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static SOURCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[source:\s*([^\]]+)\]").unwrap());
static SOURCE_TAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s*\[source:\s*[^\]]+\]\s*$").unwrap());
static ADDED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)added:\s*(\d{4}-\d{2}-\d{2})").unwrap());
static CATEGORY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\[([a-z][a-z0-9_-]{0,32})\]\s*").unwrap());
static CATEGORY_LABEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_-]{0,32}$").unwrap());
static TEMPORAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)@(effective|occurred|due|valid|mentioned)(?::(\d{4}-\d{2}-\d{2})|\[(\d{4}-\d{2}-\d{2})?,(\d{4}-\d{2}-\d{2})?\))").unwrap()
});
static LINKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\]\n]{1,200})\]\]").unwrap());
static FACT_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*[-*+]\s+(?:\[[a-z][a-z0-9_-]{0,32}\]\s*)?").unwrap());
static HIDDEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\u{00ad}\u{180e}\u{200b}-\u{200f}\u{202a}-\u{202e}\u{2060}-\u{2064}\u{2066}-\u{2069}\u{feff}]").unwrap()
});
static PROMPT_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[?&](q|prompt|query|instructions)=").unwrap());
static SECRET_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^\p{L}\p{N}])(?:password|passwd|passcode|api[ _-]?key|secret[ _-]?key|token|contraseña|clave api)\s*(?::|=|\s(?:is|es)\s)\s*\S").unwrap()
});
static SECRET_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|\s)(?:sk-|ghp_|gho_|github_pat_|xoxb-|xoxp-|akia|-----begin)").unwrap()
});
static SSN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap());
static CARD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d{13,19}\b").unwrap());
static OPAQUE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9+/=_-]{32,}").unwrap());
static OPAQUE_SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[/_-]+").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Refusal {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedFact {
    pub category: Option<String>,
    pub text: String,
    pub source: Option<String>,
    pub added: Option<String>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub links: Vec<String>,
}

pub(super) fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub(super) fn normalize_fact(value: &str) -> String {
    let stripped = BULLET.replace(value, "");
    WHITESPACE.replace_all(&stripped, " ").trim().to_lowercase()
}

pub(super) fn hash_fact(value: &str) -> String {
    sha256_hex(&normalize_fact(value))
}

#[cfg(test)]
pub(super) fn is_bullet(line: &str) -> bool {
    BULLET.is_match(line)
}

pub(super) fn category_label(value: &str) -> Option<String> {
    let label = value.trim().to_lowercase();
    if label.is_empty() {
        return None;
    }
    CATEGORY_LABEL.is_match(&label).then_some(label)
}

pub(super) fn is_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let Ok(year) = value[0..4].parse::<i32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    let max = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap(year) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max).contains(&day)
}

pub(super) fn local_date(now: &str) -> String {
    let head = now.get(..10).unwrap_or(now);
    if is_date(head) {
        head.to_string()
    } else {
        String::new()
    }
}

pub(super) fn strip_source(value: &str) -> String {
    SOURCE_TAIL.replace(value, "").trim().to_string()
}

pub(super) fn check_fact(value: &str) -> Result<(), Refusal> {
    let fact = strip_source(value);
    if fact.is_empty() {
        return refuse("empty", "A memory fact cannot be empty.");
    }
    if fact.chars().count() > FACT_CHARS {
        return refuse(
            "too_long",
            format!("Keep one fact to {FACT_CHARS} characters or fewer."),
        );
    }
    if fact.contains('\n') || fact.contains('\r') {
        return refuse("newline", "One fact per line; remove the line break.");
    }
    if HIDDEN.is_match(&fact) {
        return refuse(
            "hidden_characters",
            "The fact contains hidden or bidirectional characters.",
        );
    }
    if fact.contains(MANAGED_START)
        || fact.contains(MANAGED_END)
        || fact.contains("<memory>")
        || fact.contains("</memory>")
    {
        return refuse(
            "managed_marker",
            "The fact contains a marker reserved for generated instructions.",
        );
    }
    if PROMPT_LINK.is_match(&fact) {
        return refuse(
            "prompt_link",
            "The fact contains a link that carries a prompt.",
        );
    }
    let lower = fact.to_lowercase();
    if INSTRUCTIONS.iter().any(|phrase| lower.contains(phrase)) {
        return refuse(
            "instruction",
            "Memory holds facts about the user, not instructions to an assistant.",
        );
    }
    if let Some(refusal) = secret_refusal(&fact, "memory") {
        return Err(refusal);
    }
    Ok(())
}

pub(super) fn check_note(content: &str) -> Result<(), Refusal> {
    if content.chars().count() > NOTE_CHARS {
        return refuse(
            "too_long",
            format!("Keep a note to {NOTE_CHARS} characters or fewer."),
        );
    }
    if HIDDEN.is_match(content) {
        return refuse(
            "hidden_characters",
            "The note contains hidden or bidirectional characters.",
        );
    }
    if content.contains(MANAGED_START)
        || content.contains(MANAGED_END)
        || content.contains("<memory>")
        || content.contains("</memory>")
    {
        return refuse(
            "managed_marker",
            "The note contains a marker reserved for generated instructions.",
        );
    }
    if let Some(refusal) = secret_refusal(content, "a note") {
        return Err(refusal);
    }
    Ok(())
}

fn secret_refusal(text: &str, place: &str) -> Option<Refusal> {
    let card = CARD.is_match(text) && CARD.find_iter(text).any(|run| luhn(run.as_str()));
    let reason = if SECRET_LABEL.is_match(text)
        || SECRET_PREFIX.is_match(text)
        || SSN.is_match(text)
        || card
    {
        "secret"
    } else if opaque_run(text) {
        "opaque"
    } else {
        return None;
    };
    let message = if reason == "opaque" {
        format!("That has a long unbroken string that looks like a key or token, which belongs in the Vault, never in {place}. If it is a commit hash or an id, shorten it (12 characters is enough).")
    } else {
        format!("Passwords, keys, card numbers and identity numbers belong in the Vault, never in {place}.")
    };
    Some(Refusal {
        code: "secret",
        message,
    })
}

fn opaque_run(text: &str) -> bool {
    OPAQUE.find_iter(text).any(|run| {
        let run = run.as_str();
        run.chars().any(|ch| ch.is_ascii_digit())
            && OPAQUE_SPLIT.split(run).any(|piece| piece.len() > 16)
    })
}

fn luhn(value: &str) -> bool {
    let mut sum = 0_u32;
    let mut double = false;
    for ch in value.chars().rev() {
        let Some(mut digit) = ch.to_digit(10) else {
            continue;
        };
        if double {
            digit *= 2;
            if digit > 9 {
                digit -= 9;
            }
        }
        sum += digit;
        double = !double;
    }
    sum > 0 && sum.is_multiple_of(10)
}

pub(super) fn parse_fact(value: &str) -> ParsedFact {
    let mut body = BULLET.replace(value.trim(), "").to_string();
    let source = SOURCE.captures(&body).and_then(|caps| {
        caps.get(1)
            .map(|matched| matched.as_str().trim().to_string())
    });
    if let Some(found) = SOURCE.find(&body) {
        body.replace_range(found.range(), "");
        body = body.trim().to_string();
    }
    let added = source.as_deref().and_then(|source| {
        ADDED
            .captures(source)
            .and_then(|caps| caps.get(1).map(|matched| matched.as_str().to_string()))
    });
    let category = CATEGORY
        .captures(&body)
        .and_then(|caps| caps.get(1).map(|matched| matched.as_str().to_lowercase()));
    if let Some(found) = CATEGORY.find(&body) {
        body.replace_range(found.range(), "");
        body = body.trim().to_string();
    }
    let (valid_from, valid_to) = if let Some(caps) = TEMPORAL.captures(&body) {
        let from = caps
            .get(2)
            .or_else(|| caps.get(3))
            .map(|matched| matched.as_str().to_string());
        let to = caps.get(4).map(|matched| matched.as_str().to_string());
        if let Some(found) = TEMPORAL.find(&body) {
            body.replace_range(found.range(), "");
            body = body.trim().to_string();
        }
        (from, to)
    } else {
        (None, None)
    };
    let links = LINKS
        .captures_iter(&body)
        .filter_map(|caps| {
            caps.get(1)
                .map(|matched| matched.as_str().trim().to_string())
        })
        .collect();
    ParsedFact {
        category,
        text: body.trim().to_string(),
        source,
        added,
        valid_from,
        valid_to,
        links,
    }
}

pub(super) fn format_fact(
    fact: &str,
    category: Option<&str>,
    source: Option<&str>,
    valid_from: Option<&str>,
) -> String {
    let without_source = SOURCE_TAIL.replace(fact, "");
    let text = BULLET.replace(&without_source, "");
    let text = text.trim();
    let head = category
        .map(|category| format!("[{category}] "))
        .unwrap_or_default();
    let temporal = valid_from
        .map(|date| format!("@effective:{date} "))
        .unwrap_or_default();
    let origin = source
        .map(|source| format!(" [source: {}]", source.replace(']', "")))
        .unwrap_or_default();
    format!("- {head}{temporal}{text}{origin}")
}

/// Close a fact's range without inventing a start date.
pub(super) fn close_fact_range(line: &str, to: &str) -> String {
    let (body, source) = match SOURCE_TAIL.find(line) {
        Some(found) => (&line[..found.start()], found.as_str().to_string()),
        None => (line, String::new()),
    };
    let body = if let Some(caps) = TEMPORAL.captures(body) {
        let kind = caps
            .get(1)
            .map(|matched| matched.as_str().to_string())
            .unwrap_or_else(|| "valid".to_string());
        let from = caps
            .get(2)
            .or_else(|| caps.get(3))
            .map(|matched| matched.as_str().to_string())
            .unwrap_or_default();
        TEMPORAL
            .replace(body, format!("@{kind}[{from},{to})"))
            .into_owned()
    } else {
        let at = FACT_PREFIX
            .find(body)
            .map(|matched| matched.end())
            .unwrap_or(0);
        format!("{}@valid[,{to}) {}", &body[..at], &body[at..])
    };
    format!("{body}{source}")
}

fn leap(year: i32) -> bool {
    let divisible = |by: i32| year.rem_euclid(by) == 0;
    divisible(4) && (!divisible(100) || divisible(400))
}

fn refuse(code: &'static str, message: impl Into<String>) -> Result<(), Refusal> {
    Err(Refusal {
        code,
        message: message.into(),
    })
}
