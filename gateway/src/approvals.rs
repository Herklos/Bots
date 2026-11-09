//! Answering a pending request on a Bot's thread.
//!
//! Secrets use `secrets.answerRequest` (`packages/contracts/src/rpc.ts`, payload
//! `SecretRequestAnswerInput` in `packages/contracts/src/secretRequest.ts`):
//! `{ threadId, turnItemId, answer: { type: "save", secret } | { type: "decline" } }`.
//! Approvals and questions use the orchestration command `runtime-request.respond`
//! (sent through `orchestration.dispatchCommand`; the fields are `threadId`, `requestId`,
//! and optional `decision`, `answers` and `feedback`, as in
//! `apps/server/src/orchestration-v2/testkit/OrchestratorScenario.ts`).
use crate::error::ApiResult;
use crate::t3::T3;
use serde_json::{json, Value};

pub const SECRET_TAG: &str = "secrets.answerRequest";
pub const RESPOND: &str = "runtime-request.respond";

pub enum Outbound {
    /// `secrets.answerRequest`. The secret, when there is one, stays inside this value.
    Secret(Value),
    /// A `runtime-request.respond` command. `dispatch` adds the command id.
    Command(Value),
}

pub fn outbound(
    kind: &str,
    thread_id: &str,
    request_id: &str,
    body: &Value,
) -> Result<Outbound, String> {
    if thread_id.is_empty() || request_id.is_empty() {
        return Err("This Bot has no conversation to answer in.".into());
    }
    match kind {
        "secret" | "secret_request" => secret(thread_id, request_id, body),
        "approval" | "approval_request" | "question" | "user_input" | "user_input_request" => {
            command(thread_id, request_id, body)
        }
        _ => Err("That request is not one this app can answer.".into()),
    }
}

fn secret(thread_id: &str, request_id: &str, body: &Value) -> Result<Outbound, String> {
    let decline = body["decision"].as_str() == Some("decline")
        || body["answer"]["type"].as_str() == Some("decline");
    let answer = if decline {
        json!({ "type": "decline" })
    } else {
        let secret = body["secret"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Enter the secret, or decline.")?;
        json!({ "type": "save", "secret": secret })
    };
    Ok(Outbound::Secret(json!({
        "threadId": thread_id,
        "turnItemId": request_id,
        "answer": answer,
    })))
}

fn command(thread_id: &str, request_id: &str, body: &Value) -> Result<Outbound, String> {
    let mut cmd = json!({
        "type": RESPOND,
        "threadId": thread_id,
        "requestId": request_id,
    });
    if let Some(decision) = body["decision"].as_str().filter(|s| !s.is_empty()) {
        cmd["decision"] = json!(decision);
    }
    if let Some(answers) = body.get("answers").filter(|v| !v.is_null()) {
        cmd["answers"] = answers.clone();
    }
    if let Some(feedback) = body["feedback"].as_str().filter(|s| !s.is_empty()) {
        cmd["feedback"] = json!(feedback);
    }
    if cmd.get("decision").is_none() && cmd.get("answers").is_none() {
        return Err("Choose an answer first.".into());
    }
    Ok(Outbound::Command(cmd))
}

pub async fn send(t3: &T3, outbound: Outbound) -> ApiResult<Value> {
    let rpc = t3.rpc().await?;
    match outbound {
        Outbound::Secret(payload) => Ok(rpc.call(SECRET_TAG, payload).await?),
        Outbound::Command(command) => Ok(rpc.dispatch(command).await?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_is_saved_or_declined_without_a_command() {
        let saved = outbound(
            "secret_request",
            "thread-1",
            "turn-9",
            &json!({ "secret": "hunter2" }),
        )
        .unwrap();
        let Outbound::Secret(payload) = saved else {
            panic!("secret answers are not orchestration commands");
        };
        assert_eq!(payload["threadId"], "thread-1");
        assert_eq!(payload["turnItemId"], "turn-9");
        assert_eq!(payload["answer"]["type"], "save");
        assert_eq!(payload["answer"]["secret"], "hunter2");

        let declined = outbound(
            "secret",
            "thread-1",
            "turn-9",
            &json!({ "decision": "decline" }),
        )
        .unwrap();
        let Outbound::Secret(payload) = declined else {
            panic!("a decline is still a secret answer");
        };
        assert_eq!(payload["answer"], json!({ "type": "decline" }));
        assert!(payload["answer"].get("secret").is_none());
    }

    #[test]
    fn approvals_and_questions_become_a_runtime_request_response() {
        let approval = outbound(
            "approval_request",
            "thread-1",
            "req-2",
            &json!({ "decision": "decline", "feedback": "Use an explicit path." }),
        )
        .unwrap();
        let Outbound::Command(cmd) = approval else {
            panic!("an approval is a command");
        };
        assert_eq!(cmd["type"], RESPOND);
        assert_eq!(cmd["requestId"], "req-2");
        assert_eq!(cmd["decision"], "decline");
        assert_eq!(cmd["feedback"], "Use an explicit path.");

        let question = outbound(
            "user_input_request",
            "thread-1",
            "req-3",
            &json!({ "answers": { "name": "Ada" } }),
        )
        .unwrap();
        let Outbound::Command(cmd) = question else {
            panic!("a question is a command");
        };
        assert_eq!(cmd["answers"]["name"], "Ada");
        assert!(outbound("approval", "thread-1", "req-2", &json!({})).is_err());
        assert!(outbound(
            "nope",
            "thread-1",
            "req-2",
            &json!({ "decision": "accept" })
        )
        .is_err());
    }

    fn bun() -> Option<std::path::PathBuf> {
        if let Some(path) = std::env::var_os("BUN") {
            return Some(path.into());
        }
        let home = std::env::var_os("HOME")?;
        let installed = std::path::PathBuf::from(home).join(".bun/bin/bun");
        installed.exists().then_some(installed)
    }

    /// The mock speaks the same tags the gateway sends. Skipped when bun is not installed
    /// (CI runs the payload tests above; this checks the socket).
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
        let history = crate::routines::RoutineHistory::new(history_dir.path().join("history.json"));
        let created = crate::routines::change(
            &t3,
            &history,
            "upsert",
            crate::routines::UPSERT,
            json!({ "title": "Morning", "prompt": "Say hello", "enabled": true }),
        )
        .await
        .expect("upsert");
        let id = created["task"]["id"].as_str().unwrap().to_string();
        let listed = crate::routines::list(&t3).await.expect("list");
        assert_eq!(listed["tasks"][0]["title"], "Morning");
        crate::routines::change(
            &t3,
            &history,
            "run",
            crate::routines::RUN_NOW,
            json!({ "id": id }),
        )
        .await
        .expect("run");
        let approval = outbound(
            "approval_request",
            "thread-1",
            "req-2",
            &json!({ "decision": "decline", "feedback": "Use an explicit path." }),
        )
        .unwrap();
        send(&t3, approval).await.expect("approval");
        let secret = outbound(
            "secret_request",
            "thread-1",
            "turn-9",
            &json!({ "secret": "hunter2" }),
        )
        .unwrap();
        send(&t3, secret).await.expect("secret");
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
