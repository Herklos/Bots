use super::*;

// ---------- versions ----------

pub(super) struct Semver<'a> {
    pub(super) core: [u64; 3],
    pub(super) pre: Option<&'a str>,
}

pub(super) fn number(s: &str) -> Option<u64> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

pub(super) fn parse_version(v: &str) -> Option<Semver<'_>> {
    let v = v.strip_prefix('v').unwrap_or(v);
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) if !pre.is_empty() => (core, Some(pre)),
        Some(_) => return None,
        None => (v, None),
    };
    let mut parts = core.split('.');
    let mut nums = [0u64; 3];
    for n in nums.iter_mut() {
        *n = number(parts.next()?)?;
    }
    parts.next().is_none().then_some(Semver { core: nums, pre })
}

/// Pre-release identifiers: numbers compare as numbers and sort before text.
pub(super) fn compare_pre(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.split('.'), b.split('.'));
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                let order = match (number(p), number(q)) {
                    (Some(m), Some(n)) => m.cmp(&n),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => p.cmp(q),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// Compare two `X.Y.Z[-pre]` versions. A release comes after its own pre-releases.
pub(crate) fn compare_versions(a: &str, b: &str) -> Result<Ordering> {
    let x = parse_version(a).ok_or_else(|| anyhow!("Not a version: {a}"))?;
    let y = parse_version(b).ok_or_else(|| anyhow!("Not a version: {b}"))?;
    Ok(match x.core.cmp(&y.core) {
        Ordering::Equal => match (x.pre, y.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(p), Some(q)) => compare_pre(p, q),
        },
        other => other,
    })
}

/// Previews are versions with a pre-release tag; they follow the preview channel.
pub(crate) fn is_preview(version: &str) -> bool {
    version.contains('-')
}

/// The rustc target triple the release archive is named with, matching `scripts/pack-release.sh`.
pub(crate) fn target() -> String {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        (arch, os) => return format!("{arch}-unknown-{os}"),
    }
    .to_string()
}

pub(crate) fn asset_name(version: &str, target: &str) -> String {
    format!("hbots-{version}-{target}.tar.gz")
}

/// The hash for `name` in a `SHA256SUMS` file (`<sha256>  <name>`, `*` before a binary name allowed).
pub(crate) fn expected_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let hash = fields.next()?;
        let file = fields.next()?;
        let file = file.strip_prefix('*').unwrap_or(file);
        (file == name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}
