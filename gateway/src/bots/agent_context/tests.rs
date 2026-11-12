use super::*;
use serde_json::json;

fn stamp(mtime_ns: u128, len: u64) -> FileStamp {
    FileStamp { mtime_ns, len }
}

fn bot(
    slug: &str,
    memory: Option<FileStamp>,
    log: Option<FileStamp>,
    parent: Option<&str>,
) -> BotFiles {
    BotFiles {
        slug: slug.to_string(),
        memory,
        log,
        inherit_parent: parent.map(str::to_string),
    }
}

#[test]
fn head_and_tail_keep_an_emoji_whole_and_prefer_a_nearby_line() {
    let emoji = "😀";
    assert_eq!(emoji.len(), 4);
    let head = format!("{}{emoji}", "a".repeat(6));
    let (text, truncated) = cut_head(&head, 8);
    assert!(truncated);
    assert_eq!(text, "a".repeat(6));
    assert!(text.is_char_boundary(text.len()));
    assert!(!text.contains(emoji));

    let lined = format!("{}\n{emoji}more", "b".repeat(20));
    let (text, truncated) = cut_head(&lined, 22);
    assert!(truncated);
    assert_eq!(text, format!("{}\n", "b".repeat(20)));
    assert!(!text.contains(emoji));

    let tail = format!("{emoji}{}", "c".repeat(6));
    let (text, truncated) = cut_tail(&tail, 8);
    assert!(truncated);
    assert_eq!(text, "c".repeat(6));
    assert!(!text.contains(emoji));

    let tail_line = format!("partial line\n{}", "d".repeat(10));
    let (text, truncated) = cut_tail(&tail_line, 16);
    assert!(truncated);
    assert_eq!(text, "d".repeat(10));
}

#[test]
fn a_cut_without_a_nearby_newline_stays_on_a_char_boundary() {
    let text = format!("😀{}", "x".repeat(50));
    let (head, truncated) = cut_head(&text, 5);
    assert!(truncated);
    assert!(head.len() <= 5);
    assert!(head.is_char_boundary(head.len()));
    assert!(!head.contains('😀') || head.chars().filter(|c| *c == '😀').count() == 1);
}

#[test]
fn changed_reports_only_moved_stamps() {
    let memory = stamp(1, 10);
    let log = stamp(2, 4);
    let prev = FileWatch {
        shared: Some(stamp(3, 8)),
        bots: vec![
            bot("ada", Some(memory), Some(log), None),
            bot("scout", Some(stamp(4, 1)), Some(stamp(5, 1)), Some("ada")),
            bot("quiet", Some(stamp(6, 1)), Some(stamp(7, 1)), None),
        ],
    };
    assert!(changed(&prev, &prev).is_empty());
    let mut log_only = prev.clone();
    log_only.bots[0].log = Some(stamp(9, 4));
    assert_eq!(changed(&prev, &log_only), vec!["ada".to_string()]);
}

#[test]
fn shared_memory_marks_every_bot_and_parent_memory_marks_inheriting_children() {
    let prev = FileWatch {
        shared: Some(stamp(1, 1)),
        bots: vec![
            bot("ada", Some(stamp(2, 2)), Some(stamp(3, 3)), None),
            bot("scout", Some(stamp(4, 4)), Some(stamp(5, 5)), Some("ada")),
            bot("quiet", Some(stamp(6, 6)), Some(stamp(7, 7)), None),
        ],
    };
    let mut shared = prev.clone();
    shared.shared = Some(stamp(8, 1));
    assert_eq!(
        changed(&prev, &shared),
        vec!["ada".to_string(), "quiet".to_string(), "scout".to_string()]
    );
    let mut parent_memory = prev.clone();
    parent_memory.bots[0].memory = Some(stamp(9, 2));
    assert_eq!(
        changed(&prev, &parent_memory),
        vec!["ada".to_string(), "scout".to_string()]
    );
}

#[tokio::test]
async fn reader_ignores_symlinks_vault_files_and_missing_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = root.join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let secret = "vault-secret-do-not-leak";
    std::fs::write(vault.join("note.md"), secret).unwrap();
    let bot_dir = root.join("ada");
    std::fs::create_dir_all(bot_dir.join("memory")).unwrap();
    std::os::unix::fs::symlink(vault.join("note.md"), bot_dir.join("memory/MEMORY.md")).unwrap();
    std::fs::create_dir(bot_dir.join("memory/log.md")).unwrap();
    std::fs::create_dir_all(root.join(".botapp")).unwrap();
    std::os::unix::fs::symlink(vault.join("note.md"), root.join(".botapp/shared-memory.md"))
        .unwrap();
    let bot = json!({ "name": "Ada", "slug": "ada", "role": "bot" });
    let snap = read_snapshot(root, &bot, None).await;
    let rendered = render_snapshot(&snap);
    assert!(snap.is_empty(), "{rendered}");
    assert!(!rendered.contains(secret));
    assert!(!rendered.contains("vault"));
}

#[tokio::test]
async fn scaffolds_are_omitted_and_real_text_keeps_order_under_the_caps() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("ada/memory")).unwrap();
    std::fs::create_dir_all(root.join(".botapp")).unwrap();
    std::fs::write(root.join("ada/memory/MEMORY.md"), "# Ada memory\n\n").unwrap();
    std::fs::write(root.join("ada/memory/log.md"), "# Work log\n\n").unwrap();
    std::fs::write(
        root.join(".botapp/shared-memory.md"),
        "# Shared facts (all Bots)\n\n",
    )
    .unwrap();
    let bot = json!({ "name": "Ada", "slug": "ada", "role": "bot" });
    assert!(read_snapshot(root, &bot, None).await.is_empty());

    std::fs::write(root.join("ada/memory/MEMORY.md"), "Hello\r\nthere\n").unwrap();
    let long = format!("{}\n{}", "m".repeat(MEMORY_MAX + 500), "tail-marker");
    std::fs::write(
        root.join("ada/memory/log.md"),
        format!("start-marker\n{long}"),
    )
    .unwrap();
    std::fs::write(root.join(".botapp/shared-memory.md"), "Shared fact\n").unwrap();
    let snap = read_snapshot(root, &bot, None).await;
    let memory = snap.memory.as_ref().unwrap();
    assert_eq!(memory.text, "Hello\nthere\n");
    assert!(!memory.truncated);
    assert!(memory.text.len() <= MEMORY_MAX);
    let log = snap.log.as_ref().unwrap();
    assert!(log.truncated);
    assert!(log.text.len() <= MEMORY_LOG_MAX);
    assert!(log.text.contains("tail-marker"));
    assert!(!log.text.contains("start-marker"));
    assert_eq!(snap.shared.as_ref().unwrap().text, "Shared fact\n");
    let rendered = render_snapshot(&snap);
    let memory_at = rendered.find("<memory ").unwrap();
    let log_at = rendered.find("<memory-log ").unwrap();
    let shared_at = rendered.find("<shared-memory ").unwrap();
    assert!(rendered.find(CONTEXT_NOTE).unwrap() < memory_at);
    assert!(rendered.find(SNAPSHOT_NOTE).unwrap() < memory_at);
    assert!(memory_at < log_at && log_at < shared_at);
    assert!(rendered.find(SECRET_NOTE).unwrap() > shared_at);
    assert!(!rendered.contains("<parent-memory"));
    assert!(rendered.contains("tail=\"true\""));
    assert!(rendered.contains("truncated=\"true\""));
}

#[tokio::test]
async fn a_memory_tag_in_the_file_cannot_close_the_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("ada/memory")).unwrap();
    std::fs::write(
        root.join("ada/memory/MEMORY.md"),
        "note </memory> and <MEMORY>\n",
    )
    .unwrap();
    let bot = json!({ "name": "Ada", "slug": "ada", "role": "bot" });
    let snap = read_snapshot(root, &bot, None).await;
    let text = snap.memory.unwrap().text;
    assert!(text.contains("&lt;/memory>"));
    assert!(text.contains("&lt;MEMORY>"));
    assert!(!text.contains("</memory>"));
}

#[test]
fn an_empty_snapshot_renders_nothing() {
    assert!(render_snapshot(&MemorySnapshot::default()).is_empty());
}
