//! Approval modes, least access first: `approval-required`, then `auto`, then `full-access`.
//!
//! `clamp_mode` returns the lower of a requested mode and a cap.

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
#[allow(
    dead_code,
    reason = "Bot, routine, and subagent writes call this when the cap is enforced"
)]
pub(crate) fn clamp_mode(requested: RuntimeMode, max: RuntimeMode) -> RuntimeMode {
    requested.min(max)
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
    }
}
