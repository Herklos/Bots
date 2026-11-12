//! Whether one Bot is doing code work. No I/O and no harness calls.
//!
//! `auto` is true when the workspace is a repository, or when the caller
//! supplies activity facts. The gate does not supply those facts: a turn item
//! does not yet carry a path that can be trusted.

use serde_json::Value;

/// Per-Bot choice stored as `code` on the Bot record. Missing means [`CodeMode::Auto`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodeMode {
    Auto,
    On,
    Off,
}

impl CodeMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::On => "on",
            Self::Off => "off",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "auto" => Some(Self::Auto),
            "on" => Some(Self::On),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

/// The part of a repository status the gate reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VcsStatus {
    pub is_repo: bool,
}

/// Path facts from a Bot's turns. Absent means "do not use activity".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActivityFacts {
    pub file_change_in_repo: bool,
    pub git_command_in_repo: bool,
}

/// Result of [`signal`]. `show` is whether the code surface applies to this Bot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Signal {
    pub show: bool,
}

#[derive(Debug)]
pub(crate) struct SignalInput<'a> {
    pub mode: CodeMode,
    pub vcs: Option<&'a VcsStatus>,
    pub activity: Option<&'a ActivityFacts>,
}

pub(crate) fn signal(input: &SignalInput<'_>) -> Signal {
    let show = match input.mode {
        CodeMode::On => true,
        CodeMode::Off => false,
        CodeMode::Auto => repo_hit(input.vcs) || activity_hit(input.activity),
    };
    Signal { show }
}

fn repo_hit(vcs: Option<&VcsStatus>) -> bool {
    vcs.is_some_and(|status| status.is_repo)
}

fn activity_hit(activity: Option<&ActivityFacts>) -> bool {
    activity.is_some_and(|facts| facts.file_change_in_repo || facts.git_command_in_repo)
}

/// `code` on a write. Missing is not handled here.
pub(crate) fn require_mode(value: &Value) -> Result<CodeMode, &'static str> {
    value
        .as_str()
        .and_then(CodeMode::parse)
        .ok_or("code must be auto, on, or off")
}

/// Stored `code`. Missing or unknown is auto, the same default as a new Bot.
pub(crate) fn mode_of(bot: &Value) -> CodeMode {
    bot.get("code")
        .and_then(Value::as_str)
        .and_then(CodeMode::parse)
        .unwrap_or(CodeMode::Auto)
}
