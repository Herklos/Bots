//! The Bot face written into its workspace so T3 can use it as a project favicon.
//!
//! One still SVG: the idle outline and eyes from the face contract, filled so the eyes
//! read on the avatar colour. The file name carries a short hash of those bytes. A new
//! face is a new path, and a path T3 already has is left untouched.

use sha2::{Digest, Sha256};
use std::io;
use std::path::Path;

/// Write `.bot/avatar-<hash>.svg` and return that path relative to `workspace`.
///
/// The file is replaced only when its bytes differ. A symlink whose target leaves
/// `workspace` is refused, and nothing is written through it.
pub fn write_face(workspace: &Path, shape: &str, color: &str) -> io::Result<String> {
    let svg = face_svg(shape, color);
    let hash = short_hash(svg.as_bytes());
    let name = format!("avatar-{hash}.svg");
    let rel = format!(".bot/{name}");
    let root = workspace.canonicalize()?;
    let bot_dir = workspace.join(".bot");
    ensure_bot_dir(&root, &bot_dir)?;
    let dest = bot_dir.join(&name);
    stay_inside(&root, &dest)?;
    match std::fs::read(&dest) {
        Ok(existing) if existing == svg.as_bytes() => {}
        Ok(_) | Err(_) => {
            // A symlink was already checked. Re-check so a swap between the read and
            // the write cannot land outside the workspace.
            stay_inside(&root, &dest)?;
            std::fs::write(&dest, svg.as_bytes())?;
        }
    }
    Ok(rel)
}

/// The idle face: one filled outline and two idle eyes. Unknown shapes are a ball.
pub fn face_svg(shape: &str, color: &str) -> String {
    let tokens = hbots_design::tokens();
    let face = &tokens.face;
    let (path, dy, k) = face
        .shape(shape)
        .or_else(|| face.shape("ball"))
        .expect("the ball outline is in the design tokens");
    let fill = tokens
        .avatar
        .hex(color)
        .unwrap_or_else(|| tokens.avatar.default_hex());
    let ink = hbots_design::ink_on(fill);
    let eye = &face.eye;
    let width = eye.width * k;
    let height = eye.length * k;
    let y = num(eye.y + dy - height / 2.0);
    let w = num(width);
    let h = num(height);
    let gap = eye.gap * k;
    let left = num(50.0 - gap - width / 2.0);
    let right = num(50.0 + gap - width / 2.0);
    let rx = num(eye.corner);
    let white = tokens.avatar.hex("white").expect("the palette has white");
    let outline = if fill == white {
        format!(
            r#"<path d="{path}" fill="{fill}" stroke="{}" stroke-width="1.25"/>"#,
            tokens.color.mist
        )
    } else {
        format!(r#"<path d="{path}" fill="{fill}"/>"#)
    };
    format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">"#,
            "{outline}",
            r#"<rect x="{left}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{ink}"/>"#,
            r#"<rect x="{right}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{ink}"/>"#,
            "</svg>"
        ),
        outline = outline,
        left = left,
        right = right,
        y = y,
        w = w,
        h = h,
        rx = rx,
        ink = ink,
    )
}

fn short_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)[..12].to_string()
}

fn num(n: f64) -> String {
    let s = format!("{n:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn ensure_bot_dir(root: &Path, bot_dir: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(bot_dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => std::fs::create_dir(bot_dir),
        Err(e) => Err(e),
        Ok(meta) if meta.file_type().is_symlink() => {
            stay_inside(root, bot_dir)?;
            if bot_dir.canonicalize()?.is_dir() {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    ".bot is not a directory",
                ))
            }
        }
        Ok(meta) if meta.is_dir() => stay_inside(root, bot_dir),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            ".bot is not a directory",
        )),
    }
}

/// `path` may not exist yet. Its resolved parent, and the path itself when it is a
/// symlink, must stay under `root`.
fn stay_inside(root: &Path, path: &Path) -> io::Result<()> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().unwrap_or(path);
            let real = parent.canonicalize()?;
            if !real.starts_with(root) {
                return Err(leaves_workspace());
            }
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    if meta.file_type().is_symlink() {
        let target = std::fs::canonicalize(path)?;
        if !target.starts_with(root) {
            return Err(leaves_workspace());
        }
        return Ok(());
    }
    let real = std::fs::canonicalize(path)?;
    if !real.starts_with(root) {
        return Err(leaves_workspace());
    }
    Ok(())
}

fn leaves_workspace() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "face file leaves the workspace",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shape_emits_its_path() {
        let face = &hbots_design::tokens().face;
        for name in face.shape_names() {
            let (path, _, _) = face.shape(name).unwrap();
            let svg = face_svg(name, "violet");
            assert!(svg.contains(path), "{name}");
            assert!(svg.contains(r#"xmlns="http://www.w3.org/2000/svg""#));
            assert!(!svg.contains("animate") && !svg.contains("circle"));
        }
    }

    #[test]
    fn a_white_face_has_a_mist_edge() {
        let mist = &hbots_design::tokens().color.mist;
        let white = face_svg("ball", "white");
        assert!(white.contains(&format!(r#"stroke="{mist}" stroke-width="1.25""#)));
        assert!(!face_svg("ball", "teal").contains("stroke"));
    }

    #[test]
    fn an_unknown_shape_is_a_ball() {
        assert_eq!(face_svg("nope", "teal"), face_svg("ball", "teal"));
        let ball = hbots_design::tokens().face.shape("ball").unwrap().0;
        assert!(face_svg("nope", "teal").contains(ball));
    }

    #[test]
    fn the_same_face_returns_the_same_path() {
        let dir = tempfile::tempdir().unwrap();
        let first = write_face(dir.path(), "hex", "orange").unwrap();
        let second = write_face(dir.path(), "hex", "orange").unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with(".bot/avatar-"));
        assert_eq!(
            std::path::Path::new(&first).extension(),
            Some(std::ffi::OsStr::new("svg"))
        );
        assert_eq!(first.len(), ".bot/avatar-".len() + 12 + ".svg".len());
        let bytes = std::fs::read(dir.path().join(".bot").join(&first[".bot/".len()..])).unwrap();
        assert_eq!(bytes, face_svg("hex", "orange").into_bytes());
    }

    #[test]
    fn a_changed_colour_changes_the_hash() {
        let dir = tempfile::tempdir().unwrap();
        let teal = write_face(dir.path(), "ball", "teal").unwrap();
        let pink = write_face(dir.path(), "ball", "pink").unwrap();
        assert_ne!(teal, pink);
        assert!(teal.contains("avatar-") && pink.contains("avatar-"));
    }

    #[test]
    fn a_symlink_out_of_the_workspace_is_refused() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), workspace.path().join(".bot")).unwrap();
        let err = write_face(workspace.path(), "ball", "teal").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(outside.path().read_dir().unwrap().next().is_none());
    }

    #[test]
    fn a_file_symlink_out_of_the_workspace_is_refused() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let rel = write_face(workspace.path(), "drop", "blue").unwrap();
        let file = workspace.path().join(".bot").join(&rel[".bot/".len()..]);
        let leaked = outside.path().join("avatar.svg");
        std::fs::write(&leaked, b"kept").unwrap();
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&leaked, &file).unwrap();
        let err = write_face(workspace.path(), "drop", "blue").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read(&leaked).unwrap(), b"kept");
    }
}
