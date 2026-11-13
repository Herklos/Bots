use super::p4::{P4_COLUMNS, P4_TABLES};
use super::table::{COLUMNS, TABLES};
use std::collections::HashSet;

#[test]
fn p4_catalog_is_additive_and_every_property_is_declared() {
    let mut columns = HashSet::new();
    for column in COLUMNS.iter().chain(P4_COLUMNS) {
        assert!(
            columns.insert(column.key),
            "duplicate property {}",
            column.key
        );
        assert!(column.key.starts_with("m_") && !column.key.chars().any(|c| c.is_ascii_digit()));
        assert!(matches!(
            column.format,
            "text"
                | "number"
                | "select"
                | "multi_select"
                | "date"
                | "files"
                | "checkbox"
                | "url"
                | "objects"
        ));
        if column.format == "select" {
            assert!(!column.options.is_empty(), "{} has no options", column.key);
        }
    }
    let mut types = HashSet::new();
    for table in TABLES.iter().chain(P4_TABLES) {
        assert!(types.insert(table.key), "duplicate type {}", table.key);
        assert!(table.key.starts_with("m_") && !table.key.chars().any(|c| c.is_ascii_digit()));
        assert!(super::table::properties(table).count() <= 128);
        let mut properties = HashSet::new();
        for property in table.properties {
            assert!(
                columns.contains(property),
                "{} refers to undeclared {property}",
                table.key
            );
            assert!(
                properties.insert(property),
                "{} repeats {property}",
                table.key
            );
        }
    }
}

#[test]
fn p4_has_all_required_types_and_keeps_legacy_formats() {
    for key in [
        "m_bot",
        "m_settings",
        "m_model_profile",
        "m_connector",
        "m_schedule",
        "m_group",
        "m_msg_segment",
        "m_attachment",
        "m_project",
        "m_note",
        "m_task",
        "m_plan",
        "m_link",
        "m_repo",
        "m_machine",
        "m_service",
    ] {
        assert!(
            P4_TABLES.iter().any(|table| table.key == key),
            "missing {key}"
        );
    }
    for key in ["m_owner_bot", "m_status"] {
        assert_eq!(
            COLUMNS
                .iter()
                .find(|column| column.key == key)
                .unwrap()
                .format,
            "text"
        );
    }
    assert_eq!(
        P4_COLUMNS
            .iter()
            .find(|c| c.key == "m_owner_bot_ref")
            .unwrap()
            .format,
        "objects"
    );
    assert_eq!(
        P4_COLUMNS
            .iter()
            .find(|c| c.key == "m_record_status")
            .unwrap()
            .format,
        "select"
    );
    for key in [
        "m_project",
        "m_note",
        "m_task",
        "m_plan",
        "m_link",
        "m_repo",
        "m_machine",
        "m_service",
    ] {
        let table = P4_TABLES.iter().find(|table| table.key == key).unwrap();
        assert!(table.properties.contains(&"m_owner_bot_ref"));
        assert!(table.properties.contains(&"m_record_status"));
    }
}
