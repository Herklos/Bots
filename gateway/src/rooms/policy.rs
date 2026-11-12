use super::*;

/// Bot-to-Bot wake-ups one message from the person can set off. One handoff is enough: a Bot
/// can ask another once, and that answer lands in the room without starting a ping-pong.
pub(crate) const MAX_HOPS: u64 = 1;

/// Who answers a group message that does not name anyone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerPolicy {
    Last,
    Everyone,
    Chief,
}

impl AnswerPolicy {
    pub(crate) fn parse(raw: Option<&str>) -> Option<Self> {
        match raw {
            Some("last") => Some(Self::Last),
            Some("everyone") => Some(Self::Everyone),
            Some("chief") => Some(Self::Chief),
            Some(_) | None => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Last => "last",
            Self::Everyone => "everyone",
            Self::Chief => "chief",
        }
    }
}

/// Stored room policy, or the settings default when the field is missing or `"default"`.
/// An unrecognised stored value behaves as `last`.
pub(crate) fn resolve_policy(room: &Value, default: AnswerPolicy) -> AnswerPolicy {
    match room["answerPolicy"].as_str() {
        None | Some("default") => default,
        Some(stored) => AnswerPolicy::parse(Some(stored)).unwrap_or(AnswerPolicy::Last),
    }
}

/// The chief slug when it is still a member, otherwise the first member.
pub(crate) fn effective_chief(room: &Value, members: &[Member]) -> Option<String> {
    let stored = room["chief"].as_str().unwrap_or("");
    if !stored.is_empty() && members.iter().any(|member| member.slug == stored) {
        return Some(stored.to_string());
    }
    members.first().map(|member| member.slug.clone())
}

/// Bot-to-Bot wake-ups one message from the person can still set off.
pub(crate) fn max_hops(policy: AnswerPolicy, members_len: usize) -> u64 {
    match policy {
        AnswerPolicy::Last | AnswerPolicy::Everyone => MAX_HOPS,
        AnswerPolicy::Chief => u64::try_from(members_len.saturating_sub(1)).unwrap_or(0),
    }
}
