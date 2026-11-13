//! Bot profile persistence. Shared profile fields and machine bindings both pass
//! through here: a remote adapter projects the shared fields and keeps bindings in
//! a device-local overlay. The slug rule travels with the store so adapters accept
//! exactly the names the gateway accepts.
use crate::platform::error::ApiResult;
use async_trait::async_trait;
use serde_json::Value;

/// `[a-z0-9][a-z0-9-]{0,62}`: the only names under the Bots root.
pub(crate) fn valid_slug(slug: &str) -> bool {
    let b = slug.as_bytes();
    !b.is_empty()
        && b.len() <= 63
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

#[async_trait]
pub(crate) trait BotStore: std::fmt::Debug + Send + Sync {
    /// Hold while a read-modify-write of bot data spans several store calls.
    async fn acquire(&self) -> std::io::Result<Option<std::fs::File>>;
    /// Valid bot directory names under the root, in no particular order.
    async fn list_slugs(&self) -> ApiResult<Vec<String>>;
    /// Read one Bot's persisted profile. `None` when it has none, or the profile
    /// is not readable JSON on this backend.
    async fn read_meta(&self, slug: &str) -> ApiResult<Option<Value>>;
    async fn write_meta(&self, slug: &str, meta: &Value) -> ApiResult<()>;
    /// The saved main-Bot choice. Missing is no choice; unreadable is an error
    /// so it cannot be silently overwritten.
    async fn read_selection(&self) -> ApiResult<Option<Value>>;
    /// A slug of `None` clears the choice (the reconciler stores exactly that).
    async fn write_selection(&self, slug: Option<&str>) -> ApiResult<()>;
}
