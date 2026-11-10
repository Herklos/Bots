//! Small JSON files on disk, written via a temp file and then rename.
use serde_json::Value;
use std::path::Path;

pub async fn read_json(path: &Path) -> Option<Value> {
    let text = tokio::fs::read_to_string(path).await.ok()?;
    serde_json::from_str(&text).ok()
}

pub async fn write_json(path: &Path, value: &Value) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(value).expect("JSON values serialize") + "\n";
    write_atomic(path, text.as_bytes(), None).await
}

pub async fn write_atomic(path: &Path, bytes: &[u8], mode: Option<u32>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let tmp = path.with_extension(format!("{}.tmp", rand::random::<u32>()));
    {
        use tokio::io::AsyncWriteExt;
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(mode.unwrap_or(0o644));
        let mut file = options.open(&tmp).await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
    }
    tokio::fs::rename(&tmp, path).await
}

/// Shallow merge, as JavaScript's `{ ...a, ...b }`.
pub fn merge(base: &Value, patch: &Value) -> Value {
    let mut out = base.as_object().cloned().unwrap_or_default();
    if let Some(p) = patch.as_object() {
        for (k, v) in p {
            out.insert(k.clone(), v.clone());
        }
    }
    Value::Object(out)
}
