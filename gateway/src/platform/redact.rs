//! Redaction for diagnostics.
//!
//! Tokens (`Bearer`, `T3_TOKEN`), the password, session ids, the contents of vault
//! paths, emails, and home-directory names are removed before a report is shown.

use regex::Regex;

/// Values the report must not repeat. Short secrets are still removed when they
/// appear as `T3_TOKEN=...` or `HBOTS_PASSWORD=...`. A bare value is removed only
/// when it is at least 6 characters, so a short password does not eat ordinary words.
pub(crate) struct Secrets {
    pub token: Option<String>,
    pub password: Option<String>,
    /// Stored password hash and salt, and the session secret, when those files exist.
    pub extras: Vec<String>,
    /// The person's home directory. It is replaced by `~`, and so are other `/home` and `/Users` names.
    pub home: Option<String>,
}

pub(crate) fn redact(text: &str, secrets: &Secrets) -> String {
    let mut out = apply_patterns(text);
    out = blank_known(&out, secrets);
    out = redact_emails(&out);
    out = map_lines(&out, redact_vault_line);
    redact_homes(&out, secrets.home.as_deref())
}

fn apply_patterns(text: &str) -> String {
    const PATTERNS: &[(&str, &str)] = &[
        (r"(?i)\bBearer\s+\S+", "Bearer [redacted]"),
        (
            r#"(?i)\b(HBOTS_PASSWORD|BOT_PASSWORD|T3_TOKEN)\s*[:=]\s*"?[^"\s]+"?"#,
            "$1=[redacted]",
        ),
        (r"(?i)\bbot_session=[^\s;]+", "bot_session=[redacted]"),
        (r"v1\.\d{10,}\.[0-9a-fA-F]{16,}", "[redacted]"),
        (
            r#"(?i)"session_id"\s*:\s*"[^"]*""#,
            r#""session_id":"[redacted]""#,
        ),
        (
            r#"(?i)"sessionId"\s*:\s*"[^"]*""#,
            r#""sessionId":"[redacted]""#,
        ),
        (r"(?i)\bsession_id=[^\s,;}]+", "session_id=[redacted]"),
        (r"(?i)\bsessionId=[^\s,;}]+", "sessionId=[redacted]"),
    ];
    let mut out = text.to_string();
    for (pattern, replacement) in PATTERNS {
        let re = Regex::new(pattern).expect("redaction pattern");
        out = re.replace_all(&out, *replacement).into_owned();
    }
    out
}

fn blank_known(text: &str, secrets: &Secrets) -> String {
    let mut needles: Vec<&str> = secrets
        .token
        .iter()
        .chain(secrets.password.iter())
        .chain(secrets.extras.iter())
        .map(String::as_str)
        .filter(|secret| secret.len() >= 6 && !secret.contains(char::is_whitespace))
        .collect();
    needles.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
    needles.dedup();
    let mut out = text.to_string();
    for secret in needles {
        out = replace_bounded(&out, secret);
    }
    out
}

fn replace_bounded(text: &str, secret: &str) -> String {
    let Ok(re) = Regex::new(&format!(
        r"(^|[^A-Za-z0-9]){}([^A-Za-z0-9]|$)",
        regex::escape(secret)
    )) else {
        return text.to_string();
    };
    re.replace_all(text, "$1[redacted]$2").into_owned()
}

fn redact_emails(text: &str) -> String {
    let re = Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").expect("email pattern");
    re.replace_all(text, "[redacted]").into_owned()
}

/// A vault path stays. Anything after it on the same line is file contents and is dropped.
fn redact_vault_line(line: &str) -> String {
    const MARKERS: [&str; 3] = ["/vault/", "vault.enc", ".vault-key"];
    let Some(start) = MARKERS.iter().filter_map(|marker| line.find(marker)).min() else {
        return line.to_string();
    };
    let tail = &line[start..];
    let path_len = tail
        .find(|ch: char| ch.is_ascii_whitespace() || ch == ':')
        .unwrap_or(tail.len());
    let end = start + path_len;
    if line[end..].trim().is_empty() {
        return line.to_string();
    }
    format!("{} [redacted]", line[..end].trim_end())
}

fn redact_homes(text: &str, home: Option<&str>) -> String {
    let mut out = text.to_string();
    if let Some(home) = home
        .map(|path| path.trim_end_matches('/'))
        .filter(|path| path.len() > 1)
    {
        out = out.replace(home, "~");
    }
    let re = Regex::new(r"(?:/home|/Users)/[^/\s:]+").expect("home pattern");
    re.replace_all(&out, "~").into_owned()
}

fn map_lines(text: &str, mut map: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&map(line));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets() -> Secrets {
        Secrets {
            token: Some("t3tok_live_secret_value".into()),
            password: Some("hunter2-password".into()),
            extras: vec!["session-secret-value".into()],
            home: Some("/home/alice".into()),
        }
    }

    #[test]
    fn leaky_lines_are_redacted() {
        let raw = include_str!("fixtures/leaky-lines.txt");
        let out = redact(raw, &secrets());
        let expected = "\
2026-11-12 INFO gateway listening
Authorization: Bearer [redacted]
env T3_TOKEN=[redacted]
also the bare token [redacted] in a message
HBOTS_PASSWORD=[redacted]
BOT_PASSWORD=[redacted]
login failed for [redacted] from 10.0.0.2
cookie bot_session=[redacted]
body {\"session_id\":\"[redacted]\"} sessionId=[redacted]
read ~/.hbots/vault/note.md [redacted]
copied ~/.hbots/vault.enc [redacted]
key file ~/.hbots/.vault-key [redacted]
mail the report to [redacted] and [redacted]
data ~/.hbots/bots and ~/Library
";
        assert_eq!(out, expected);
        assert!(!out.contains("t3tok_live_secret_value"));
        assert!(!out.contains("hunter2-password"));
        assert!(!out.contains("swordfish-vault-secret"));
        assert!(!out.contains("ada@example.com"));
        assert!(!out.contains("/home/alice"));
        assert!(!out.contains("/Users/ada"));
        assert!(!out.contains("sess-9f3a2c"));
    }

    #[test]
    fn a_vault_path_without_contents_keeps_the_path() {
        let out = redact("opened /home/alice/.hbots/vault.enc\n", &secrets());
        assert_eq!(out, "opened ~/.hbots/vault.enc\n");
    }

    #[test]
    fn a_short_password_assignment_is_still_removed() {
        let out = redact("HBOTS_PASSWORD=ab\n", &secrets());
        assert_eq!(out, "HBOTS_PASSWORD=[redacted]\n");
        assert!(!out.contains("=ab"));
    }

    #[test]
    fn an_extra_secret_is_removed_when_it_stands_alone() {
        let out = redact("secret session-secret-value kept\n", &secrets());
        assert_eq!(out, "secret [redacted] kept\n");
    }
}
