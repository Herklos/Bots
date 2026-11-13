//! Two engines, two folders, one mock. A write acknowledged on a device stays
//! findable after both sides go quiet, and the folders agree or a conflict is open.

use super::super::sha256_hex;
use super::support::{api, hb_files, read_rel, remote_body, write_rel, Rig};
use crate::storage::adapters::anytype::tests::mock::Mock;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const PATHS: usize = 50;
const OPS: usize = 1_000;

enum Act {
    Write(String),
    Delete,
}

#[tokio::test]
async fn two_engines_keep_every_acknowledged_write() {
    let mock = Mock::start().await;
    super::support::prepare(&mock).await;
    let mut rigs = [
        Rig::open(&mock.url, "device-a"),
        Rig::open(&mock.url, "device-b"),
    ];
    rigs[0].live().await;
    rigs[1].live().await;
    let paths: Vec<String> = (0..PATHS)
        .map(|index| format!("bot/memory/f{index:02}.md"))
        .collect();
    let mut rng = StdRng::seed_from_u64(215);
    let mut latest = [BTreeMap::new(), BTreeMap::new()];
    for op in 0..OPS {
        let side = rng.gen_range(0..2);
        let path = paths[rng.gen_range(0..PATHS)].clone();
        if rng.gen_bool(0.8) {
            let bytes = format!("s{side}-{op}\n").into_bytes();
            let sha = sha256_hex(&bytes);
            write_rel(rigs[side].root.path(), &path, &bytes);
            latest[side].insert(path, Act::Write(sha));
        } else {
            let file = rigs[side].root.path().join(&path);
            if file.is_file() {
                std::fs::remove_file(file).unwrap();
            }
            latest[side].insert(path, Act::Delete);
        }
        let cycle = rigs[side].engine.run_once().await.unwrap();
        assert!(!cycle.stopped, "{cycle:?}");
        if op % 4 == 0 {
            let other = rigs[1 - side].engine.run_once().await.unwrap();
            assert!(!other.stopped, "{other:?}");
        }
    }
    quiesce(&mut rigs).await;
    let bag = harvest(&mock, &rigs).await;
    for (side, marks) in latest.iter().enumerate() {
        for (path, act) in marks {
            let Act::Write(sha) = act else {
                continue;
            };
            assert!(bag.contains(sha), "device {side} lost {path} {sha}");
        }
    }
    let conflicts = api(&mock.url)
        .search(&serde_json::json!({
            "type": "hb_conflict",
            "fields": ["hb_path", "hb_conflict_state", "hb_copy_a", "hb_copy_b"],
        }))
        .await
        .unwrap();
    for path in &paths {
        let left = read_rel(rigs[0].root.path(), path);
        let right = read_rel(rigs[1].root.path(), path);
        if left != right {
            assert!(
                open_conflict(&rigs, &conflicts, path),
                "{path} diverged without a conflict"
            );
        }
        let copies = hb_files(&mock.url)
            .await
            .into_iter()
            .filter(|row| row["properties"]["hb_path"].as_str() == Some(path.as_str()))
            .count();
        assert!(copies <= 1, "{path} has {copies} objects");
    }
}

async fn quiesce(rigs: &mut [Rig; 2]) {
    let mut stable = 0_u8;
    for _ in 0..80 {
        let mut busy = false;
        for rig in rigs.iter_mut() {
            let cycle = rig.engine.run_once().await.unwrap();
            assert!(!cycle.stopped, "{cycle:?}");
            if cycle.applied > 0 || cycle.deferred {
                busy = true;
            }
        }
        if busy {
            stable = 0;
        } else {
            stable = stable.saturating_add(1);
        }
        if stable >= 2 {
            return;
        }
    }
    panic!("engines did not quiesce");
}

async fn harvest(mock: &Mock, rigs: &[Rig; 2]) -> BTreeSet<String> {
    let mut bag = BTreeSet::new();
    for rig in rigs {
        collect(rig.root.path(), &mut bag);
        collect(rig.sync_dir.path(), &mut bag);
    }
    for row in hb_files(&mock.url).await {
        bag.insert(sha256_hex(&remote_body(&mock.url, &row).await));
    }
    let conflicts = api(&mock.url)
        .search(&serde_json::json!({
            "type": "hb_conflict",
            "fields": ["hb_path", "hb_conflict_state", "hb_copy_a", "hb_copy_b"],
        }))
        .await
        .unwrap();
    for row in conflicts {
        for key in ["hb_copy_a", "hb_copy_b"] {
            let Some(id) = row["properties"][key]
                .as_array()
                .and_then(|ids| ids.first())
                .and_then(Value::as_str)
            else {
                continue;
            };
            if let Ok(bytes) = api(&mock.url).download_file(id).await {
                bag.insert(sha256_hex(&bytes));
            }
        }
    }
    bag
}

fn collect(dir: &Path, bag: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, bag);
        } else if let Ok(bytes) = std::fs::read(&path) {
            bag.insert(sha256_hex(&bytes));
        }
    }
}

fn open_conflict(rigs: &[Rig; 2], conflicts: &[Value], path: &str) -> bool {
    rigs.iter()
        .any(|rig| rig.engine.conflict_for(path).is_some())
        || conflicts.iter().any(|row| {
            row["properties"]["hb_path"].as_str() == Some(path)
                && row["properties"]["hb_conflict_state"].as_str() == Some("open")
        })
}
