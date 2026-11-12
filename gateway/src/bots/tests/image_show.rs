use super::*;
use crate::bots::image_tools::{dispatch, is_image_tool, tool_defs, Deps};
use crate::chat::images::{marks, png_bytes, MAX_IMAGE_BYTES};
use crate::storage::LocalDisk;
use serde_json::{json, Value};
use std::os::unix::fs::symlink;
use std::path::Path;

struct Scene {
    _dir: tempfile::TempDir,
    ws: LocalDisk,
    root: std::path::PathBuf,
}

impl Scene {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let ws = LocalDisk::new(root.clone());
        Self {
            _dir: dir,
            ws,
            root,
        }
    }

    fn bot(&self, slug: &str, thread: &str, role: &str, parent: Option<&str>, hide: bool) -> Value {
        json!({
            "slug": slug, "name": slug, "mainThreadId": thread, "role": role,
            "parentSlug": parent, "hideFromSidebar": hide,
            "workspaceRoot": self.root.join(slug).to_string_lossy(),
        })
    }

    async fn put(&self, rel: &str, bytes: &[u8]) {
        self.ws.write(rel, bytes, None).await.unwrap();
    }

    async fn show(
        &self,
        roster: &[Value],
        thread: &str,
        path: &str,
        extra: Value,
    ) -> Result<Value, String> {
        let mut args = json!({ "threadId": thread, "path": path });
        if let Some(fields) = extra.as_object() {
            for (key, value) in fields {
                args[key] = value.clone();
            }
        }
        dispatch(
            &Deps {
                workspace: &self.ws,
                roster,
            },
            "image_show",
            &args,
        )
        .await
    }

    async fn absent(&self, rel: &str) -> bool {
        self.ws.symlink_kind(rel).await.unwrap().is_none()
    }
}

#[test]
fn the_prompt_names_image_show() {
    assert!(is_image_tool("image_show"));
    assert!(!is_image_tool("vault_list"));
    assert_eq!(tool_defs()[0]["name"], "image_show");
    let text = render_agents_md(
        &json!({ "name": "Ada", "slug": "ada" }),
        Path::new("/bots"),
        None,
        &[],
        &MemorySnapshot::default(),
    );
    let line = "To show the person a picture, save it as PNG, JPEG, GIF or WebP under `library/`, call `image_show`, and put the Markdown it returns in your reply.";
    assert!(text.contains(line));
}

#[tokio::test]
async fn copies_a_picture_from_a_scratch_folder() {
    let scene = Scene::new();
    let png = png_bytes(12, 8);
    let other = png_bytes(3, 5);
    scene.put("ada/scratch/chart.png", &png).await;
    scene.put("ada/shots/chart.png", &other).await;
    let roster = vec![scene.bot("ada", "thread-ada", "bot", None, false)];
    let got = scene
        .show(
            &roster,
            "thread-ada",
            "scratch/chart.png",
            json!({ "alt": "Chart", "slug": "bea" }),
        )
        .await
        .unwrap();
    assert_eq!(got["markdown"], "![Chart](library/chart.png)");
    assert_eq!(got["bytes"], png.len());
    assert_eq!(got["mime"], "image/png");
    assert_eq!(got["width"], 12);
    assert_eq!(got["height"], 8);
    assert_eq!(scene.ws.read("ada/library/chart.png").await.unwrap(), png);
    assert_eq!(scene.ws.read("ada/scratch/chart.png").await.unwrap(), png);
    let root = scene.root.join("ada").to_string_lossy().into_owned();
    assert_eq!(
        marks(got["markdown"].as_str().unwrap(), &root)[0]
            .path
            .as_deref(),
        Some("library/chart.png")
    );
    let absolute = scene.root.join("ada/shots/chart.png");
    let again = scene
        .show(
            &roster,
            "thread-ada",
            &absolute.to_string_lossy(),
            json!({}),
        )
        .await
        .unwrap();
    assert_eq!(again["markdown"], "![chart-2](library/chart-2.png)");
    assert_eq!(again["width"], 3);
    assert_eq!(again["height"], 5);
    assert_eq!(
        scene.ws.read("ada/library/chart-2.png").await.unwrap(),
        other
    );
    assert_eq!(scene.ws.read("ada/library/chart.png").await.unwrap(), png);
    let same = scene
        .show(&roster, "thread-ada", "library/chart.png", json!({}))
        .await
        .unwrap();
    assert_eq!(same["markdown"], "![chart](library/chart.png)");
    assert!(scene.absent("ada/library/chart-3.png").await);
}

#[tokio::test]
async fn refuses_a_symlink() {
    let scene = Scene::new();
    scene.put("ada/scratch/real.png", &png_bytes(2, 2)).await;
    symlink("real.png", scene.root.join("ada/scratch/link.png")).unwrap();
    let roster = vec![scene.bot("ada", "thread-ada", "bot", None, false)];
    let err = scene
        .show(&roster, "thread-ada", "scratch/link.png", json!({}))
        .await
        .unwrap_err();
    assert_eq!(err, "is a symlink");
    assert!(scene.absent("ada/library/link.png").await);
    assert!(scene.absent("ada/library/real.png").await);
}

#[tokio::test]
async fn refuses_a_file_larger_than_5_mib() {
    let scene = Scene::new();
    let mut bytes = png_bytes(1, 1);
    bytes.resize(MAX_IMAGE_BYTES + 1, 0);
    scene.put("ada/scratch/big.png", &bytes).await;
    let roster = vec![scene.bot("ada", "thread-ada", "bot", None, false)];
    let err = scene
        .show(&roster, "thread-ada", "scratch/big.png", json!({}))
        .await
        .unwrap_err();
    assert_eq!(err, "larger than 5 MiB");
    assert!(scene.absent("ada/library/big.png").await);
}

#[tokio::test]
async fn refuses_the_wrong_type() {
    let scene = Scene::new();
    let png = png_bytes(2, 2);
    scene.put("ada/scratch/notes.txt", b"hello").await;
    scene.put("ada/scratch/fake.png", b"hello").await;
    scene.put("ada/.secret/a.png", &png).await;
    scene.put("ada/memory/a.png", &png).await;
    let roster = vec![scene.bot("ada", "thread-ada", "bot", None, false)];
    let err = |path: &'static str| scene.show(&roster, "thread-ada", path, json!({}));
    assert_eq!(
        err("scratch/notes.txt").await.unwrap_err(),
        "unsupported type"
    );
    assert_eq!(err("scratch/fake.png").await.unwrap_err(), "not an image");
    assert_eq!(
        err(".secret/a.png").await.unwrap_err(),
        "That path is not available."
    );
    assert_eq!(
        err("memory/a.png").await.unwrap_err(),
        "That path is not available."
    );
    for name in ["notes.txt", "fake.png", "a.png"] {
        assert!(scene.absent(&format!("ada/library/{name}")).await);
    }
}

#[tokio::test]
async fn a_subagent_writes_only_its_own_library() {
    let scene = Scene::new();
    let png = png_bytes(4, 3);
    scene.put("ada-jr/scratch/chart.png", &png).await;
    scene.put("ada/library/keep.png", &png).await;
    let roster = vec![
        scene.bot("ada", "thread-ada", "bot", None, false),
        scene.bot("ada-jr", "thread-jr", "subagent", Some("ada"), true),
    ];
    let got = scene
        .show(
            &roster,
            "thread-jr",
            "scratch/chart.png",
            json!({ "slug": "ada", "alt": "Chart" }),
        )
        .await
        .unwrap();
    assert_eq!(got["markdown"], "![Chart](library/chart.png)");
    assert_eq!(
        scene.ws.read("ada-jr/library/chart.png").await.unwrap(),
        png
    );
    assert!(scene.absent("ada/library/chart.png").await);
    assert_eq!(scene.ws.read("ada/library/keep.png").await.unwrap(), png);
    let outside = scene
        .root
        .join("ada/library/keep.png")
        .to_string_lossy()
        .into_owned();
    let err = scene
        .show(&roster, "thread-jr", &outside, json!({}))
        .await
        .unwrap_err();
    assert_eq!(err, "That path is not in this Bot's folder.");
    assert!(scene.absent("ada-jr/library/keep.png").await);
    let climbed = scene
        .show(&roster, "thread-jr", "../ada/library/keep.png", json!({}))
        .await
        .unwrap_err();
    assert_eq!(climbed, "That path is not in this Bot's folder.");
}
