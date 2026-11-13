//! The four types v1 stores in Anytype. Keys have no digits: Anytype splits a digit
//! off the key (`hbprobe2` came back as `hbprobe_2`) and the key is then permanent.
//!
//! Create uses `{name, api_key, layout}` and `{name, key, format}` (measured, spike S2).
//! The spike did not record a body for select options, so `m_conflict_state` is
//! created as `select` and its options stay here for writers. They are not sent on create.

#[derive(Clone, Copy)]
pub(crate) struct Column {
    pub key: &'static str,
    pub name: &'static str,
    pub format: &'static str,
    pub options: &'static [&'static str],
}

#[derive(Clone, Copy)]
pub(crate) struct Table {
    pub key: &'static str,
    pub name: &'static str,
    pub layout: &'static str,
    pub properties: &'static [&'static str],
}

const OPEN: &[&str] = &["open", "resolved", "dismissed"];

pub(crate) const COLUMNS: &[Column] = &[
    plain("m_path", "text"),
    plain("m_dir", "text"),
    plain("m_content", "text"),
    plain("m_sha", "text"),
    plain("m_size", "number"),
    plain("m_mime", "text"),
    plain("m_mtime", "number"),
    plain("m_deleted", "checkbox"),
    plain("m_owner_bot", "text"),
    plain("m_device", "text"),
    plain("m_rev", "number"),
    plain("m_blob", "files"),
    plain("m_schema_version", "number"),
    plain("m_min_gateway", "text"),
    plain("m_keys_json", "text"),
    Column {
        key: "m_conflict_state",
        name: "m_conflict_state",
        format: "select",
        options: OPEN,
    },
    plain("m_device_a", "text"),
    plain("m_device_b", "text"),
    plain("m_sha_a", "text"),
    plain("m_sha_b", "text"),
    plain("m_detected_at", "date"),
    plain("m_copy_a", "files"),
    plain("m_copy_b", "files"),
    plain("m_slug", "text"),
    plain("m_hostname", "text"),
    plain("m_gateway_version", "text"),
    plain("m_status", "text"),
    plain("m_last_seen", "number"),
];

const META: &[&str] = &["m_schema_version", "m_min_gateway", "m_keys_json"];
const FILE: &[&str] = &[
    "m_path",
    "m_dir",
    "m_content",
    "m_blob",
    "m_sha",
    "m_size",
    "m_mime",
    "m_mtime",
    "m_deleted",
    "m_owner_bot",
    "m_device",
    "m_rev",
];
const CONFLICT: &[&str] = &[
    "m_path",
    "m_owner_bot",
    "m_conflict_state",
    "m_device_a",
    "m_device_b",
    "m_sha_a",
    "m_sha_b",
    "m_copy_a",
    "m_copy_b",
    "m_detected_at",
];
const DEVICE: &[&str] = &[
    "m_slug",
    "m_hostname",
    "m_gateway_version",
    "m_status",
    "m_last_seen",
];

pub(crate) const TABLES: &[Table] = &[
    Table {
        key: "m_meta",
        name: "m_meta",
        layout: "basic",
        properties: META,
    },
    Table {
        key: "m_file",
        name: "m_file",
        layout: "basic",
        properties: FILE,
    },
    Table {
        key: "m_conflict",
        name: "m_conflict",
        layout: "basic",
        properties: CONFLICT,
    },
    Table {
        key: "m_device",
        name: "m_device",
        layout: "basic",
        properties: DEVICE,
    },
];

const fn plain(key: &'static str, format: &'static str) -> Column {
    Column {
        key,
        name: key,
        format,
        options: &[],
    }
}

pub(crate) fn column(key: &str) -> Option<&'static Column> {
    columns().find(|column| column.key == key)
}

pub(crate) fn table(key: &str) -> Option<&'static Table> {
    tables().find(|table| table.key == key)
}

pub(crate) use super::p5::extensions;

pub(crate) fn columns() -> impl Iterator<Item = &'static Column> {
    COLUMNS.iter().chain(super::p4::P4_COLUMNS).chain(super::p5::P5_COLUMNS)
}

pub(crate) fn tables() -> impl Iterator<Item = &'static Table> {
    TABLES.iter().chain(super::p4::P4_TABLES).chain(super::p5::P5_TABLES)
}

/// Anytype's built-in properties must be attached explicitly to be queryable.
pub(crate) const BUILTINS: &[&str] = &[
    "backlinks",
    "links",
    "tag",
    "created_date",
    "creator",
    "last_modified_date",
    "last_modified_by",
];

pub(crate) fn properties(table: &'static Table) -> impl Iterator<Item = &'static str> {
    let bundled = if is_generated(table.key) { BUILTINS } else { &[] };
    table
        .properties
        .iter()
        .copied()
        .chain(super::p5::extensions(table.key).iter().copied())
        .chain(bundled.iter().copied())
}

/// Any type Prepare creates or maintains (everything past the Tier A four)
/// carries the bundled queryable properties.
fn is_generated(key: &str) -> bool {
    super::p4::P4_TABLES.iter().any(|p4| p4.key == key)
        || super::p5::P5_TABLES.iter().any(|p5| p5.key == key)
}

/// A select value this gateway is allowed to send. Unknown options are refused.
pub(crate) fn option_allowed(property: &str, option: &str) -> bool {
    column(property)
        .map(|column| column.options.contains(&option))
        .unwrap_or(false)
}

pub(crate) fn type_map_key(key: &str) -> String {
    format!("type:{key}")
}
