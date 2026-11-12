//! Provider limit bars, reduced to the fields the app may show.
//!
//! `usedPercent` is clamped to 0..=100. A window without a finite percent is dropped.
//! Bars sort session, weekly, monthly, other. `unsupported` clears the bars and is not
//! an error. `probeFailed` keeps the last good bars and says they could not be refreshed.
//! A fingerprint, reset credit, runtime path, or non-https usage link never leaves here.
use serde_json::{json, Value};

const REFRESH_NOTE: &str = "Could not refresh";

/// Session, then weekly, then monthly, then other. Declaration order is the sort order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum WindowKind {
    Session,
    Weekly,
    Monthly,
    Other,
}

impl WindowKind {
    fn parse(value: &Value) -> Option<Self> {
        match value.as_str()? {
            "session" => Some(Self::Session),
            "weekly" => Some(Self::Weekly),
            "monthly" => Some(Self::Monthly),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AccountState {
    Ok,
    Unavailable,
    SignedOut,
    NotInstalled,
}

impl AccountState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Unavailable => "unavailable",
            Self::SignedOut => "signed-out",
            Self::NotInstalled => "not-installed",
        }
    }
}

/// One provider instance, or `None` when it has no instance id.
pub(super) fn limits_of(provider: &Value) -> Option<Value> {
    let instance = provider["instanceId"]
        .as_str()
        .filter(|id| !id.is_empty())?;
    let label = provider["displayName"]
        .as_str()
        .filter(|name| !name.is_empty())
        .unwrap_or(instance);
    let usage = provider
        .get("usageLimits")
        .filter(|value| value.is_object());
    let reason = usage
        .and_then(|usage| usage["unavailable"]["reason"].as_str())
        .unwrap_or("");
    // A failed probe keeps the last good bars. Any other unavailable reason clears them.
    let windows = if reason.is_empty() || reason == "probeFailed" {
        usage.map(windows_of).unwrap_or_default()
    } else {
        Vec::new()
    };
    let note = if reason == "probeFailed" {
        REFRESH_NOTE
    } else {
        ""
    };
    let checked_at = usage
        .and_then(|usage| usage["checkedAt"].as_str())
        .filter(|at| !at.is_empty())
        .map(|at| json!(at))
        .unwrap_or(Value::Null);
    let mut out = json!({
        "instanceId": instance,
        "label": label,
        "state": account_state(provider).as_str(),
        "windows": windows,
        "checkedAt": checked_at,
        "note": note,
    });
    if let Some(link) = usage.and_then(external_usage) {
        out["externalUsage"] = link;
    }
    Some(out)
}

fn account_state(provider: &Value) -> AccountState {
    if provider["installed"] != true {
        return AccountState::NotInstalled;
    }
    if provider["auth"]["status"] != "authenticated" {
        return AccountState::SignedOut;
    }
    if provider["enabled"] != true || provider["status"] != "ready" {
        return AccountState::Unavailable;
    }
    if provider["unavailableReason"]
        .as_str()
        .is_some_and(|reason| !reason.is_empty())
    {
        return AccountState::Unavailable;
    }
    if provider["availability"].as_str() == Some("unavailable") {
        return AccountState::Unavailable;
    }
    AccountState::Ok
}

fn windows_of(usage: &Value) -> Vec<Value> {
    let mut windows: Vec<(WindowKind, Value)> = usage["windows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(window_of)
        .collect();
    windows.sort_by_key(|(kind, _)| *kind);
    windows.into_iter().map(|(_, window)| window).collect()
}

fn window_of(window: &Value) -> Option<(WindowKind, Value)> {
    let id = window["id"].as_str().filter(|id| !id.is_empty())?;
    let kind = WindowKind::parse(&window["kind"])?;
    let used = used_percent(&window["usedPercent"])?;
    let label = window["label"].as_str().unwrap_or("");
    let mut out = json!({
        "id": id,
        "kind": kind.as_str(),
        "label": label,
        "usedPercent": used,
    });
    if let Some(resets) = window["resetsAt"].as_str().filter(|at| !at.is_empty()) {
        out["resetsAt"] = json!(resets);
    }
    Some((kind, out))
}

/// `None` when the percent is missing or not a finite number. Values outside 0..=100 clamp.
fn used_percent(value: &Value) -> Option<Value> {
    let number = value.as_f64()?;
    if !number.is_finite() {
        return None;
    }
    if (0.0..=100.0).contains(&number) {
        return Some(value.clone());
    }
    if number < 0.0 {
        Some(json!(0))
    } else {
        Some(json!(100))
    }
}

fn external_usage(usage: &Value) -> Option<Value> {
    let block = usage.get("externalUsage")?;
    let url = https_url(block["url"].as_str()?)?;
    let label = block["label"].as_str().unwrap_or("");
    Some(json!({ "label": label, "url": url }))
}

fn https_url(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let url = url::Url::parse(trimmed).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?;
    if host.is_empty() || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    Some(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!(
            "../../../../tests/fixtures/t3/usage-limits.json"
        ))
        .expect("usage limits fixture")
    }

    fn provider(case: &str) -> Value {
        fixture()["providers"]
            .as_array()
            .and_then(|providers| providers.iter().find(|item| item["case"] == case).cloned())
            .unwrap_or_else(|| panic!("fixture case {case}"))
    }

    fn normalised(case: &str) -> Value {
        limits_of(&provider(case)).unwrap_or_else(|| panic!("provider {case} should normalise"))
    }

    #[test]
    fn missing_usage_limits_has_no_bars_and_is_not_an_error() {
        let out = normalised("missing");
        assert_eq!(out["instanceId"], "claude");
        assert_eq!(out["label"], "Claude");
        assert_eq!(out["state"], "ok");
        assert_eq!(out["windows"], json!([]));
        assert_eq!(out["note"], "");
        assert!(out["checkedAt"].is_null());
        assert!(!out.to_string().contains("person@example.com"));
    }

    #[test]
    fn unsupported_yields_no_bars_and_no_note() {
        let out = normalised("unsupported");
        assert_eq!(out["state"], "ok");
        assert_eq!(out["windows"], json!([]));
        assert_eq!(out["note"], "");
        assert_eq!(out["checkedAt"], "2026-11-12T14:00:00Z");
        assert!(!out.to_string().contains("hidden-probe-message"));
        assert!(!out.to_string().contains("5 hour"));
    }

    #[test]
    fn probe_failed_keeps_the_last_good_bars() {
        let out = normalised("probe-failed");
        assert_eq!(out["state"], "ok");
        assert_eq!(out["note"], "Could not refresh");
        let windows = out["windows"].as_array().expect("windows");
        let ids: Vec<&str> = windows.iter().filter_map(|w| w["id"].as_str()).collect();
        assert_eq!(ids, ["5h", "week", "month", "extra"]);
        assert_eq!(windows[0]["kind"], "session");
        assert_eq!(windows[0]["label"], "5 hour");
        assert_eq!(windows[0]["usedPercent"], json!(62));
        assert_eq!(windows[0]["resetsAt"], "2026-11-12T17:00:00Z");
        assert_eq!(windows[1]["kind"], "weekly");
        assert_eq!(windows[1]["resetsAt"], "2026-11-16T00:00:00Z");
        assert!(windows[1].get("windowDurationMins").is_none());
        assert!(!out.to_string().contains("stale"));
        assert!(!out.to_string().contains("10080"));
    }

    #[test]
    fn out_of_range_percent_is_clamped() {
        let windows = normalised("out-of-range")["windows"]
            .as_array()
            .expect("windows")
            .clone();
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0]["id"], "high");
        assert_eq!(windows[0]["usedPercent"], json!(100));
        assert_eq!(windows[1]["id"], "low");
        assert_eq!(windows[1]["usedPercent"], json!(0));
        assert_eq!(windows[2]["id"], "month");
        assert_eq!(windows[2]["usedPercent"], json!(62));
        let text = windows
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!text.contains("blank"));
        assert!(!text.contains("text"));
        assert!(!text.contains("bad"));
        assert!(!text.contains("daily"));
    }

    #[test]
    fn secret_fields_are_stripped() {
        let dumped = fixture()["providers"]
            .as_array()
            .expect("providers")
            .iter()
            .filter_map(limits_of)
            .map(|provider| provider.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        for secret in [
            "credentialFingerprint",
            "resetCredits",
            "runtimePaths",
            "fp-secret-9f3a",
            "credit-secret-11",
            "/secret/runtime/path",
            "person@example.com",
            "ws-secret",
            "secret-token",
            "hidden-probe-message",
            "windowDurationMins",
            "10080",
            "http://example.com/usage",
            "javascript:alert(1)",
            "file:///tmp/usage",
        ] {
            assert!(!dumped.contains(secret), "{secret} left the normaliser");
        }
        let secrets = normalised("secrets");
        assert!(secrets.get("externalUsage").is_none());
        assert_eq!(secrets["windows"][0]["id"], "5h");
        assert_eq!(secrets["windows"][0]["usedPercent"], json!(20));
    }

    #[test]
    fn non_https_external_usage_is_dropped() {
        for case in ["http-link", "script-link", "file-link"] {
            assert!(
                normalised(case).get("externalUsage").is_none(),
                "{case} kept a usage link"
            );
        }
        assert_eq!(
            normalised("https-link")["externalUsage"],
            json!({ "label": "Usage", "url": "https://example.com/usage" })
        );
    }

    #[test]
    fn account_state_follows_install_auth_and_readiness() {
        assert_eq!(normalised("signed-out")["state"], "signed-out");
        assert_eq!(normalised("not-installed")["state"], "not-installed");
        assert_eq!(normalised("turned-off")["state"], "unavailable");
        assert_eq!(normalised("update-advised")["state"], "unavailable");
        assert_eq!(normalised("offline")["state"], "unavailable");
    }

    #[test]
    fn an_unavailable_account_clears_bars_without_a_note() {
        let out = normalised("account-down");
        assert_eq!(out["state"], "ok");
        assert_eq!(out["windows"], json!([]));
        assert_eq!(out["note"], "");
        assert!(!out.to_string().contains("hidden-probe-message"));
    }

    #[test]
    fn a_provider_without_an_instance_is_left_out() {
        assert!(limits_of(&provider("blank")).is_none());
    }
}
