//! Markdown/AnyBlock conversion for page-backed sync, independent of transport.
//!
//! Unknown or styled blocks are represented by opaque sidecar markers, rather
//! than flattened. Callers must persist the sidecar with the rendered text and
//! must not interpret opaque read blocks as valid `insert_blocks` payloads.
//! No page writer uses this module yet.
#![allow(dead_code)]

mod parse;
mod render;
#[cfg(test)]
mod tests;

use serde_json::Value;
use std::collections::BTreeMap;

/// An opaque block must survive verbatim in JSON representation, including IDs
/// and fields not understood by this gateway.
pub(crate) type Sidecar = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rendered {
    pub markdown: String,
    pub sidecar: Sidecar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodecError {
    FrontMatter,
    UnclosedFence,
    InvalidMarker,
    MissingOpaqueBlock,
    DuplicateOpaqueBlock,
    RemovedOpaqueBlock,
    UnsupportedMarkdown(&'static str),
}

#[allow(unused_imports, reason = "page writers are not connected yet")]
pub(crate) use parse::parse;
#[allow(unused_imports, reason = "page writers are not connected yet")]
pub(crate) use render::render;

const MARKER_PREFIX: &str = "<!-- hbots:anyblock:";
const MARKER_SUFFIX: &str = " -->";
