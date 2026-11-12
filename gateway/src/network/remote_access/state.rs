use super::*;

/// The saved Tailscale route. An enabled route always has its ports and users saved with it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TailscaleSaved {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub https_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub users: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TunnelSaved {
    #[serde(default)]
    pub enabled: bool,
}

/// What `access.json` holds.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccessState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<StoredPassword>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailscale: Option<TailscaleSaved>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TunnelSaved>,
    /// `local`, `lan`, or `tailscale`. Absent on a file written before this choice existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
}

/// A missing or unreadable file reads as nothing saved.
pub(crate) fn read_state(file: &Path) -> AccessState {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn write_state(file: &Path, state: &AccessState) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(state).map_err(std::io::Error::other)? + "\n";
    let temp = PathBuf::from(format!("{}.{}.tmp", file.display(), std::process::id()));
    crate::network::auth::write_private(&temp, &text)?;
    std::fs::rename(temp, file)
}

/// An ISO-8601 UTC timestamp, without pulling in a date crate.
pub(crate) fn iso_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let days = i64::try_from(days).unwrap_or(i64::MAX);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a (year, month, day) in the proleptic Gregorian calendar.
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(0);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(0);
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

// ---------- the gateway's side ----------

#[derive(Clone, Debug)]
pub struct RemoteConfig {
    pub home: PathBuf,
    pub store_file: PathBuf,
    /// Where the tunnel's link is kept for the `bots` CLI to read.
    pub url_file: PathBuf,
    pub local_port: u16,
    pub env_password: String,
    pub env_tailnet: bool,
    pub env_tailnet_users: Vec<String>,
    /// The `tailscale` binary, or `None` when it isn't installed.
    pub tailscale_bin: Option<String>,
    /// `HBOTS_NETWORK_ACCESS`, from the process environment or `~/.hbots/.env`.
    pub network_env: Option<String>,
    /// `HOST` from the process environment, before dotenv reads the settings file.
    pub process_host: Option<String>,
    /// `HOST` after dotenv. `127.0.0.1` is the value `hbots up` writes and is not an override.
    pub merged_host: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ListenerNote {
    pub(crate) bound: String,
    pub(crate) error: Option<String>,
    pub(crate) withheld: bool,
}
