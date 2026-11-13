//! Cooperative inter-process locks for read/modify/write of machine-local state.
//! Older gateways do not take these locks; they must be stopped before migration.
use std::{fs::File, io, path::PathBuf};

/// Leave untouched filesystem-only homes alone. Once Anytype has used the home,
/// the persistent lock file also coordinates a later switch back to disk.
pub(crate) async fn acquire_if_active(path: PathBuf, enabled: bool) -> io::Result<Option<File>> {
    if !enabled {
        match tokio::fs::metadata(&path).await {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        }
    }
    acquire(path).await.map(Some)
}

pub(crate) async fn acquire(path: PathBuf) -> io::Result<File> {
    tokio::task::spawn_blocking(move || {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        fs2::FileExt::lock_exclusive(&file)?;
        Ok(file)
    })
    .await
    .map_err(io::Error::other)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn filesystem_only_home_does_not_create_a_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unused/state.lock");
        assert!(acquire_if_active(path.clone(), false)
            .await
            .unwrap()
            .is_none());
        assert!(!path.exists());
        drop(acquire_if_active(path.clone(), true).await.unwrap());
        assert!(acquire_if_active(path, false).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn separate_handles_serialize_access() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.lock");
        let first = acquire(path.clone()).await.unwrap();
        let mut second = tokio::spawn(acquire(path));
        assert!(tokio::time::timeout(Duration::from_millis(50), &mut second)
            .await
            .is_err());
        drop(first);
        assert!(tokio::time::timeout(Duration::from_secs(2), second)
            .await
            .unwrap()
            .unwrap()
            .is_ok());
    }

    #[tokio::test]
    async fn child_process_acquires_lock() {
        let Some(dir) = std::env::var_os("HBOTS_LOCK_TEST_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        std::fs::write(dir.join("ready"), b"ready").unwrap();
        let _guard = acquire(dir.join("state.lock")).await.unwrap();
        std::fs::write(dir.join("acquired"), b"acquired").unwrap();
    }

    #[tokio::test]
    async fn separate_processes_serialize_access() {
        let dir = tempfile::tempdir().unwrap();
        let first = acquire(dir.path().join("state.lock")).await.unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::local_lock::tests::child_process_acquires_lock",
            ])
            .env("HBOTS_LOCK_TEST_DIR", dir.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !dir.path().join("ready").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!dir.path().join("acquired").exists());
        drop(first);
        let status = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            let _ = child.kill();
            panic!("child did not acquire released lock")
        });
        assert!(status.success());
        assert!(dir.path().join("acquired").exists());
    }
}
