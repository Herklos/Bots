use super::*;
use crate::storage::stores::SettingsStore;

#[derive(Debug, Default)]
struct MemorySettings(std::sync::Mutex<Option<Value>>);

#[async_trait::async_trait]
impl SettingsStore for MemorySettings {
    async fn read(&self) -> std::io::Result<Option<Value>> {
        Ok(self.0.lock().unwrap().clone())
    }
    async fn write(&self, value: &Value) -> std::io::Result<()> {
        *self.0.lock().unwrap() = Some(value.clone());
        Ok(())
    }
    async fn initialize(&self, defaults: &Value) -> std::io::Result<()> {
        let mut saved = self.0.lock().unwrap();
        if saved.is_none() {
            *saved = Some(defaults.clone());
        }
        Ok(())
    }
}

#[tokio::test]
async fn settings_api_reads_and_writes_the_injected_store() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = super::tests::settings_state(dir.path());
    let store = Arc::new(MemorySettings::default());
    *store.0.lock().unwrap() = Some(json!({"userName":"Ada", "theme":"light"}));
    Arc::get_mut(&mut state).unwrap().settings_store = store.clone();
    assert_eq!(state.settings().await["userName"], "Ada");
    super::tests::put(&state, json!({"userName":"Bea"}))
        .await
        .unwrap();
    let saved = store.read().await.unwrap().unwrap();
    assert_eq!(saved["userName"], "Bea");
    assert_eq!(saved["theme"], "light");
    assert!(!state.settings_path().exists());
}

#[tokio::test]
async fn bot_instructions_use_the_injected_settings_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(MemorySettings::default());
    *store.0.lock().unwrap() = Some(json!({"rules":{"alwaysAsk":["approve purchases"]}}));
    let bots = crate::bots::Bots::new(dir.path().to_path_buf(), String::new(), String::new())
        .with_settings_store(store);
    bots.create(&json!({"name":"Ada", "runtimeMode":"independent"}))
        .await
        .unwrap();
    let instructions = tokio::fs::read_to_string(dir.path().join("ada/AGENTS.md"))
        .await
        .unwrap();
    assert!(instructions.contains("approve purchases"));
    assert!(!dir.path().join(".botapp/settings.json").exists());
}

#[derive(Debug, Default)]
struct UnavailableSettings(std::sync::atomic::AtomicBool);

#[async_trait::async_trait]
impl SettingsStore for UnavailableSettings {
    async fn read(&self) -> std::io::Result<Option<Value>> {
        Err(std::io::Error::other("settings cache unavailable"))
    }
    async fn write(&self, _: &Value) -> std::io::Result<()> {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    async fn initialize(&self, _: &Value) -> std::io::Result<()> {
        Err(std::io::Error::other("settings cache unavailable"))
    }
}

#[tokio::test]
async fn settings_update_does_not_write_defaults_after_store_read_failure() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = super::tests::settings_state(dir.path());
    let store = Arc::new(UnavailableSettings::default());
    Arc::get_mut(&mut state).unwrap().settings_store = store.clone();
    assert_eq!(
        state.settings().await["maxRuntimeMode"],
        "approval-required"
    );
    assert!(super::tests::put(&state, json!({"userName":"Ada"}))
        .await
        .is_err());
    assert!(!store.0.load(std::sync::atomic::Ordering::SeqCst));
}
