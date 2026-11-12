//! Approval modes, least access first: `approval-required`, then `auto`, then `full-access`.
//!
//! `clamp_mode` returns the lower of a requested mode and a cap.

use serde_json::{json, Value};

/// How much a Bot may do without asking.
///
/// The order is the access order. A higher variant may do everything a lower one may.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeMode {
    ApprovalRequired,
    Auto,
    FullAccess,
}

impl RuntimeMode {
    pub(crate) fn parse(raw: Option<&str>) -> Option<Self> {
        match raw {
            Some("approval-required") => Some(Self::ApprovalRequired),
            Some("auto") => Some(Self::Auto),
            Some("full-access") => Some(Self::FullAccess),
            Some(_) | None => None,
        }
    }

    /// Stored `maxRuntimeMode`. Missing or unknown is full access, which is no cap.
    pub(crate) fn cap(raw: Option<&str>) -> Self {
        Self::parse(raw).unwrap_or(Self::FullAccess)
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ApprovalRequired => "approval-required",
            Self::Auto => "auto",
            Self::FullAccess => "full-access",
        }
    }

    /// Higher means more access. The cap uses this order, not declaration order.
    fn access(self) -> u8 {
        match self {
            Self::ApprovalRequired => 0,
            Self::Auto => 1,
            Self::FullAccess => 2,
        }
    }
}

impl PartialOrd for RuntimeMode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RuntimeMode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.access().cmp(&other.access())
    }
}

/// `requested` when it is within `max`, otherwise `max`.
pub(crate) fn clamp_mode(requested: RuntimeMode, max: RuntimeMode) -> RuntimeMode {
    requested.min(max)
}

/// A known mode, lowered to `cap`. An unknown mode stays for the caller to reject.
pub(crate) fn clamp_label(mode: &str, cap: RuntimeMode) -> Option<&'static str> {
    RuntimeMode::parse(Some(mode)).map(|mode| clamp_mode(mode, cap).as_str())
}

/// The mode to set on an existing thread before the next turn.
/// `None` when the stored mode is missing, unknown, or already within the cap.
pub(crate) fn turn_mode(stored: Option<&str>, cap: RuntimeMode) -> Option<&'static str> {
    let mode = RuntimeMode::parse(stored)?;
    (mode > cap).then_some(clamp_mode(mode, cap).as_str())
}

/// A stored mode the list can label. Missing and unknown modes are within the cap.
pub(crate) fn above_limit(stored: Option<&str>, cap: RuntimeMode) -> bool {
    turn_mode(stored, cap).is_some()
}

/// `defaultRuntimeMode` is stored at or below `maxRuntimeMode`. A missing default stays missing.
pub(crate) fn apply_default_cap(settings: &mut Value) {
    let cap = RuntimeMode::cap(settings.get("maxRuntimeMode").and_then(Value::as_str));
    let Some(raw) = settings.get("defaultRuntimeMode").and_then(Value::as_str) else {
        return;
    };
    let Some(mode) = RuntimeMode::parse(Some(raw)) else {
        return;
    };
    settings["defaultRuntimeMode"] = json!(clamp_mode(mode, cap).as_str());
}

/// Response-only flag. This does not write the Bot.
pub(crate) fn mark_above_limit(bots: &mut [Value], cap: RuntimeMode) {
    for bot in bots {
        let stored = bot.get("runtimeMode").and_then(Value::as_str);
        bot["aboveLimit"] = json!(above_limit(stored, cap));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_mode_follows_approval_then_auto_then_full_access() {
        assert!(RuntimeMode::ApprovalRequired < RuntimeMode::Auto);
        assert!(RuntimeMode::Auto < RuntimeMode::FullAccess);
        assert!(RuntimeMode::ApprovalRequired < RuntimeMode::FullAccess);

        assert_eq!(
            clamp_mode(RuntimeMode::FullAccess, RuntimeMode::Auto),
            RuntimeMode::Auto
        );
        assert_eq!(
            clamp_mode(RuntimeMode::Auto, RuntimeMode::ApprovalRequired),
            RuntimeMode::ApprovalRequired
        );
        assert_eq!(
            clamp_mode(RuntimeMode::ApprovalRequired, RuntimeMode::FullAccess),
            RuntimeMode::ApprovalRequired
        );
        assert_eq!(
            clamp_mode(RuntimeMode::Auto, RuntimeMode::FullAccess),
            RuntimeMode::Auto
        );
        assert_eq!(
            clamp_mode(RuntimeMode::Auto, RuntimeMode::Auto),
            RuntimeMode::Auto
        );
        assert_eq!(
            clamp_mode(RuntimeMode::FullAccess, RuntimeMode::FullAccess),
            RuntimeMode::FullAccess
        );
    }

    #[test]
    fn mode_labels_round_trip_and_unknown_is_rejected() {
        for mode in [
            RuntimeMode::ApprovalRequired,
            RuntimeMode::Auto,
            RuntimeMode::FullAccess,
        ] {
            assert_eq!(RuntimeMode::parse(Some(mode.as_str())), Some(mode));
        }
        assert_eq!(RuntimeMode::parse(Some("ask")), None);
        assert_eq!(RuntimeMode::parse(None), None);
        assert_eq!(RuntimeMode::cap(None), RuntimeMode::FullAccess);
        assert_eq!(RuntimeMode::cap(Some("ask")), RuntimeMode::FullAccess);
        assert_eq!(RuntimeMode::cap(Some("auto")), RuntimeMode::Auto);
    }

    #[test]
    fn a_turn_uses_the_cap_only_when_the_stored_mode_is_above_it() {
        assert_eq!(
            turn_mode(Some("full-access"), RuntimeMode::Auto),
            Some("auto")
        );
        assert_eq!(turn_mode(Some("auto"), RuntimeMode::Auto), None);
        assert_eq!(
            turn_mode(Some("approval-required"), RuntimeMode::Auto),
            None
        );
        assert_eq!(turn_mode(None, RuntimeMode::Auto), None);
        assert_eq!(turn_mode(Some("ask"), RuntimeMode::Auto), None);
        assert!(above_limit(
            Some("full-access"),
            RuntimeMode::ApprovalRequired
        ));
        assert!(!above_limit(Some("auto"), RuntimeMode::FullAccess));
    }

    #[test]
    fn the_default_mode_is_lowered_to_the_cap_and_a_list_row_is_only_marked() {
        let mut settings = json!({
            "defaultRuntimeMode": "full-access",
            "maxRuntimeMode": "auto",
        });
        apply_default_cap(&mut settings);
        assert_eq!(settings["defaultRuntimeMode"], "auto");
        let mut missing = json!({ "maxRuntimeMode": "auto" });
        apply_default_cap(&mut missing);
        assert!(missing.get("defaultRuntimeMode").is_none());
        let mut unknown = json!({ "defaultRuntimeMode": "ask", "maxRuntimeMode": "auto" });
        apply_default_cap(&mut unknown);
        assert_eq!(unknown["defaultRuntimeMode"], "ask");

        let mut bots = vec![
            json!({ "slug": "ada", "runtimeMode": "full-access" }),
            json!({ "slug": "bea", "runtimeMode": "auto" }),
            json!({ "slug": "cy" }),
        ];
        mark_above_limit(&mut bots, RuntimeMode::Auto);
        assert_eq!(bots[0]["aboveLimit"], true);
        assert_eq!(bots[1]["aboveLimit"], false);
        assert_eq!(bots[2]["aboveLimit"], false);
        assert_eq!(bots[0]["runtimeMode"], "full-access");
    }
}
