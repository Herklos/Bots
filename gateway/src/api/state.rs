use super::*;

#[derive(Debug)]
pub struct AppState {
    pub cfg: crate::platform::config::Config,
    pub(crate) settings_store: Arc<dyn crate::storage::stores::SettingsStore>,
    pub auth: crate::network::auth::Auth,
    pub harness: std::sync::Arc<dyn crate::harness::Harness>,
    pub bots: bots::Bots,
    pub vault: vault::Vault,
    pub projects: projects::Projects,
    // T4 remote access: Cloudflare Access, the Tailscale listener, the tunnel, the password in access.json.
    pub access: Option<crate::network::access::Access>,
    pub exposure: crate::network::exposure::Exposure,
    /// Per-address and global buckets for `POST /hooks/`.
    pub hook_limit: Arc<crate::network::ratelimit::Limiter>,
    pub tailnet: Arc<crate::network::exposure::Tailnet>,
    pub remote: Arc<crate::network::remote_access::RemoteAccess>,
    pub threads: crate::chat::thread_meta::ThreadMeta,
    pub roster: crate::bots::roster::Roster,
    pub providers: crate::harness::providers::Providers,
    pub rooms: crate::chat::rooms::Rooms,
    /// Last known working flag per group thread, and the short grace after a dispatch.
    /// Not written to `groups.json`.
    pub(crate) room_working: Mutex<crate::chat::rooms::WorkingBook>,
    pub routine_history: crate::chat::routines::RoutineHistory,
    pub system: crate::platform::release::System,
    pub icons: crate::bots::icons::Icons,
    pub usage: crate::bots::usage::Usage,
    /// Shared with [`crate::bots::library::Library`] and [`crate::bots::memory::MemoryStore`].
    pub workspace: Arc<dyn crate::storage::workspace::Workspace>,
    pub memory: crate::bots::memory::MemoryStore,
    pub library: crate::bots::library::Library,
    /// Checked library images named in Bot replies. A hit does no file IO.
    pub(crate) images: crate::chat::images::Resolver,
    /// Repository facts for the code gate. One minute per Bot.
    pub(crate) code_signals: crate::chat::code::SignalCache,
    /// Status and pull-request reads for an open Code tab.
    pub(crate) code_reads: crate::chat::code::ReadCache,
    pub mcp_token: String,
    pub provision_now: tokio::sync::Notify,
    /// The Anytype sync engine handle, set once after `AppState` is built.
    /// `None` when the saved backend is the file system.
    pub(crate) sync: crate::storage::sync::SyncSlot,
    /// One held chat or group read at a time. Shutdown wakes it.
    pub(crate) holds: chat::HeldReads,
}

impl AppState {
    #[cfg(test)]
    pub(crate) fn settings_path(&self) -> std::path::PathBuf {
        self.cfg.app_dir().join("settings.json")
    }

    pub fn settings_defaults() -> Value {
        json!({
            "userName": "", "theme": "dark", "palette": "amoled", "accent": "bot", "timezone": "", "autoReview": true,
            "rules": { "alwaysAsk": [], "allowWithoutAsking": [] }, "sections": [], "defaultModel": null, "avatarDrawingModel": null,
            "beta": { "projects": false, "code": false },
            "notifications": { "enabled": false, "sound": true, "preview": "full", "when": "any" },
            "webhooks": { "publicBase": "" },
            "groupAnswerPolicy": "last",
            "activityLevel": "quiet",
            "finalOnly": false,
            "managementMcp": { "enabled": false },
            "maxRuntimeMode": "full-access",
        })
    }

    /// Known notification fields from the file. Missing or invalid values stay at the defaults.
    /// Unknown keys are dropped, the same way `beta` keeps only `projects` and `code`.
    pub(crate) fn notifications_view(saved: &Value) -> Value {
        let mut out = Self::settings_defaults()["notifications"].clone();
        let Some(given) = saved.get("notifications").and_then(|v| v.as_object()) else {
            return out;
        };
        for (k, v) in given {
            match k.as_str() {
                "enabled" | "sound" => {
                    if let Some(on) = v.as_bool() {
                        out[k] = json!(on);
                    }
                }
                "preview" => {
                    if matches!(v.as_str(), Some("full" | "hidden")) {
                        out[k] = v.clone();
                    }
                }
                "when" => {
                    if matches!(v.as_str(), Some("any" | "background")) {
                        out[k] = v.clone();
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Known webhook fields. An empty `publicBase` means "not set". Anything else must be an https origin.
    pub(crate) fn webhooks_view(saved: &Value) -> Value {
        let base = saved
            .get("webhooks")
            .and_then(|value| value.get("publicBase"))
            .and_then(Value::as_str)
            .filter(|value| crate::chat::routines::webhook::public_base_ok(value))
            .map(crate::chat::routines::webhook::canonical_public_base)
            .unwrap_or_default();
        json!({ "publicBase": base })
    }

    /// `settings.json` with hbots defaults filled in.
    pub async fn settings(&self) -> Value {
        self.try_settings().await.unwrap_or_else(|error| {
            tracing::warn!("reading settings: {}", error.message);
            let mut fallback = Self::settings_defaults();
            // A future remote store may be temporarily unavailable. Do not
            // grant the full-access default when its saved cap is unknown.
            fallback["maxRuntimeMode"] = json!("approval-required");
            fallback["defaultRuntimeMode"] = json!("approval-required");
            fallback
        })
    }

    pub(crate) async fn try_settings(&self) -> ApiResult<Value> {
        let saved = self
            .settings_store
            .read()
            .await?
            .unwrap_or_else(|| json!({}));
        let mut out = storage::merge(&Self::settings_defaults(), &saved);
        out["beta"] = json!({
            "projects": saved["beta"]["projects"] == true,
            "code": saved["beta"]["code"] == true,
        });
        out["managementMcp"] = json!({"enabled": saved["managementMcp"]["enabled"] == true});
        out["notifications"] = Self::notifications_view(&saved);
        out["webhooks"] = Self::webhooks_view(&saved);
        out["groupAnswerPolicy"] = json!(group_answer_policy(&out).as_str());
        out["activityLevel"] = json!(activity_level(&out));
        out["finalOnly"] = json!(final_only(&out));
        // Unknown or missing is full access, so the cap is off.
        out["maxRuntimeMode"] = json!(max_runtime_mode(&out).as_str());
        crate::bots::runtime_mode::apply_default_cap(&mut out);
        // Missing stays []. Invalid becomes [] so one bad value cannot blank Settings.
        out["sections"] = super::settings::sections_view(&saved);
        Ok(out)
    }
}

/// Stored tool-call display. Missing or unknown stays quiet.
pub(crate) fn activity_level(settings: &Value) -> &'static str {
    match settings["activityLevel"].as_str() {
        Some("problems") => "problems",
        Some("summary") => "summary",
        Some("detail") => "detail",
        _ => "quiet",
    }
}

pub(crate) fn final_only(settings: &Value) -> bool {
    settings["finalOnly"] == true
}

pub(crate) fn group_answer_policy(settings: &Value) -> crate::chat::rooms::AnswerPolicy {
    crate::chat::rooms::AnswerPolicy::parse(settings["groupAnswerPolicy"].as_str())
        .unwrap_or(crate::chat::rooms::AnswerPolicy::Last)
}

/// Stored `maxRuntimeMode`. Missing or unknown is full access, which is no cap.
pub(crate) fn max_runtime_mode(settings: &Value) -> crate::bots::runtime_mode::RuntimeMode {
    crate::bots::runtime_mode::RuntimeMode::cap(settings["maxRuntimeMode"].as_str())
}

pub(crate) type S = State<Arc<AppState>>;

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
