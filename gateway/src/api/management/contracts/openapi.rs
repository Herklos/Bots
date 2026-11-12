use super::*;

pub(in crate::api::management) fn openapi(operations: &[Operation]) -> Value {
    let mut paths = serde_json::Map::new();
    for operation in operations {
        let mut parameters = Vec::new();
        for (location, schema) in [
            ("path", operation.path_schema()),
            ("query", operation.query_schema()),
        ] {
            for (name, field) in schema["properties"].as_object().unwrap() {
                parameters.push(
                    json!({"name":name,"in":location,"required":location=="path","schema":field}),
                );
            }
        }
        let mut entry = json!({"operationId":operation.name,"summary":format!("{} {}",operation.method.to_uppercase(),operation.path),"parameters":parameters,"x-hbots-permission":operation.permission(),"responses":{"200":{"description":"Original gateway operation result","content":{"application/json":{"schema":{"description":"Domain and provider response fields are preserved.","x-hbots-dynamic":true}}}},"default":{"description":"Gateway error","content":{"application/json":{"schema":{"type":"object","properties":{"error":{"type":"string"}},"required":["error"]}}}}},"security":[{"applicationSession":[]}]});
        entry["responses"]["200"]["content"]["application/json"]["schema"] =
            operation.api_response_schema();
        if let Some(body) = operation.body_schema() {
            let media = if operation.name == "hbots_import_csv" {
                "text/csv"
            } else {
                "application/json"
            };
            entry["requestBody"] = json!({"required":false,"content":{media:{"schema":body}}});
        }
        let binary_media = match operation.name.as_str() {
            "hbots_export" => Some("text/csv"),
            "hbots_download" | "hbots_library_file" | "hbots_get_icon" => {
                Some("application/octet-stream")
            }
            _ => None,
        };
        if let Some(media) = binary_media {
            entry["responses"]["200"]["content"] =
                json!({media:{"schema":{"type":"string","format":"binary"}}});
        }
        paths
            .entry(operation.path.to_string())
            .or_insert_with(|| json!({}))[operation.method] = entry;
    }
    json!({"openapi":"3.1.0","info":{"title":"hbots gateway application API","version":env!("CARGO_PKG_VERSION"),"description":"Generated from the application operation registry. Infrastructure endpoints are excluded. MCP uses the same operations through /mcp with dedicated bearer credentials."},"paths":paths,"components":{"securitySchemes":{"applicationSession":{"type":"http","scheme":"bearer"}}}})
}
