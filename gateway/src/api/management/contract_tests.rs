use super::*;

#[test]
fn every_generated_schema_compiles_and_openapi_matches_the_registry() {
    let (_, operations) = super::router::application_routes();
    let api = contracts::openapi(&operations);
    assert_eq!(api["openapi"], "3.1.0");
    let count: usize = api["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|p| p.as_object().unwrap().len())
        .sum();
    assert_eq!(count, operations.len());
    for op in operations {
        jsonschema::validator_for(&op.input_schema()).unwrap();
        jsonschema::validator_for(&op.output_schema()).unwrap();
        assert_eq!(api["paths"][op.path][op.method]["operationId"], op.name);
        assert_eq!(
            api["paths"][op.path][op.method]["x-hbots-permission"],
            op.permission()
        );
    }
    assert!(
        api["paths"]["/api/vault/import-csv"]["post"]["requestBody"]["content"]["text/csv"]
            .is_object()
    );
    assert!(
        api["paths"]["/api/vault/export.csv"]["get"]["responses"]["200"]["content"]["text/csv"]
            .is_object()
    );
}

#[tokio::test]
async fn argument_schemas_reject_bad_types_and_encoded_bypasses_without_echoing_values() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    storage::write_json(
        &s.settings_path(),
        &json!({"managementMcp":{"enabled":true}}),
    )
    .await
    .unwrap();
    let peer = "127.0.0.1:1234".parse().unwrap();
    for arguments in [
        json!({"body":{"name":123}}),
        json!({"bodyBase64":STANDARD.encode(br#"{"name":123}"#)}),
        json!({"path":{"unexpected":"value"},"body":{"name":"Ignored"}}),
        json!({"body":{"name":"Ignored"},"bodyBase64":"e30="}),
    ] {
        let err = execute(
            &s,
            peer,
            &json!({"name":"hbots_create_bot","arguments":arguments}),
        )
        .await
        .unwrap_err();
        assert!(!err.message.contains("123"));
        assert!(s.bots.list().await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn self_disable_returns_success_and_prevents_further_calls() {
    let dir = tempfile::tempdir().unwrap();
    let s = super::super::tests::settings_state(dir.path());
    storage::write_json(
        &s.settings_path(),
        &json!({"managementMcp":{"enabled":true}}),
    )
    .await
    .unwrap();
    let peer = "127.0.0.1:1234".parse().unwrap();
    let result=execute(&s,peer,&json!({"name":"hbots_put_settings","arguments":{"body":{"managementMcp":{"enabled":false}}}})).await.unwrap();
    assert_eq!(result["status"], 200);
    assert_eq!(s.settings().await["managementMcp"]["enabled"], false);
    assert!(execute(&s, peer, &json!({"name":"hbots_list_bots"}))
        .await
        .is_err());
}

#[test]
fn image_content_preserves_structured_api_results_and_rejects_invalid_image_data() {
    let image = STANDARD.encode(crate::chat::images::png_bytes(8, 6));
    let value = json!({"status":200,"body":{"image":image,"selected":"tab"}});
    let result = content::success(value.clone());
    assert_eq!(result["structuredContent"], value);
    assert_eq!(result["content"][1]["type"], "image");
    assert_eq!(result["content"][1]["mimeType"], "image/png");
    assert_eq!(result["content"][1]["data"], image);
    let invalid = content::success(
        json!({"status":200,"body":{"contentType":"image/png","bodyBase64":STANDARD.encode(b"not an image")}}),
    );
    assert_eq!(invalid["content"].as_array().unwrap().len(), 1);
}
