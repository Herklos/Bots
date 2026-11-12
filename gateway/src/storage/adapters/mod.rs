//! Backend implementations. The domain code in `storage/` never names a backend.
pub(crate) mod anytype;
pub(crate) mod filesystem;
#[cfg(test)]
pub(crate) mod memory;
