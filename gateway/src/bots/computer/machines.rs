//! This computer and the computers T3 Code already linked.
//!
//! The list is read only. Pairing stays in T3 Code. `forwarding` stays false
//! because this build does not send `environmentId`.

use crate::harness::{without_urls, ComputerCatalog};
use serde_json::{json, Value};

/// The public list. A down catalog is still a body, with `ok: false`.
pub(crate) fn machines_view(catalog: &ComputerCatalog) -> Value {
    if !catalog.ok {
        return json!({
            "ok": false,
            "forwarding": false,
            "machines": [],
            "error": "environment_offline",
        });
    }
    let mut machines = vec![shown(Shown {
        id: "",
        label: &catalog.local_label,
        local: true,
        status: "online",
        laptop: catalog.local_laptop,
        seen: "",
        version: &catalog.local_version,
        error: "",
        access: "",
    })];
    for link in &catalog.links {
        machines.push(shown(Shown {
            id: &link.id,
            label: &link.label,
            local: false,
            status: machine_status(&link.status, link.laptop),
            laptop: link.laptop,
            seen: &link.last_seen_at,
            version: &link.version,
            error: &link.last_error,
            access: &link.access,
        }));
    }
    json!({
        "ok": true,
        "forwarding": false,
        "machines": machines,
        "error": "",
    })
}

struct Shown<'a> {
    id: &'a str,
    label: &'a str,
    local: bool,
    status: &'a str,
    laptop: bool,
    seen: &'a str,
    version: &'a str,
    error: &'a str,
    access: &'a str,
}

fn shown(row: Shown<'_>) -> Value {
    json!({
        "id": row.id,
        "label": without_urls(row.label),
        "local": row.local,
        "status": row.status,
        "laptop": row.laptop,
        "lastSeenAt": without_urls(row.seen),
        "t3Version": without_urls(row.version),
        "lastError": without_urls(row.error),
        "access": row.access,
    })
}

/// Reachable is online. Expired stays expired. Any other laptop is asleep.
pub(crate) fn machine_status(status: &str, laptop: bool) -> &'static str {
    match status {
        "reachable" | "online" => "online",
        "expired" => "expired",
        "asleep" => "asleep",
        _ if laptop => "asleep",
        _ => "unreachable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::ComputerLink;

    fn link(id: &str, status: &str, laptop: bool) -> ComputerLink {
        ComputerLink {
            id: id.into(),
            label: id.into(),
            status: status.into(),
            access: "approval-required".into(),
            selectable: status == "reachable",
            last_seen_at: String::new(),
            last_error: String::new(),
            laptop,
            version: String::new(),
        }
    }

    fn catalog(links: Vec<ComputerLink>) -> ComputerCatalog {
        ComputerCatalog {
            ok: true,
            local_label: "Studio".into(),
            local_version: "0.1.9".into(),
            local_laptop: false,
            broker_id: "15f077d9-b158-435e-96f1-ed7709f11cbd".into(),
            links,
        }
    }

    #[test]
    fn the_list_names_this_computer_and_maps_link_status() {
        let mut sleep = link("srv-sleep", "unreachable", true);
        sleep.label = "Travel laptop".into();
        sleep.last_seen_at = "2026-11-01T00:00:00Z".into();
        sleep.last_error = "timed out https://sleep.example/health".into();
        let mut expired = link("srv-old", "expired", true);
        expired.access = "auto".into();
        let body = machines_view(&catalog(vec![
            link("srv-west-1", "reachable", false),
            sleep,
            expired,
            link("srv-down", "unreachable", false),
        ]));
        assert_eq!(body["forwarding"], false);
        assert_eq!(body["ok"], true);
        assert_eq!(body["error"], "");
        let machines = body["machines"].as_array().unwrap();
        assert_eq!(machines[0]["local"], true);
        assert_eq!(machines[0]["id"], "");
        assert_eq!(machines[0]["label"], "Studio");
        assert_eq!(machines[0]["status"], "online");
        assert_eq!(machines[0]["t3Version"], "0.1.9");
        assert_eq!(machines[1]["status"], "online");
        assert_eq!(machines[2]["status"], "asleep");
        assert_eq!(machines[2]["label"], "Travel laptop");
        assert_eq!(machines[2]["lastSeenAt"], "2026-11-01T00:00:00Z");
        assert_eq!(machines[2]["lastError"], "timed out");
        assert_eq!(machines[3]["status"], "expired");
        assert_eq!(machines[4]["status"], "unreachable");
        let text = body.to_string();
        assert!(!text.contains("https://"));
        assert!(!text.contains("15f077d9-b158-435e-96f1-ed7709f11cbd"));
        assert!(!text.contains("urls"));
    }

    #[test]
    fn an_offline_catalog_has_no_machines() {
        let body = machines_view(&ComputerCatalog::offline());
        assert_eq!(body["ok"], false);
        assert_eq!(body["forwarding"], false);
        assert_eq!(body["error"], "environment_offline");
        assert!(body["machines"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_reachable_laptop_stays_online() {
        assert_eq!(machine_status("reachable", true), "online");
        assert_eq!(machine_status("expired", true), "expired");
        assert_eq!(machine_status("asleep", false), "asleep");
    }
}
