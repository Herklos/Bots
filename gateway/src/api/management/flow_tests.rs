//! Full management flows with a recorded in-memory harness; no real Bot work runs.
use super::*;
use crate::harness::adapters::mock::MockHarness;

#[tokio::test]
async fn chat_routines_and_pending_requests_reach_the_harness_and_preserve_runtime_caps() {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    let s = super::super::tests::settings_state_with(dir.path(), Some(mock.clone()));
    storage::write_json(
        &s.settings_path(),
        &json!({"managementMcp":{"enabled":true},"maxRuntimeMode":"approval-required"}),
    )
    .await
    .unwrap();
    let peer = "127.0.0.1:1234".parse().unwrap();
    let bot=execute(&s,peer,&json!({"name":"hbots_create_bot","arguments":{"body":{"name":"Flow Ada","runtimeMode":"full-access"}}})).await.unwrap()["body"].clone();
    assert_eq!(bot["runtimeMode"], "approval-required");
    let slug = bot["slug"].as_str().unwrap();
    s.bots
        .save(
            slug,
            &json!({"mainThreadId":"flow-thread","projectId":"flow-project"}),
            false,
        )
        .await
        .unwrap();
    let sent=execute(&s,peer,&json!({"name":"hbots_send_chat","arguments":{"path":{"slug":slug},"body":{"text":"Check the fixture","messageId":"flow-message"}}})).await.unwrap();
    assert_eq!(sent["body"]["messageId"], "flow-message");
    assert_eq!(mock.messages()[0].thread_id, "flow-thread");
    assert_eq!(mock.messages()[0].text, "Check the fixture");
    let voice=execute(&s,peer,&json!({"name":"hbots_send_voice","arguments":{"path":{"slug":slug},"body":{"data":STANDARD.encode(b"audio fixture"),"seconds":2.3}}})).await.unwrap();
    assert!(voice["body"]["attachment"]["id"].is_string());
    assert_eq!(mock.messages()[1].attachments.len(), 1);
    execute(&s,peer,&json!({"name":"hbots_respond_request","arguments":{"path":{"slug":slug,"id":"flow-request"},"body":{"kind":"approval","decision":"allow"}}})).await.unwrap();
    assert_eq!(mock.approvals().len(), 1);
    let made=execute(&s,peer,&json!({"name":"hbots_bot_routine_save","arguments":{"path":{"slug":slug},"body":{"title":"Fixture check","kind":"day"}}})).await.unwrap();
    let task = &made["body"]["task"];
    let id = task["id"].as_str().unwrap();
    assert_eq!(task["runtimeMode"], "approval-required");
    let listed = execute(
        &s,
        peer,
        &json!({"name":"hbots_bot_routines","arguments":{"path":{"slug":slug}}}),
    )
    .await
    .unwrap();
    assert_eq!(listed["body"]["tasks"][0]["id"], id);
    execute(&s,peer,&json!({"name":"hbots_bot_routine_enabled","arguments":{"path":{"slug":slug},"body":{"id":id,"enabled":true}}})).await.unwrap();
    execute(&s,peer,&json!({"name":"hbots_bot_routine_run","arguments":{"path":{"slug":slug},"body":{"id":id}}})).await.unwrap();
    execute(&s,peer,&json!({"name":"hbots_bot_routine_delete","arguments":{"path":{"slug":slug},"body":{"id":id}}})).await.unwrap();
    let listed = execute(
        &s,
        peer,
        &json!({"name":"hbots_bot_routines","arguments":{"path":{"slug":slug}}}),
    )
    .await
    .unwrap();
    assert_eq!(listed["body"]["tasks"], json!([]));
}

#[tokio::test]
async fn api_failures_keep_machine_readable_error_details() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    storage::write_json(
        &s.settings_path(),
        &json!({"managementMcp":{"enabled":true}}),
    )
    .await
    .unwrap();
    let error = execute(
        &s,
        "127.0.0.1:1234".parse().unwrap(),
        &json!({"name":"hbots_get_bot","arguments":{"path":{"slug":"missing"}}}),
    )
    .await
    .unwrap_err()
    .result();
    assert_eq!(error["isError"], true);
    assert_eq!(error["structuredContent"]["error"]["code"], "not_found");
    assert_eq!(error["structuredContent"]["error"]["status"], 404);
}
