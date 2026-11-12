use super::super::policy::{accept_pulled, classify, Class, MergePolicy, PullReject};
#[cfg(unix)]
use std::os::unix::fs::symlink;

fn allow(rel: &str, merge: MergePolicy) {
    assert_eq!(classify(rel), Class::Allow { merge }, "{rel}");
}

fn deny(rel: &str) {
    assert_eq!(classify(rel), Class::Deny, "{rel}");
}

#[test]
fn allows_memory() {
    allow("ada/memory/sub/MEMORY.md", MergePolicy::Text3Way);
}

#[test]
fn allows_skills() {
    allow("ada/skills/note.txt", MergePolicy::Text3Way);
}

#[test]
fn allows_library() {
    allow("ada/library/Page.HTML", MergePolicy::Text3Way);
}

#[test]
fn allows_attachments() {
    allow("ada/.bot/attachments/file.bin", MergePolicy::Whole);
    allow(
        "ada/.bot/attachments/nested/file.txt",
        MergePolicy::Text3Way,
    );
}

#[test]
fn allows_library_json_as_a_whole_file() {
    allow("ada/.bot/library.json", MergePolicy::Whole);
    deny("ada/.bot/library.json.bak");
}

#[test]
fn allows_shared_memory() {
    allow(".botapp/shared-memory.md", MergePolicy::Text3Way);
}

#[test]
fn json_merges_as_a_whole_file() {
    allow("ada/memory/data.json", MergePolicy::Whole);
}

#[test]
fn denies_agents_md() {
    deny("ada/memory/AGENTS.md");
}

#[test]
fn denies_claude_md() {
    deny("ada/skills/CLAUDE.md");
}

#[test]
fn denies_mcp_json() {
    deny("ada/library/.mcp.json");
}

#[test]
fn denies_codex() {
    deny("ada/memory/.codex/config.toml");
}

#[test]
fn denies_claude_dir() {
    deny("ada/skills/.claude/settings.json");
}

#[test]
fn denies_bot_json() {
    deny("ada/.bot/bot.json");
}

#[test]
fn denies_local_json() {
    deny("ada/.bot/local.json");
}

#[test]
fn denies_connectors_json() {
    deny("ada/.bot/connectors.json");
}

#[test]
fn denies_the_rest_of_botapp() {
    deny(".botapp/other.md");
    deny(".botapp/nested/shared-memory.md");
    deny("ada/.botapp/shared-memory.md");
}

#[test]
fn denies_git() {
    deny("ada/memory/.git/config");
}

#[test]
fn denies_conflict_copies() {
    deny("ada/skills/note.conflict-ab12cd34-20261112.md");
}

#[test]
fn denies_tmp_files() {
    deny("ada/memory/draft.TMP");
}

#[test]
fn denies_lock_files() {
    deny("ada/library/page.lock");
}

#[test]
fn denies_a_path_that_leaves_the_root() {
    deny("../x");
    deny("/etc/passwd");
    deny("ada\\memory\\a.md");
    deny("");
    deny("ada//memory/a.md");
    deny("ada/memory/../../outside.md");
}

#[test]
fn denies_secret_file_names() {
    for name in [
        "vault.enc",
        ".vault-key",
        "access.json",
        "session-secret",
        ".bot-mcp-token",
        "t3-token",
        ".env",
        "storage.json",
        "anytype-key",
    ] {
        deny(&format!("ada/memory/{name}"));
    }
}

#[test]
fn denies_a_file_outside_the_allow_list() {
    deny("ada/README.md");
    deny("ada/.bot/notes.md");
    deny("memory/MEMORY.md");
    deny("ada/memory");
}

#[test]
fn pulled_parent_segment_is_refused() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        accept_pulled(root.path(), "ada/memory/../../outside.md"),
        Err(PullReject::Escapes)
    );
}

#[test]
fn pulled_denied_path_is_refused() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        accept_pulled(root.path(), "ada/.bot/bot.json"),
        Err(PullReject::Denied)
    );
}

#[cfg(unix)]
#[test]
fn pulled_symlink_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(outside.path().join("nested")).unwrap();
    std::fs::write(outside.path().join("nested/note.md"), "secret").unwrap();
    std::fs::create_dir_all(root.path().join("ada/skills")).unwrap();
    std::fs::create_dir_all(root.path().join("ada/library")).unwrap();
    std::fs::write(root.path().join("ada/library/note.md"), "ok").unwrap();
    symlink(outside.path(), root.path().join("ada/memory")).unwrap();
    symlink("note.md", root.path().join("ada/skills/link.md")).unwrap();

    assert_eq!(
        accept_pulled(root.path(), "ada/memory/nested/note.md"),
        Err(PullReject::Symlink)
    );
    assert_eq!(
        accept_pulled(root.path(), "ada/skills/link.md"),
        Err(PullReject::Symlink)
    );
    assert_eq!(
        accept_pulled(root.path(), "ada/library/note.md"),
        Ok(Class::Allow {
            merge: MergePolicy::Text3Way
        })
    );
    assert_eq!(
        accept_pulled(root.path(), "ada/library/fresh.md"),
        Ok(Class::Allow {
            merge: MergePolicy::Text3Way
        })
    );
}
