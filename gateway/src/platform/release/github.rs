use super::*;

// ---------- finding a release ----------

/// A release to install: its version, where to download the archive and the sums, and what GitHub says.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Release {
    pub version: String,
    pub archive: String,
    pub sums: String,
    pub notes: String,
    pub url: String,
    pub published_at: Option<String>,
}

/// Where releases come from: the GitHub API list (or a mirror of it) and, optionally, a download base.
#[derive(Clone, Debug)]
pub(crate) struct ReleaseSource {
    pub api: String,
    pub base: Option<String>,
}

impl ReleaseSource {
    pub(crate) fn from_env() -> Self {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Self {
            api: get("HBOTS_RELEASES_URL").unwrap_or_else(|| {
                format!("https://api.github.com/repos/{REPO}/releases?per_page=30")
            }),
            base: get("HBOTS_RELEASE_BASE_URL").map(|b| b.trim_end_matches('/').to_string()),
        }
    }
}

pub(super) fn tag_of(release: &Value) -> &str {
    release["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v')
}

/// Pick the release from a GitHub-API-shaped list: an exact `version`, or the newest one.
pub(crate) fn pick_release(
    releases: &[Value],
    version: Option<&str>,
    preview: bool,
    base: Option<&str>,
    target: &str,
) -> Option<Release> {
    let candidates = releases.iter().filter(|r| {
        !r["draft"].as_bool().unwrap_or(false)
            && (preview || !r["prerelease"].as_bool().unwrap_or(false))
    });
    let wanted = match version {
        Some(v) => {
            let v = v.trim_start_matches('v');
            candidates.into_iter().find(|r| tag_of(r) == v)?
        }
        None => candidates
            .filter(|r| parse_version(tag_of(r)).is_some())
            .max_by(|a, b| compare_versions(tag_of(a), tag_of(b)).unwrap_or(Ordering::Equal))?,
    };
    let v = tag_of(wanted).to_string();
    let name = asset_name(&v, target);
    let (archive, sums) = match base {
        Some(base) => (
            format!("{base}/v{v}/{name}"),
            format!("{base}/v{v}/SHA256SUMS"),
        ),
        None => {
            let find = |file: &str| -> Option<String> {
                wanted["assets"]
                    .as_array()?
                    .iter()
                    .find(|a| a["name"] == file)?["browser_download_url"]
                    .as_str()
                    .map(String::from)
            };
            (find(&name)?, find("SHA256SUMS")?)
        }
    };
    Some(Release {
        url: wanted["html_url"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("https://github.com/{REPO}/releases/tag/v{v}")),
        notes: wanted["body"].as_str().unwrap_or("").to_string(),
        published_at: wanted["published_at"].as_str().map(String::from),
        version: v,
        archive,
        sums,
    })
}

pub(crate) fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(USER_AGENT_NAME)
        .timeout(Duration::from_secs(300))
        .build()?)
}

pub(crate) async fn find_release(
    client: &reqwest::Client,
    source: &ReleaseSource,
    version: Option<&str>,
    preview: bool,
) -> Result<Option<Release>> {
    let res = client
        .get(&source.api)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await?;
    if !res.status().is_success() {
        bail!("Couldn't list releases (HTTP {})", res.status().as_u16());
    }
    let releases: Vec<Value> = res.json().await?;
    Ok(pick_release(
        &releases,
        version,
        preview,
        source.base.as_deref(),
        &target(),
    ))
}

// ---------- installing ----------

pub(super) async fn download(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let res = client.get(url).send().await?;
    if !res.status().is_success() {
        bail!("Download failed: {url} (HTTP {})", res.status().as_u16());
    }
    Ok(res.bytes().await?.to_vec())
}

/// Unpack the archive into `dir` and check that its `hbots` reports `version`.
pub(super) fn unpack_and_check(bytes: &[u8], dir: &Path, version: &str) -> Result<()> {
    let archive = dir.join(".archive.tar.gz");
    std::fs::write(&archive, bytes)?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(dir)
        .status()
        .context("running tar")?;
    std::fs::remove_file(&archive).ok();
    if !status.success() {
        bail!("tar couldn't unpack the release");
    }
    let binary = dir.join("hbots");
    if !binary.is_file() {
        bail!("The archive has no hbots binary");
    }
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))?;
    let out = Command::new(&binary).arg("--version").output()?;
    let got = String::from_utf8_lossy(&out.stdout)
        .trim()
        .trim_start_matches("hbots ")
        .to_string();
    if got != version {
        bail!("The archive holds version {got}, not {version}");
    }
    Ok(())
}

/// Download, verify and unpack a release into `versions/<version>`. Returns that folder.
pub(crate) async fn install_release(
    client: &reqwest::Client,
    home: &Path,
    release: &Release,
    log: &dyn Fn(&str),
) -> Result<PathBuf> {
    let paths = RuntimePaths::new(home);
    let target_dir = paths.versions.join(&release.version);
    if target_dir.join(".install-complete").exists() {
        return Ok(target_dir);
    }
    std::fs::create_dir_all(&paths.versions)?;
    log(&format!("Downloading hbots {}", release.version));
    let archive = download(client, &release.archive).await?;
    let sums = download(client, &release.sums).await?;
    let name = asset_name(&release.version, &target());
    let expected = expected_sum(&String::from_utf8_lossy(&sums), &name)
        .ok_or_else(|| anyhow!("SHA256SUMS has no entry for {name}"))?;
    let actual = hex::encode(Sha256::digest(&archive));
    if actual != expected {
        bail!("Checksum mismatch for {name}: expected {expected}, got {actual}");
    }
    log(&format!("Checksum OK ({}…)", &actual[..12]));
    let staging = paths
        .versions
        .join(format!(".staging-{}", std::process::id()));
    std::fs::remove_dir_all(&staging).ok();
    std::fs::create_dir_all(&staging)?;
    if let Err(error) = unpack_and_check(&archive, &staging, &release.version) {
        std::fs::remove_dir_all(&staging).ok();
        return Err(error);
    }
    std::fs::write(
        staging.join(".install-complete"),
        format!("{}\n", iso8601(unix_now())),
    )?;
    std::fs::remove_dir_all(&target_dir).ok();
    std::fs::rename(&staging, &target_dir)?;
    Ok(target_dir)
}
