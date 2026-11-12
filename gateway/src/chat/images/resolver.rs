//! A bounded cache of library image checks.
//!
//! A hit returns the stored result and does not touch the workspace, so the
//! 500 ms chat poll does no file IO. A failure on a message younger than 30
//! seconds is kept for 5 seconds, so a file written just after the reply
//! appears on the next look. After that the failure is kept for the same
//! 10 minutes as a success.
use super::{image_id, kind_of, picture, read_regular, ImageFault, Kind, MAX_IMAGE_BYTES};
use crate::storage::workspace::Workspace;
use std::collections::HashMap;
use std::sync::Mutex;

const SUCCESS_MS: u64 = 10 * 60 * 1000;
const RETRY_MS: u64 = 5_000;
const FINAL_AFTER_MS: u64 = 30_000;
const CACHE_CAP: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImageFile {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) mime: &'static str,
    pub(crate) bytes: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) etag: String,
    pub(crate) id: String,
}

#[derive(Debug, Clone)]
struct Slot {
    at: u64,
    ttl: u64,
    body: Result<ImageFile, ImageFault>,
}

#[derive(Debug, Default)]
struct Cache {
    slots: HashMap<(String, String), Slot>,
}

#[derive(Debug, Default)]
pub(crate) struct Resolver {
    cache: Mutex<Cache>,
}

impl Resolver {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn resolve(
        &self,
        ws: &dyn Workspace,
        slug: &str,
        path: &str,
        message_ms: Option<u64>,
        now_ms: u64,
    ) -> Result<ImageFile, ImageFault> {
        let key = (slug.to_string(), path.to_string());
        if let Some(hit) = self.fresh(&key, now_ms) {
            return hit;
        }
        let loaded = inspect(ws, slug, path).await;
        let ttl = ttl_of(&loaded, message_ms, now_ms);
        self.store(
            key,
            Slot {
                at: now_ms,
                ttl,
                body: loaded.clone(),
            },
        );
        loaded
    }

    fn fresh(&self, key: &(String, String), now_ms: u64) -> Option<Result<ImageFile, ImageFault>> {
        let cache = self.cache.lock().unwrap_or_else(|err| err.into_inner());
        let slot = cache.slots.get(key)?;
        (now_ms.saturating_sub(slot.at) < slot.ttl).then(|| slot.body.clone())
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.cache
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .slots
            .len()
    }

    fn store(&self, key: (String, String), slot: Slot) {
        let mut cache = self.cache.lock().unwrap_or_else(|err| err.into_inner());
        if cache.slots.len() >= CACHE_CAP && !cache.slots.contains_key(&key) {
            let old = cache
                .slots
                .iter()
                .min_by_key(|(_, slot)| slot.at)
                .map(|(key, _)| key.clone());
            if let Some(old) = old {
                cache.slots.remove(&old);
            }
        }
        cache.slots.insert(key, slot);
    }
}

fn ttl_of(loaded: &Result<ImageFile, ImageFault>, message_ms: Option<u64>, now_ms: u64) -> u64 {
    match loaded {
        Ok(_) => SUCCESS_MS,
        Err(ImageFault::Io) => RETRY_MS,
        Err(_) => {
            let settled = message_ms.is_some_and(|at| now_ms.saturating_sub(at) > FINAL_AFTER_MS);
            if settled {
                SUCCESS_MS
            } else {
                RETRY_MS
            }
        }
    }
}

async fn inspect(ws: &dyn Workspace, slug: &str, path: &str) -> Result<ImageFile, ImageFault> {
    if !crate::bots::valid_slug(slug) || super::library_rel(path).as_deref() != Some(path) {
        return Err(ImageFault::BadPath);
    }
    let Some(kind) = kind_of(path) else {
        return Err(ImageFault::NotImage);
    };
    let bytes = read_regular(ws, &format!("{slug}/{path}")).await?;
    checked(path, kind, &bytes)
}

fn checked(path: &str, kind: Kind, bytes: &[u8]) -> Result<ImageFile, ImageFault> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ImageFault::TooBig);
    }
    let picture = picture(kind, bytes).ok_or(ImageFault::NotImage)?;
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    Ok(ImageFile {
        id: image_id(path),
        name,
        mime: picture.mime,
        bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        width: picture.width,
        height: picture.height,
        etag: super::etag_of(bytes),
        path: path.to_string(),
    })
}
