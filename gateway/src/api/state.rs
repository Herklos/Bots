use super::*;

#[derive(Debug)]
pub struct AppState {
    pub cfg: config::Config,
    pub auth: auth::Auth,
    pub t3: t3::T3,
    pub bots: bots::Bots,
    pub vault: vault::Vault,
    pub projects: projects::Projects,
    // T4 remote access: Cloudflare Access, the Tailscale listener, the tunnel, the password in access.json.
    pub access: Option<access::Access>,
    pub exposure: exposure::Exposure,
    pub tailnet: Arc<exposure::Tailnet>,
    pub remote: Arc<remote_access::RemoteAccess>,
    pub threads: thread_meta::ThreadMeta,
    pub roster: roster::Roster,
    pub providers: providers::Providers,
    pub rooms: rooms::Rooms,
    /// Last known working flag per group thread, and the short grace after a dispatch.
    /// Not written to `groups.json`.
    pub(crate) room_working: Mutex<rooms::WorkingBook>,
    pub routine_history: routines::RoutineHistory,
    pub system: release::System,
    pub icons: icons::Icons,
    pub usage: usage::Usage,
    pub library: library::Library,
    pub mcp_token: String,
    pub provision_now: tokio::sync::Notify,
    /// One held chat or group read at a time. Shutdown wakes it.
    pub(crate) holds: chat::HeldReads,
}

impl AppState {
    pub(crate) fn settings_path(&self) -> std::path::PathBuf {
        self.cfg.app_dir().join("settings.json")
    }

    pub fn settings_defaults() -> Value {
        json!({
            "userName": "", "theme": "dark", "palette": "amoled", "accent": "bot", "timezone": "", "autoReview": true,
            "rules": { "alwaysAsk": [], "allowWithoutAsking": [] }, "sections": [], "defaultModel": null, "avatarDrawingModel": null,
            "beta": { "projects": false },
            "notifications": { "enabled": false, "sound": true, "preview": "full", "when": "any" },
            "groupAnswerPolicy": "last",
        })
    }

    /// Known notification fields from the file. Missing or invalid values stay at the defaults.
    /// Unknown keys are dropped, the same way `beta` keeps only `projects`.
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

    /// `settings.json` with hbots defaults filled in.
    pub async fn settings(&self) -> Value {
        let saved = storage::read_json(&self.settings_path())
            .await
            .unwrap_or_else(|| json!({}));
        let mut out = storage::merge(&Self::settings_defaults(), &saved);
        out["beta"] = json!({ "projects": saved["beta"]["projects"] == true });
        out["notifications"] = Self::notifications_view(&saved);
        out["groupAnswerPolicy"] = json!(group_answer_policy(&out).as_str());
        out
    }
}

pub(crate) fn group_answer_policy(settings: &Value) -> rooms::AnswerPolicy {
    rooms::AnswerPolicy::parse(settings["groupAnswerPolicy"].as_str())
        .unwrap_or(rooms::AnswerPolicy::Last)
}

pub(crate) type S = State<Arc<AppState>>;

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
