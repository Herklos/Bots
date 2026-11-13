//! The sentence Settings shows when Prepare space fails.

use crate::storage::adapters::anytype::client::AnytypeError;

const GENERIC: &str = "Anytype could not prepare the space.";

pub(crate) fn setup_failure(error: &AnytypeError) -> (u16, String) {
    match error {
        AnytypeError::Unreachable => (
            502,
            "Anytype is not reachable. Open the Anytype app and check the host.".into(),
        ),
        AnytypeError::NotGranted => (403, "Anytype did not grant this space.".into()),
        AnytypeError::RateLimited { .. } => (429, "Anytype is busy. Try again.".into()),
        AnytypeError::Bad { status, message } => {
            let reason = reason(message);
            if reason.is_empty() {
                (502, format!("{GENERIC} ({status})"))
            } else {
                (502, format!("{GENERIC} {status}: {reason}"))
            }
        }
        _ => (502, GENERIC.into()),
    }
}

/// The Anytype message with space ids (long opaque tokens) removed.
fn reason(message: &str) -> String {
    let words: Vec<&str> = message
        .split_whitespace()
        .filter(|word| word.trim_matches(|c: char| !c.is_alphanumeric()).len() < 32)
        .collect();
    let text = words.join(" ");
    text.chars().take(200).collect()
}
