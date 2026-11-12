//! Create missing types and properties, then the one `hb_meta` object.
//! Nothing is renamed or deleted. Keys come back from the create response and
//! any key that differs from the one we asked for is stored in `hb_keys_json`.

use super::table::{self, Column, Table, COLUMNS, TABLES};
use crate::storage::adapters::anytype::client::{AnytypeClient, AnytypeError};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Report {
    pub dry_run: bool,
    pub types: Vec<String>,
    pub properties: Vec<String>,
    pub meta: bool,
    pub schema_ready: bool,
    pub keys: BTreeMap<String, String>,
}

pub(crate) async fn bootstrap(
    client: &AnytypeClient,
    dry_run: bool,
) -> Result<Report, AnytypeError> {
    assert_tables();
    let types = client.list_types().await?;
    let properties = client.list_properties().await?;
    let existing = meta_object(client, &types, &BTreeMap::new()).await?;
    let mut keys = existing.as_ref().map(read_keys).unwrap_or_default();
    let mut create_types = Vec::new();
    let mut create_properties = Vec::new();
    let mut attach = Vec::new();

    for table in TABLES {
        match remember(&types, table.key, &type_label(table), &mut keys) {
            Some(ref type_key) if missing_definitions(client, type_key, table).await? => {
                attach.push((*table, type_key.clone()));
            }
            Some(_) => {}
            None => create_types.push(table.key.to_string()),
        }
    }
    for column in COLUMNS {
        if remember(&properties, column.key, column.key, &mut keys).is_none() {
            create_properties.push(column.key.to_string());
        }
    }

    let current = existing.as_ref().map(|object| read_meta(object, &keys));
    let drift = drifts(&keys);
    let ready_now = create_types.is_empty()
        && create_properties.is_empty()
        && attach.is_empty()
        && current.as_ref().and_then(|meta| meta.version) == Some(SCHEMA_VERSION)
        && current
            .as_ref()
            .and_then(|meta| meta.min_gateway.as_deref())
            .is_some()
        && current.as_ref().map(|meta| &meta.keys) == Some(&drift);
    if dry_run {
        let mut properties = create_properties;
        properties.extend(
            attach
                .iter()
                .flat_map(|(table, _)| table.properties.iter().map(|key| key.to_string())),
        );
        return Ok(Report {
            dry_run: true,
            types: create_types,
            properties,
            meta: !ready_now,
            schema_ready: ready_now,
            keys: drift,
        });
    }

    for key in &create_types {
        let table = table::table(key).expect("declared type");
        let created = client
            .create_type(&type_body(table), &format!("hbots-type-{key}"))
            .await?;
        keys.insert(type_label(table), read_back(&created, key));
    }
    for key in &create_properties {
        let column = table::column(key).expect("declared property");
        let created = client
            .create_property(&property_body(column), &format!("hbots-property-{key}"))
            .await?;
        keys.insert((*key).to_string(), read_back(&created, key));
    }
    for (table, type_key) in &attach {
        let body = json!({ "property_definitions": property_definitions(table) });
        client
            .patch_type(type_key, &body, &format!("hbots-type-defs-{}", table.key))
            .await?;
    }

    let drifts = drifts(&keys);
    let wrote = write_meta(client, &keys, &drifts, existing.as_ref()).await?;
    Ok(Report {
        dry_run: false,
        types: create_types,
        properties: create_properties,
        meta: wrote,
        schema_ready: true,
        keys: drifts,
    })
}

pub(crate) async fn schema_ready(client: &AnytypeClient) -> Result<bool, AnytypeError> {
    Ok(bootstrap(client, true).await?.schema_ready)
}

/// Known fields only. A property this gateway does not name is ignored.
pub(crate) fn read_meta(object: &Value, keys: &BTreeMap<String, String>) -> Meta {
    Meta {
        version: version(object, keys),
        min_gateway: text_prop(object, keys, "hb_min_gateway"),
        keys: read_keys(object),
    }
}

pub(crate) struct Meta {
    pub version: Option<i64>,
    pub min_gateway: Option<String>,
    pub keys: BTreeMap<String, String>,
}

fn type_label(table: &Table) -> String {
    table::type_map_key(table.key)
}

fn type_body(table: &Table) -> Value {
    json!({
        "name": table.name,
        "api_key": table.key,
        "layout": table.layout,
        "property_definitions": property_definitions(table),
    })
}

fn property_definitions(table: &Table) -> Vec<Value> {
    table
        .properties
        .iter()
        .map(|key| {
            let column = table::column(key).expect("declared property");
            match column.options.is_empty() {
                true => json!({ "name": column.name, "format": column.format }),
                false => json!({
                    "name": column.name,
                    "format": column.format,
                    "options": column
                        .options
                        .iter()
                        .map(|option| json!({ "name": option }))
                        .collect::<Vec<_>>(),
                }),
            }
        })
        .collect()
}

/// True when the type exists but does not name every column hbots searches with.
/// Anytype rejects a `fields` projection that the type has not declared.
async fn missing_definitions(
    client: &AnytypeClient,
    type_key: &str,
    table: &Table,
) -> Result<bool, AnytypeError> {
    let value = client.get_type(type_key).await?;
    let names: Vec<&str> = value["blocks"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|block| block["properties"].as_array().into_iter().flatten())
        .filter_map(|property| property["property"].as_str())
        .collect();
    Ok(table.properties.iter().any(|key| {
        let name = table::column(key).map(|column| column.name).unwrap_or(key);
        !names.contains(&name)
    }))
}

fn property_body(column: &Column) -> Value {
    assert!(
        column.format != "select"
            || column
                .options
                .iter()
                .all(|option| table::option_allowed(column.key, option)),
        "{}",
        column.key
    );
    json!({ "name": column.name, "key": column.key, "format": column.format })
}

fn remember(
    rows: &[Value],
    canonical: &str,
    map_key: &str,
    keys: &mut BTreeMap<String, String>,
) -> Option<String> {
    let mapped = keys.get(map_key).cloned();
    let found = find_row(rows, canonical, mapped.as_deref())?;
    let actual = found.to_string();
    if actual != canonical {
        keys.insert(map_key.to_string(), actual.clone());
    } else {
        keys.remove(map_key);
    }
    Some(actual)
}

fn find_row<'a>(rows: &'a [Value], canonical: &str, mapped: Option<&str>) -> Option<&'a str> {
    rows.iter().find_map(|row| {
        let key = row["key"].as_str().unwrap_or("");
        let name = row["name"].as_str().unwrap_or("");
        if key == canonical || mapped == Some(key) || name == canonical {
            Some(key)
        } else {
            None
        }
    })
}

fn read_back(created: &Value, fallback: &str) -> String {
    created["key"]
        .as_str()
        .or_else(|| created["data"]["key"].as_str())
        .filter(|key| !key.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn drifts(keys: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    keys.iter()
        .filter(|(requested, actual)| {
            requested.strip_prefix("type:") != Some(actual.as_str())
                && requested.as_str() != actual.as_str()
        })
        .map(|(requested, actual)| (requested.clone(), actual.clone()))
        .collect()
}

fn assert_tables() {
    for table in TABLES {
        for key in table.properties {
            assert!(table::column(key).is_some(), "{key} is not a column");
        }
    }
}

fn actual<'a>(keys: &'a BTreeMap<String, String>, canonical: &'a str) -> &'a str {
    keys.get(canonical).map(String::as_str).unwrap_or(canonical)
}

async fn meta_object(
    client: &AnytypeClient,
    types: &[Value],
    keys: &BTreeMap<String, String>,
) -> Result<Option<Value>, AnytypeError> {
    // A brand-new space does not know the `hb_meta` type yet, and Anytype
    // refuses searches that filter by an unknown type key. Filter by name
    // locally instead when the type is not in the space's type list.
    let type_key = find_row(
        types,
        "hb_meta",
        keys.get(&table::type_map_key("hb_meta"))
            .map(String::as_str),
    );
    let mut body = json!({
        "query": "hb_meta",
        "fields": [
            "name",
            "hb_schema_version",
            "hb_min_gateway",
            "hb_keys_json",
            "created_date",
        ],
    });
    if let Some(key) = type_key {
        body["type"] = json!(key);
    }
    let mut rows = client.search(&body).await?;
    rows.retain(|row| row["name"].as_str() == Some("hb_meta") && row["archived"] != true);
    rows.sort_by(|left, right| {
        left["created_date"]
            .as_str()
            .unwrap_or("")
            .cmp(right["created_date"].as_str().unwrap_or(""))
    });
    Ok(rows.into_iter().next())
}

async fn write_meta(
    client: &AnytypeClient,
    keys: &BTreeMap<String, String>,
    drifts: &BTreeMap<String, String>,
    existing: Option<&Value>,
) -> Result<bool, AnytypeError> {
    let properties = json!({
        actual(keys, "hb_schema_version"): SCHEMA_VERSION,
        actual(keys, "hb_min_gateway"): env!("CARGO_PKG_VERSION"),
        actual(keys, "hb_keys_json"): serde_json::to_string(drifts).unwrap_or_else(|_| "{}".into()),
    });
    let Some(object) = existing else {
        let body = json!({
            "type": meta_type(keys),
            "name": "hb_meta",
            "properties": properties,
        });
        client.create_object(&body, &idem(&body)).await?;
        return Ok(true);
    };
    let current = read_meta(object, keys);
    if current.version == Some(SCHEMA_VERSION)
        && current.min_gateway.is_some()
        && current.keys == *drifts
    {
        return Ok(false);
    }
    let id = object["id"].as_str().unwrap_or("");
    let fetched = client.get_object(id).await?;
    let etag = fetched["etag"].as_str().unwrap_or("");
    let body = json!({ "ops": [{ "op": "set_properties", "set": properties }] });
    client
        .patch_object(id, &body, Some(etag), &idem(&body))
        .await?;
    Ok(true)
}

fn meta_type(keys: &BTreeMap<String, String>) -> &str {
    keys.get(&table::type_map_key("hb_meta"))
        .map(String::as_str)
        .unwrap_or("hb_meta")
}

fn idem(body: &Value) -> String {
    let raw = serde_json::to_vec(body).unwrap_or_default();
    format!("hbots-meta-{}", hex::encode(Sha256::digest(raw)))
}

fn version(object: &Value, keys: &BTreeMap<String, String>) -> Option<i64> {
    match prop(object, keys, "hb_schema_version") {
        Some(Value::Number(number)) => number.as_i64(),
        Some(Value::String(text)) => text.parse().ok(),
        _ => None,
    }
}

fn text_prop(object: &Value, keys: &BTreeMap<String, String>, canonical: &str) -> Option<String> {
    prop(object, keys, canonical).and_then(|value| value.as_str().map(str::to_string))
}

fn prop<'a>(
    object: &'a Value,
    keys: &BTreeMap<String, String>,
    canonical: &str,
) -> Option<&'a Value> {
    object["properties"].get(actual(keys, canonical))
}

fn read_keys(object: &Value) -> BTreeMap<String, String> {
    let Some(properties) = object["properties"].as_object() else {
        return BTreeMap::new();
    };
    if let Some(map) = properties.get("hb_keys_json").and_then(parse_map) {
        return map;
    }
    properties.values().find_map(parse_map).unwrap_or_default()
}

fn parse_map(value: &Value) -> Option<BTreeMap<String, String>> {
    let text = value.as_str()?;
    let parsed: Map<String, Value> = serde_json::from_str(text).ok()?;
    if !parsed.contains_key("hb_sha")
        && !parsed.contains_key("hb_schema_version")
        && !parsed.contains_key(&table::type_map_key("hb_meta"))
        && !parsed
            .keys()
            .any(|key| key.starts_with("hb_") || key.starts_with("type:"))
    {
        return None;
    }
    Some(
        parsed
            .into_iter()
            .filter_map(|(key, value)| value.as_str().map(|actual| (key, actual.to_string())))
            .collect(),
    )
}
