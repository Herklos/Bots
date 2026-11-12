//! The hbots brand in one place.
//!
//! `tokens.json` is the source. This crate reads it, checks the brand's own rules (contrast,
//! the palette) and writes every file that repeats a brand value: the app's design
//! (`app/app.tamber`), the marked regions of `app/host/palette.ts`, `app/app.contract` and
//! `app/face.contract`, `app/app.json`, a stylesheet and the logo files. `hbots-design sync`
//! writes them; `hbots-design check` and this crate's tests fail when one is out of step.
//!
//! The guidelines are in `design/docs/`.

use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::LazyLock,
};

mod color;
mod regions;
mod svg;
#[cfg(test)]
mod tests;
mod tokens;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub use color::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub use regions::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub use svg::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub use tokens::*;

// ---------- keeping the files in step ----------

/// One file this crate writes, relative to the repository root.
#[derive(Debug)]
pub struct Output {
    pub path: &'static str,
    /// `None`: the whole file. `Some(name)`: the lines between `// design:begin name` and
    /// `// design:end name`.
    pub region: Option<&'static str>,
    pub text: String,
}

pub fn outputs() -> Vec<Output> {
    let whole = |path, text| Output {
        path,
        region: None,
        text,
    };
    vec![
        whole("app/app.tamber", tamber()),
        whole("design/generated/tokens.css", css()),
        whole("design/assets/mark.svg", mark_svg()),
        whole("design/assets/favicon.svg", favicon_svg()),
        whole("design/assets/app-icon.svg", app_icon_svg()),
        whole("design/assets/lockup.svg", lockup_svg()),
        Output {
            path: "app/host/palette.ts",
            region: Some("palette"),
            text: ts_region(),
        },
        Output {
            path: "app/ui/fns.contract",
            region: Some("colours"),
            text: contract_region(),
        },
        Output {
            path: "app/face.contract",
            region: Some("colours"),
            text: face_region(),
        },
        Output {
            path: "app/face.contract",
            region: Some("shapes"),
            text: shapes_region(),
        },
    ]
}

/// `text` with the named region's lines replaced, or why it can't be.
pub fn replace_region(text: &str, name: &str, body: &str) -> Result<String, String> {
    let (begin, end) = (
        format!("// design:begin {name}"),
        format!("// design:end {name}"),
    );
    let start = text
        .find(&begin)
        .ok_or_else(|| format!("no `{begin}` line"))?;
    let after_begin = start
        + text[start..]
            .find('\n')
            .ok_or("the begin line has no end")?
        + 1;
    let stop = text[after_begin..]
        .find(&end)
        .map(|i| after_begin + i)
        .ok_or_else(|| format!("no `{end}` line"))?;
    Ok(format!("{}{body}{}", &text[..after_begin], &text[stop..]))
}

/// `app.json` with the brand's `theme_color` and `background_color`.
pub fn app_json(text: &str) -> Result<String, String> {
    let c = &tokens().color;
    let set = |text: &str, key: &str, value: &str| -> Result<String, String> {
        let lead = format!("\"{key}\": \"");
        let at = text.find(&lead).ok_or_else(|| format!("no `{key}`"))? + lead.len();
        let len = text[at..].find('"').ok_or("an unclosed string")?;
        Ok(format!("{}{value}{}", &text[..at], &text[at + len..]))
    };
    let text = set(text, "theme_color", &c.tide)?;
    set(&text, "background_color", &c.white)
}

/// The repository root: the folder above this crate.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate sits in the repository")
        .to_path_buf()
}

/// Brings every generated file in step with the tokens. Returns the paths that differed; with
/// `write` false nothing is changed.
pub fn sync(root: &Path, write: bool) -> Result<Vec<String>, String> {
    let mut changed = Vec::new();
    let mut put = |path: &str, want: String, have: Option<String>| -> Result<(), String> {
        if have.as_deref() != Some(want.as_str()) {
            changed.push(path.to_string());
            if write {
                let file = root.join(path);
                if let Some(dir) = file.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("{path}: {e}"))?;
                }
                std::fs::write(&file, want).map_err(|e| format!("{path}: {e}"))?;
            }
        }
        Ok(())
    };
    for out in outputs() {
        let have = std::fs::read_to_string(root.join(out.path)).ok();
        let want = match out.region {
            None => out.text,
            Some(name) => {
                let text = have
                    .as_deref()
                    .ok_or_else(|| format!("{}: missing", out.path))?;
                replace_region(text, name, &out.text).map_err(|e| format!("{}: {e}", out.path))?
            }
        };
        put(out.path, want, have)?;
    }
    let path = "app/app.json";
    let have = std::fs::read_to_string(root.join(path)).map_err(|e| format!("{path}: {e}"))?;
    put(
        path,
        app_json(&have).map_err(|e| format!("{path}: {e}"))?,
        Some(have),
    )?;
    Ok(changed)
}
