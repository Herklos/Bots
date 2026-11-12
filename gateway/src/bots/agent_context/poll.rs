//! Every 20 seconds, restore a tampered user-memory file, then refresh Bots whose files moved.
use super::{changed, inherit_parent, stamp_file, BotFiles, FileWatch, POLL_EVERY};
use crate::bots::valid_slug;

pub(crate) fn start(state: std::sync::Arc<crate::AppState>) {
    tokio::spawn(async move {
        let mut prev = scan(&state).await;
        loop {
            tokio::time::sleep(POLL_EVERY).await;
            let now = scan(&state).await;
            let dirty = changed(&prev, &now);
            prev = now;
            for slug in dirty {
                if let Err(e) = state.bots.refresh_agents(&slug).await {
                    tracing::warn!("refreshing memory for {slug}: {}", e.message);
                }
            }
        }
    });
}

async fn scan(state: &crate::AppState) -> FileWatch {
    let bots = state.bots.list().await.unwrap_or_else(|e| {
        tracing::warn!("listing Bots for the memory poller: {}", e.message);
        Vec::new()
    });
    let mut watch = FileWatch {
        shared: stamp_file(&state.cfg.bots_root.join(".botapp").join("shared-memory.md")).await,
        bots: Vec::with_capacity(bots.len()),
    };
    for bot in bots {
        let Some(slug) = bot["slug"].as_str().filter(|slug| valid_slug(slug)) else {
            continue;
        };
        if let Err(err) = crate::bots::memory::verify_user_facts(
            &state.cfg.bots_root,
            slug,
            &crate::vault::now_iso(),
        )
        .await
        {
            tracing::warn!("user memory verify for {slug}: {err}");
        }
        let dir = state.cfg.bots_root.join(slug);
        watch.bots.push(BotFiles {
            slug: slug.to_string(),
            user: stamp_file(&crate::bots::memory::user_memory_path(
                &state.cfg.bots_root,
                slug,
            ))
            .await,
            memory: stamp_file(&dir.join("memory").join("MEMORY.md")).await,
            log: stamp_file(&dir.join("memory").join("log.md")).await,
            inherit_parent: inherit_parent(&bot),
        });
    }
    watch
}
