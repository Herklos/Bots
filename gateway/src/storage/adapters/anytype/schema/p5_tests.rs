use super::p4::{P4_COLUMNS, P4_TABLES};
use super::p5::{EXTENSIONS, P5_COLUMNS, P5_TABLES, extensions};
use super::table::{COLUMNS, TABLES};
use std::collections::HashSet;

#[test]
fn p5_catalog_is_additive_and_every_property_is_declared() {
    let mut columns = HashSet::new();
    for column in COLUMNS.iter().chain(P4_COLUMNS).chain(P5_COLUMNS) {
        assert!(
            columns.insert(column.key),
            "duplicate property {}",
            column.key
        );
    }
    let mut types = HashSet::new();
    for table in TABLES.iter().chain(P4_TABLES).chain(P5_TABLES) {
        assert!(types.insert(table.key), "duplicate type {}", table.key);
        assert!(table.key.starts_with("m_") && !table.key.chars().any(|c| c.is_ascii_digit()));
        assert!(
            super::table::properties(table).count() <= 128,
            "{} exceeds the 128-property cap",
            table.key
        );
        for property in super::table::properties(table) {
            if property.starts_with("m_") {
                assert!(columns.contains(property), "{} refers to undeclared {property}", table.key);
            }
        }
    }
}

#[test]
fn p5_has_all_required_types() {
    for key in [
        "m_prompt", "m_subagent", "m_memory", "m_skill", "m_harness", "m_log",
        "m_suggestion", "m_epic", "m_user_story",
    ] {
        assert!(P5_TABLES.iter().any(|table| table.key == key), "missing {key}");
    }
    for key in ["m_epic", "m_user_story"] {
        let table = P5_TABLES.iter().find(|table| table.key == key).unwrap();
        assert!(table.properties.contains(&"m_owner_bot_ref"));
        assert!(table.properties.contains(&"m_record_status"));
        assert!(table.properties.contains(&"m_pm_status"));
        assert_eq!(table.layout, "todo");
    }
}

#[test]
fn p5_extensions_extend_declared_types_with_declared_columns() {
    for (type_key, properties) in EXTENSIONS {
        assert!(
            P4_TABLES.iter().any(|table| table.key == *type_key)
                || P5_TABLES.iter().any(|table| table.key == *type_key),
            "extension targets unknown type {type_key}"
        );
        for property in *properties {
            assert!(
                columns_has(property),
                "extension on {type_key} uses undeclared {property}"
            );
        }
    }
    assert_eq!(extensions("m_bot").len(), 16);
    assert_eq!(extensions("m_group").len(), 8);
    assert!(extensions("m_file").is_empty());
}

fn columns_has(property: &str) -> bool {
    COLUMNS
        .iter()
        .chain(P4_COLUMNS)
        .chain(P5_COLUMNS)
        .any(|column| column.key == property)
}
