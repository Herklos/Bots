//! Shared mock, folders, and assertions for the engine tests.

use super::super::engine::{Config, Engine};
use super::super::hook::{Control, Hook, Step};
use super::super::payload;
use super::super::sha256_hex;
use super::super::status::Phase;
use crate::storage::adapters::anytype::client::AnytypeClient;
use crate::storage::adapters::anytype::tests::mock::Mock;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

pub(super) struct Rig {
    pub root: TempDir,
    pub sync_dir: TempDir,
    pub engine: Engine,
    pub device: String,
    pub url: String,
    config: Config,
}

pub(super) struct World {
    pub mock: Mock,
    pub rig: Rig,
}

impl World {
    pub(super) async fn start(device: &str) -> Self {
        let mock = Mock::start().await;
        let rig = Rig::open(&mock.url, device);
        Self { mock, rig }
    }

    pub(super) async fn live(&mut self) {
        self.rig.live().await;
    }

    pub(super) fn write(&self, rel: &str, bytes: &[u8]) {
        write_rel(self.rig.root.path(), rel, bytes);
    }

    pub(super) fn read(&self, rel: &str) -> Option<Vec<u8>> {
        read_rel(self.rig.root.path(), rel)
    }
}

impl Rig {
    pub(super) fn open(url: &str, device: &str) -> Self {
        let root = TempDir::new().unwrap();
        let sync_dir = TempDir::new().unwrap();
        let config = test_config(
            url,
            device,
            root.path().to_path_buf(),
            sync_dir.path().to_path_buf(),
        );
        let engine = Engine::open(config.clone()).unwrap();
        Self {
            root,
            sync_dir,
            engine,
            device: device.to_string(),
            url: url.to_string(),
            config,
        }
    }

    pub(super) async fn live(&mut self) {
        let cycle = self.engine.confirm_import().await.unwrap();
        assert_eq!(self.engine.phase(), Phase::Live, "{cycle:?}");
    }

    pub(super) fn reopen(&mut self) {
        self.engine = Engine::open(self.config.clone()).unwrap();
    }

    pub(super) async fn until_quiet(&mut self) {
        for _ in 0..24 {
            let cycle = self.engine.run_once().await.unwrap();
            assert!(!cycle.stopped, "{cycle:?}");
            if cycle.applied == 0 && !cycle.deferred {
                if cycle.status.retry_in > Duration::ZERO {
                    tokio::time::sleep(cycle.status.retry_in).await;
                    continue;
                }
                return;
            }
        }
        panic!("engine did not settle");
    }
}

pub(super) fn test_config(url: &str, device: &str, root: PathBuf, sync_dir: PathBuf) -> Config {
    let mut config = Config::new(url, "k", "space", root, sync_dir, device);
    config.write_rate = 10_000.0;
    config.write_burst = 10_000.0;
    config.max_writes = 200;
    config.settle = Duration::ZERO;
    config.hash_budget = Duration::from_secs(5);
    config.backoff_floor = Duration::from_millis(5);
    config.max_passes = 12;
    config
}

pub(super) fn write_rel(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
}

pub(super) fn read_rel(root: &Path, rel: &str) -> Option<Vec<u8>> {
    std::fs::read(root.join(rel)).ok()
}

pub(super) fn api(url: &str) -> AnytypeClient {
    AnytypeClient::new(url, "k", "space").unwrap()
}

pub(super) async fn hb_files(url: &str) -> Vec<Value> {
    api(url).search(&json!({"type": "hb_file"})).await.unwrap()
}

pub(super) async fn files_with_path(url: &str, rel: &str) -> Vec<Value> {
    hb_files(url)
        .await
        .into_iter()
        .filter(|row| row["properties"]["hb_path"].as_str() == Some(rel))
        .collect()
}

pub(super) async fn remote_body(url: &str, object: &Value) -> Vec<u8> {
    let props = &object["properties"];
    if let Some(id) = props["hb_blob"]
        .as_array()
        .and_then(|ids| ids.first())
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        return api(url).download_file(id).await.unwrap();
    }
    props["hb_content"]
        .as_str()
        .unwrap_or("")
        .as_bytes()
        .to_vec()
}

pub(super) async fn seed(url: &str, rel: &str, bytes: &[u8], key: &str) -> String {
    let client = api(url);
    let props = payload::file_properties(rel, bytes, false, "peer", 1, 0, None);
    let created = client
        .create_object(&payload::create_body(props), key)
        .await
        .unwrap();
    payload::id_of(&created).unwrap()
}

pub(super) fn peer_text(mock: &Mock, id: &str, bytes: &[u8]) {
    let text = std::str::from_utf8(bytes).unwrap();
    assert!(mock.peer_write(
        id,
        json!({
            "hb_content": text,
            "hb_sha": sha256_hex(bytes),
            "hb_deleted": false,
            "hb_size": bytes.len(),
        }),
        false,
    ));
}

pub(super) fn tombstone_flag(sync_dir: &Path, rel: &str) -> bool {
    let value: Value =
        serde_json::from_str(&std::fs::read_to_string(sync_dir.join("manifest.json")).unwrap())
            .unwrap();
    value["entries"][rel]["tombstone"]
        .as_bool()
        .unwrap_or(false)
}

pub(super) struct Kill {
    pub step: Step,
    pub left: u8,
}

impl Hook for Kill {
    fn on_step(&mut self, step: Step, _path: &str) -> Control {
        if step != self.step || self.left == 0 {
            return Control::Continue;
        }
        self.left -= 1;
        if self.left == 0 {
            Control::Stop
        } else {
            Control::Continue
        }
    }
}

pub(super) async fn assert_roundtrip(world: &World, rel: &str, bytes: &[u8]) {
    assert_eq!(world.read(rel).as_deref(), Some(bytes));
    let rows = files_with_path(&world.mock.url, rel).await;
    assert_eq!(rows.len(), 1, "{rel}");
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, bytes);
    assert_eq!(world.rig.engine.status().outbox, 0);
}
