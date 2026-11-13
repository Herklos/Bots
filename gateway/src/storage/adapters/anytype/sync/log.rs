//! One debug line per sync decision. The line names the path, the decision, and the
//! shas. It never carries file bytes, an API key, or an idempotency key.
//!
//! The same line is appended to `sync.log` under the sync directory. The file is a
//! ring: once it passes the cap, the older half is dropped.

use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const LIMIT: usize = 64 * 1024;

pub(super) fn decision(
    dir: &Path,
    path: &str,
    decision: &str,
    base: Option<&str>,
    local: Option<&str>,
    remote: Option<&str>,
) {
    let base = field(base);
    let local = field(local);
    let remote = field(remote);
    let path = one_line(path);
    let decision = one_line(decision);
    tracing::debug!(
        path = %path,
        decision = %decision,
        base_sha = %base,
        local_sha = %local,
        remote_sha = %remote,
        "sync decision"
    );
    let line =
        format!("path={path} decision={decision} base={base} local={local} remote={remote}\n");
    if let Err(error) = append(dir, &line) {
        tracing::debug!(%error, "sync log was not written");
    }
}

fn field(sha: Option<&str>) -> String {
    match sha.map(str::trim).filter(|sha| !sha.is_empty()) {
        Some(sha) => one_line(sha),
        None => "-".to_string(),
    }
}

fn one_line(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { '_' } else { ch })
        .take(512)
        .collect()
}

fn append(dir: &Path, line: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("sync.log");
    {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        #[cfg(unix)]
        {
            let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
        }
        file.write_all(line.as_bytes())?;
    }
    if std::fs::metadata(&path)?.len() > LIMIT as u64 {
        trim(&path)?;
    }
    Ok(())
}

fn trim(path: &Path) -> io::Result<()> {
    let bytes = std::fs::read(path)?;
    let keep = LIMIT / 2;
    if bytes.len() <= keep {
        return Ok(());
    }
    let start = bytes.len() - keep;
    let aligned = bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|index| start + index + 1)
        .unwrap_or(start);
    let tail = &bytes[aligned.min(bytes.len())..];
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)?;
    file.write_all(tail)?;
    Ok(())
}
