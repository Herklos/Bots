//! T3 Code, the harness the gateway runs.
//!
//! [`client`] is the transport. Each other file is one capability.

mod approvals;
mod browser;
mod client;
pub(crate) mod fill;
pub(crate) mod hold;
mod previews;
mod provision;
mod routines;
pub(crate) mod setup;
pub(crate) mod stream;
pub(crate) mod transcript;
mod turns;

pub(crate) use client::{proxy_http, proxy_ws, ws_base, T3Client, T3};

use crate::harness::{
    Approvals as ApprovalsCap, Browser as BrowserCap, Harness, HarnessError,
    Previews as PreviewsCap, Provisioning as ProvisioningCap, Relay, Routines as RoutinesCap,
    Transcript as TranscriptCap, Turns as TurnsCap,
};
use async_trait::async_trait;

#[async_trait]
impl Harness for T3 {
    async fn connected(&self) -> bool {
        self.rpc().await.is_ok()
    }

    fn transcript(&self) -> Result<&dyn TranscriptCap, HarnessError> {
        Ok(self)
    }

    fn turns(&self) -> Result<&dyn TurnsCap, HarnessError> {
        Ok(self)
    }

    fn previews(&self) -> Result<&dyn PreviewsCap, HarnessError> {
        Ok(self)
    }

    fn routines(&self) -> Result<&dyn RoutinesCap, HarnessError> {
        Ok(self)
    }

    fn approvals(&self) -> Result<&dyn ApprovalsCap, HarnessError> {
        Ok(self)
    }

    fn provisioning(&self) -> Result<&dyn ProvisioningCap, HarnessError> {
        Ok(self)
    }

    fn browser(&self) -> Result<&dyn BrowserCap, HarnessError> {
        Ok(self)
    }

    fn relay(&self) -> Result<&dyn Relay, HarnessError> {
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::T3;
    use crate::chat::approvals::{outbound, send};
    use crate::harness::Harness;
    use serde_json::{json, Value};

    fn bun() -> Option<std::path::PathBuf> {
        if let Some(path) = std::env::var_os("BUN") {
            return Some(path.into());
        }
        let home = std::env::var_os("HOME")?;
        let installed = std::path::PathBuf::from(home).join(".bun/bin/bun");
        installed.exists().then_some(installed)
    }

    /// The mock speaks the same tags the gateway sends. Skipped when bun is not installed.
    #[tokio::test]
    async fn the_mock_round_trips_a_routine_and_an_approval() {
        let Some(bun) = bun() else {
            eprintln!("bun is not installed; the mock round trip was not run");
            return;
        };
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/mock-t3.ts");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut child = std::process::Command::new(bun)
            .arg(&script)
            .arg(port.to_string())
            .env("T3_TOKEN", "test-token")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("bun");
        let url = format!("http://127.0.0.1:{port}");
        let ready = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if tokio::net::TcpStream::connect(("127.0.0.1", port))
                    .await
                    .is_ok()
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;
        if ready.is_err() {
            let _ = child.kill();
            panic!("mock T3 did not listen");
        }
        let t3 = T3::new(url.clone(), "test-token".into());
        let history_dir = tempfile::tempdir().unwrap();
        let history =
            crate::chat::routines::RoutineHistory::new(history_dir.path().join("history.json"));
        let routines = t3.routines().expect("routines");
        let created = crate::chat::routines::change(
            routines,
            &history,
            "upsert",
            json!({ "title": "Morning", "prompt": "Say hello", "enabled": true }),
        )
        .await
        .expect("upsert");
        let id = created["task"]["id"].as_str().unwrap().to_string();
        let listed = crate::chat::routines::list(routines).await.expect("list");
        assert_eq!(listed["tasks"][0]["title"], "Morning");
        crate::chat::routines::change(routines, &history, "run", json!({ "id": id }))
            .await
            .expect("run");
        let approval = outbound(
            "approval_request",
            "thread-1",
            "req-2",
            &json!({ "decision": "decline", "feedback": "Use an explicit path." }),
        )
        .unwrap();
        let approvals = t3.approvals().expect("approvals");
        send(approvals, approval).await.expect("approval");
        let secret = outbound(
            "secret_request",
            "thread-1",
            "turn-9",
            &json!({ "secret": "hunter2" }),
        )
        .unwrap();
        send(approvals, secret).await.expect("secret");
        let answers: Value = t3
            .http
            .get(format!("{url}/mock/answers"))
            .bearer_auth("test-token")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(answers["answered"][0]["decision"], "decline");
        assert_eq!(answers["answered"][1]["answerType"], "save");
        assert!(history.recent().await.as_array().unwrap().len() >= 2);
        let _ = child.kill();
        let _ = child.wait();
    }
}
