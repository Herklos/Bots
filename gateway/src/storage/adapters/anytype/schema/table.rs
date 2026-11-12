//! The four types v1 stores in Anytype. Keys have no digits: Anytype splits a digit
//! off the key (`hbprobe2` came back as `hbprobe_2`) and the key is then permanent.
//!
//! Create uses `{name, key, layout}` and `{name, key, format}`. The spike did not
//! record a body for select options, so `hb_conflict_state` is created as `select`
//! and its options stay here for writers. They are not sent on create.

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
    plain("hb_path", "text"),
    plain("hb_dir", "text"),
    plain("hb_content", "text"),
    plain("hb_sha", "text"),
    plain("hb_size", "number"),
    plain("hb_mime", "text"),
    plain("hb_mtime", "number"),
    plain("hb_deleted", "checkbox"),
    plain("hb_owner_bot", "text"),
    plain("hb_device", "text"),
    plain("hb_rev", "number"),
    plain("hb_blob", "files"),
    plain("hb_schema_version", "number"),
    plain("hb_min_gateway", "text"),
    plain("hb_keys_json", "text"),
    Column {
        key: "hb_conflict_state",
        name: "hb_conflict_state",
        format: "select",
        options: OPEN,
    },
    plain("hb_device_a", "text"),
    plain("hb_device_b", "text"),
    plain("hb_sha_a", "text"),
    plain("hb_sha_b", "text"),
    plain("hb_detected_at", "date"),
    plain("hb_copy_a", "files"),
    plain("hb_copy_b", "files"),
    plain("hb_slug", "text"),
    plain("hb_hostname", "text"),
    plain("hb_gateway_version", "text"),
    plain("hb_status", "text"),
    plain("hb_last_seen", "number"),
];

const META: &[&str] = &["hb_schema_version", "hb_min_gateway", "hb_keys_json"];
const FILE: &[&str] = &[
    "hb_path",
    "hb_dir",
    "hb_content",
    "hb_blob",
    "hb_sha",
    "hb_size",
    "hb_mime",
    "hb_mtime",
    "hb_deleted",
    "hb_owner_bot",
    "hb_device",
    "hb_rev",
];
const CONFLICT: &[&str] = &[
    "hb_path",
    "hb_owner_bot",
    "hb_conflict_state",
    "hb_device_a",
    "hb_device_b",
    "hb_sha_a",
    "hb_sha_b",
    "hb_copy_a",
    "hb_copy_b",
    "hb_detected_at",
];
const DEVICE: &[&str] = &[
    "hb_slug",
    "hb_hostname",
    "hb_gateway_version",
    "hb_status",
    "hb_last_seen",
];

pub(crate) const TABLES: &[Table] = &[
    Table {
        key: "hb_meta",
        name: "hb_meta",
        layout: "basic",
        properties: META,
    },
    Table {
        key: "hb_file",
        name: "hb_file",
        layout: "basic",
        properties: FILE,
    },
    Table {
        key: "hb_conflict",
        name: "hb_conflict",
        layout: "basic",
        properties: CONFLICT,
    },
    Table {
        key: "hb_device",
        name: "hb_device",
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
    COLUMNS.iter().find(|column| column.key == key)
}

pub(crate) fn table(key: &str) -> Option<&'static Table> {
    TABLES.iter().find(|table| table.key == key)
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
