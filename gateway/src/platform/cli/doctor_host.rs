//! Disk free space and the tail of the gateway log for `hbots doctor`.

use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub(super) fn free_bytes(path: &Path) -> Option<u64> {
    let target = if path.exists() {
        path.to_path_buf()
    } else {
        path.ancestors()
            .find(|dir| dir.exists())
            .map(Path::to_path_buf)?
    };
    let output = std::process::Command::new("df")
        .args(["-kP"])
        .arg(&target)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_df_available(&String::from_utf8_lossy(&output.stdout))
}

pub(super) fn parse_df_available(text: &str) -> Option<u64> {
    let line = text.lines().nth(1)?;
    let kib: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(kib.saturating_mul(1024))
}

pub(super) fn tail_lines(path: &Path, count: usize, max_bytes: u64) -> Vec<String> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let start = len.saturating_sub(max_bytes);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    if start > 0 {
        if let Some(pos) = buf.iter().position(|byte| *byte == b'\n') {
            buf.drain(..=pos);
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let lines: Vec<&str> = text.lines().collect();
    let skip = lines.len().saturating_sub(count);
    lines.into_iter().skip(skip).map(str::to_string).collect()
}
