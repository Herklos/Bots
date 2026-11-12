//! POSIX hook that tells Claude to re-read `AGENTS.md` after the managed block changes.
//!
//! Claude Code injects hook stdout as context and caps that text at 10,000 characters.
//! Past the cap it keeps a file plus a short preview, and it does not ask the model to
//! read the file. A filtered managed block can be larger than that (a 16 KiB prompt, plus
//! memory), so the hook prints one pointer line and never a truncated block.

use serde_json::{json, Value};
#[cfg(unix)]
use std::path::Path;

/// The one-line command this gateway used to store in `.claude/settings.json`.
pub(crate) const LEGACY_AGENTS_MD_HOOK: &str = r#"f="$CLAUDE_PROJECT_DIR/AGENTS.md"; [ -f "$f" ] || exit 0; sid=$(sed -n 's/.*"session_id" *: *"\([^"]*\)".*/\1/p' | head -n 1); [ -n "$sid" ] || exit 0; seen="${TMPDIR:-/tmp}/hbots-agents-$sid"; h=$(shasum "$f" | cut -d' ' -f1); old=$(cat "$seen" 2>/dev/null); printf %s "$h" > "$seen"; if [ -n "$old" ] && [ "$old" != "$h" ]; then echo "AGENTS.md changed. These are your current instructions:"; cat "$f"; fi"#;

pub(crate) const AGENTS_MD_PROMPT_HOOK: &str =
    r#"sh "$CLAUDE_PROJECT_DIR/.bot/hooks/agents-md.sh" prompt"#;

pub(crate) const AGENTS_MD_START_HOOK: &str =
    r#"sh "$CLAUDE_PROJECT_DIR/.bot/hooks/agents-md.sh" start"#;

/// `startup|resume|clear|compact` is an exact matcher list (`|` separates values).
/// `fork` is a new session: the first prompt records a baseline and prints nothing.
const SESSION_START_MATCHER: &str = "startup|resume|clear|compact";

pub(crate) const AGENTS_MD_SCRIPT: &str = r#"#!/bin/sh
# jq is not used: a minimal image may not have it. session_id is read with sed.
# shasum is not used either. cksum is POSIX. The hook always exits 0: a
# UserPromptSubmit exit of 2 would block the person's prompt.
# Claude Code caps hook stdout at 10000 characters and spills the rest to a file
# the model is not asked to read. The managed block can be larger, so a change
# prints a pointer instead of the block.
set -u
mode=${1:-}
root=${CLAUDE_PROJECT_DIR:-}
[ -n "$root" ] || exit 0
sid=$(sed -n 's/.*"session_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' || true)
sid=${sid%%
*}
[ -n "$sid" ] || exit 0
case "$sid" in
  *[!A-Za-z0-9_-]*) exit 0 ;;
esac
state="$root/.bot/hook-state"
mkdir -p "$state" || exit 0
if [ "$mode" = start ]; then
  find "$state" -type f -mtime +7 -exec rm -f {} + || exit 0
fi
agents="$root/AGENTS.md"
[ -f "$agents" ] || exit 0
block=$(sed -n '/<!-- bot:managed:start -->/,/<!-- bot:managed:end -->/p' "$agents" | sed '/<memory-log/,/<\/memory-log>/d') || exit 0
sum=$(printf '%s' "$block" | cksum) || exit 0
seen="$state/$sid"
old=
if [ -f "$seen" ]; then
  old=$(cat "$seen") || exit 0
fi
printf '%s' "$sum" > "$seen" || exit 0
if [ "$mode" = start ]; then
  exit 0
fi
if [ -z "$old" ]; then
  exit 0
fi
if [ "$old" != "$sum" ]; then
  echo "AGENTS.md changed. Re-read AGENTS.md."
fi
exit 0
"#;

/// Drop the legacy one-liner. On Unix, keep one `UserPromptSubmit` entry and one
/// `SessionStart` entry that call the script. Other hooks stay. Non-Unix installs neither.
pub(crate) fn install_agents_md_hooks(cs: &mut Value) {
    let mut prompt = event_entries(cs, "UserPromptSubmit");
    strip_exact_command(&mut prompt, LEGACY_AGENTS_MD_HOOK);
    #[cfg(unix)]
    {
        if !cs["hooks"].is_object() {
            cs["hooks"] = json!({});
        }
        if !has_script_mode(&prompt, "prompt") {
            prompt.push(json!({
                "hooks": [{ "type": "command", "command": AGENTS_MD_PROMPT_HOOK }]
            }));
        }
        cs["hooks"]["UserPromptSubmit"] = json!(prompt);
        let mut start = event_entries(cs, "SessionStart");
        if !has_script_mode(&start, "start") {
            start.push(json!({
                "matcher": SESSION_START_MATCHER,
                "hooks": [{ "type": "command", "command": AGENTS_MD_START_HOOK }]
            }));
        }
        cs["hooks"]["SessionStart"] = json!(start);
    }
    #[cfg(not(unix))]
    if cs["hooks"].is_object() {
        cs["hooks"]["UserPromptSubmit"] = json!(prompt);
    }
}

#[cfg(unix)]
pub(crate) async fn write_agents_hook_script(path: &Path) -> std::io::Result<()> {
    let current = tokio::fs::read(path).await.unwrap_or_default();
    if current == AGENTS_MD_SCRIPT.as_bytes() {
        return Ok(());
    }
    crate::storage::write_atomic(path, AGENTS_MD_SCRIPT.as_bytes(), Some(0o755)).await
}

fn event_entries(cs: &Value, event: &str) -> Vec<Value> {
    cs["hooks"][event].as_array().cloned().unwrap_or_default()
}

fn strip_exact_command(entries: &mut Vec<Value>, command: &str) {
    for entry in entries.iter_mut() {
        let Some(hooks) = entry.get_mut("hooks").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        hooks.retain(|hook| hook["command"].as_str() != Some(command));
    }
    entries.retain(|entry| match entry["hooks"].as_array() {
        Some(hooks) => !hooks.is_empty(),
        None => true,
    });
}

fn has_script_mode(entries: &[Value], mode: &str) -> bool {
    entries.iter().any(|entry| {
        entry["hooks"].as_array().is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook["command"]
                    .as_str()
                    .is_some_and(|c| c.contains(".bot/hooks/agents-md.sh") && c.ends_with(mode))
            })
        })
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::process::{Command, Stdio};

    fn install_from(existing: Value) -> Value {
        let mut cs = existing;
        install_agents_md_hooks(&mut cs);
        cs
    }

    fn commands(cs: &Value, event: &str) -> Vec<String> {
        cs["hooks"][event]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|entry| entry["hooks"].as_array().cloned().unwrap_or_default())
            .filter_map(|hook| hook["command"].as_str().map(str::to_string))
            .collect()
    }

    #[test]
    fn legacy_hook_migrates_once_and_keeps_a_foreign_hook() {
        let legacy = json!({
            "hooks": {
                "UserPromptSubmit": [{
                    "hooks": [{ "type": "command", "command": LEGACY_AGENTS_MD_HOOK }]
                }]
            }
        });
        let once = install_from(legacy);
        assert_eq!(
            commands(&once, "UserPromptSubmit"),
            vec![AGENTS_MD_PROMPT_HOOK]
        );
        assert_eq!(commands(&once, "SessionStart"), vec![AGENTS_MD_START_HOOK]);
        assert_eq!(
            once["hooks"]["SessionStart"][0]["matcher"],
            SESSION_START_MATCHER
        );
        let twice = install_from(once.clone());
        assert_eq!(twice, once);

        let foreign = json!({
            "hooks": {
                "Stop": [{ "hooks": [{ "type": "command", "command": "true" }] }],
                "UserPromptSubmit": [{
                    "hooks": [
                        { "type": "command", "command": "true" },
                        { "type": "command", "command": LEGACY_AGENTS_MD_HOOK }
                    ]
                }]
            }
        });
        let merged = install_from(foreign);
        assert_eq!(
            commands(&merged, "UserPromptSubmit"),
            vec!["true".to_string(), AGENTS_MD_PROMPT_HOOK.to_string()]
        );
        assert_eq!(merged["hooks"]["Stop"][0]["hooks"][0]["command"], "true");
    }

    #[test]
    fn missing_hooks_key_gains_one_entry_per_event() {
        let cs = install_from(json!({ "enableAllProjectMcpServers": true }));
        assert_eq!(cs["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 1);
        assert_eq!(cs["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    }

    fn tool_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("hbots-hook-tools-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for name in ["sed", "cksum", "cat", "mkdir", "rm", "find"] {
            let mut found = None;
            for root in ["/bin", "/usr/bin"] {
                let candidate = Path::new(root).join(name);
                if candidate.exists() {
                    found = Some(candidate);
                    break;
                }
            }
            let found = found.unwrap_or_else(|| panic!("{name} is not in /bin or /usr/bin"));
            let link = dir.join(name);
            if !link.exists() {
                std::os::unix::fs::symlink(found, link).unwrap();
            }
        }
        dir
    }

    fn run(root: &Path, mode: &str, stdin: &str) -> (i32, String) {
        let script = root.join(".bot/hooks/agents-md.sh");
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(&script, AGENTS_MD_SCRIPT).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).unwrap();
        }
        let mut child = Command::new("/bin/sh")
            .arg(&script)
            .arg(mode)
            .env("CLAUDE_PROJECT_DIR", root)
            .env("PATH", tool_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let code = out.status.code().unwrap_or(1);
        let stdout = String::from_utf8(out.stdout).unwrap();
        if !out.stderr.is_empty() {
            panic!("stderr: {}", String::from_utf8_lossy(&out.stderr));
        }
        (code, stdout)
    }

    fn project() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn missing_file_and_bad_stdin_print_nothing() {
        let dir = project();
        let (code, out) = run(dir.path(), "prompt", "");
        assert_eq!((code, out.as_str()), (0, ""));
        let (code, out) = run(dir.path(), "prompt", "not json");
        assert_eq!((code, out.as_str()), (0, ""));
        fs::write(dir.path().join("AGENTS.md"), "hello\n").unwrap();
        for sid in ["a/b", "a..b", "../x"] {
            let (code, out) = run(
                dir.path(),
                "prompt",
                &format!(r#"{{"session_id":"{sid}"}}"#),
            );
            assert_eq!((code, out.as_str()), (0, ""), "{sid}");
        }
        assert!(!tool_dir().join("shasum").exists());
    }

    const POINTER: &str = "AGENTS.md changed. Re-read AGENTS.md.\n";

    fn agents(log: &str, fact: &str, instructions: &str) -> String {
        format!(
            "notes above\n<!-- bot:managed:start -->\n# Ada\n\n## Instructions\n\n{instructions}\n\n<memory-log tail=\"true\">\n{log}\n</memory-log>\n<memory>\n{fact}\n</memory>\n<!-- bot:managed:end -->\nnotes below\n"
        )
    }

    #[test]
    fn prompt_prints_a_pointer_only_when_the_filtered_block_changes() {
        let dir = project();
        let path = dir.path().join("AGENTS.md");
        fs::write(&path, agents("one", "fact", "be brief")).unwrap();
        let sid = r#"{"session_id":"sess-1"}"#;
        let (code, out) = run(dir.path(), "prompt", sid);
        assert_eq!((code, out.as_str()), (0, ""));
        let seen = fs::read(dir.path().join(".bot/hook-state/sess-1")).unwrap();
        assert!(!seen.is_empty());

        fs::write(&path, agents("two", "fact", "be brief")).unwrap();
        let (code, out) = run(dir.path(), "prompt", sid);
        assert_eq!((code, out.as_str()), (0, ""));

        fs::write(&path, agents("two", "newer fact", "be brief")).unwrap();
        let (code, out) = run(dir.path(), "prompt", sid);
        assert_eq!((code, out.as_str()), (0, POINTER));
        assert!(!out.contains("newer fact"));
        assert!(!out.contains("memory-log"));

        let (code, out) = run(dir.path(), "prompt", sid);
        assert_eq!((code, out.as_str()), (0, ""));

        fs::write(&path, agents("two", "newer fact", "be shorter")).unwrap();
        let (code, out) = run(dir.path(), "prompt", sid);
        assert_eq!((code, out.as_str()), (0, POINTER));
    }

    #[test]
    fn start_records_a_baseline_and_a_later_prompt_prints() {
        let dir = project();
        let path = dir.path().join("AGENTS.md");
        fs::write(&path, agents("one", "fact", "be brief")).unwrap();
        let sid = r#"{"session_id":"sess-2"}"#;
        let (code, out) = run(dir.path(), "start", sid);
        assert_eq!((code, out.as_str()), (0, ""));
        assert!(dir.path().join(".bot/hook-state/sess-2").exists());

        fs::write(&path, agents("one", "fact", "be shorter")).unwrap();
        let (code, out) = run(dir.path(), "prompt", sid);
        assert_eq!((code, out.as_str()), (0, POINTER));
        assert!(!out.contains("be shorter"));
    }

    #[tokio::test]
    async fn write_instructions_installs_one_script_and_drops_the_legacy_hook() {
        let dir = tempfile::tempdir().unwrap();
        let bots = crate::bots::Bots::new(dir.path().to_path_buf(), String::new(), String::new());
        bots.create(&json!({ "name": "Ada" })).await.unwrap();
        let script = dir.path().join("ada/.bot/hooks/agents-md.sh");
        let first = fs::read(&script).unwrap();
        assert_eq!(first, AGENTS_MD_SCRIPT.as_bytes());
        let before = fs::metadata(&script).unwrap().modified().unwrap();
        bots.write_instructions(&bots.get("ada").await.unwrap().unwrap())
            .await
            .unwrap();
        let after = fs::metadata(&script).unwrap().modified().unwrap();
        assert_eq!(before, after);
        let settings: Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("ada/.claude/settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            commands(&settings, "UserPromptSubmit"),
            vec![AGENTS_MD_PROMPT_HOOK]
        );
        assert!(!settings.to_string().contains("shasum"));
    }
}
