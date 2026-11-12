//! API request contracts shared by generated MCP definitions and OpenAPI.
//! Flexible provider payloads remain explicit extension objects; domain validation
//! remains authoritative for cross-field constraints and backend-specific values.
use super::*;
mod bodies;
#[cfg(test)]
mod openapi;
mod responses;
#[cfg(test)]
pub(super) use openapi::openapi;

pub(super) fn object(fields: &[(&str, Value)], required: &[&str]) -> Value {
    json!({"type":"object", "properties":fields.iter().map(|(k,v)| (k.to_string(),v.clone())).collect::<serde_json::Map<_,_>>(), "required":required, "additionalProperties":true})
}
fn string() -> Value {
    json!({"type":"string"})
}
fn boolean() -> Value {
    json!({"type":"boolean"})
}
fn strings() -> Value {
    json!({"type":"array","items":{"type":"string"}})
}
fn enumeration(values: &[&str]) -> Value {
    json!({"type":"string","enum":values})
}
fn dynamic(description: &str) -> Value {
    json!({"description":description,"x-hbots-dynamic":true})
}
fn fields(names: &[&str]) -> Vec<(String, Value)> {
    names.iter().map(|k| (k.to_string(), string())).collect()
}
fn string_object(names: &[&str]) -> Value {
    let mut schema = object(&[], &[]);
    schema["properties"] = json!(fields(names).into_iter().collect::<serde_json::Map<_, _>>());
    schema
}

impl Operation {
    pub(super) fn path_schema(&self) -> Value {
        let names: Vec<_> = self
            .path
            .split('/')
            .filter_map(|s| s.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
            .collect();
        let mut schema = object(
            &names
                .iter()
                .map(|s| (*s, json!({"type":"string","minLength":1})))
                .collect::<Vec<_>>(),
            &names,
        );
        schema["additionalProperties"] = json!(false);
        schema
    }
    pub(super) fn query_schema(&self) -> Value {
        let names: &[&str] = match self.name.as_str() {
            "hbots_read_chat" => &["child", "activity"],
            "hbots_read_activity" => &["child", "side"],
            "hbots_library_file" => &["path", "download"],
            "hbots_vault_get" => &["reveal"],
            "hbots_export" => &["type"],
            "hbots_memory_get" | "hbots_memory_put" => &["scope", "slug"],
            "hbots_computer_get" => &["tab", "view", "since"],
            "hbots_usage_report" => &["period", "bot"],
            "hbots_system_status" => &["check"],
            _ => &[],
        };
        let mut schema = string_object(names);
        // HTTP query extraction accepts string-valued extension parameters.
        schema["additionalProperties"] = string();
        schema
    }
    pub(super) fn input_schema(&self) -> Value {
        let mut schema = object(
            &[("path", self.path_schema()), ("query", self.query_schema())],
            &[],
        );
        schema["additionalProperties"] = json!(false);
        if !self.path_schema()["required"]
            .as_array()
            .unwrap()
            .is_empty()
        {
            schema["required"] = json!(["path"]);
        }
        if let Some(body) = self.body_schema() {
            schema["properties"]["body"] = body;
            schema["properties"]["bodyBase64"] = json!({"type":"string","contentEncoding":"base64","description":"Raw bytes instead of JSON body. JSON content is validated against the body schema after decoding."});
            schema["properties"]["contentType"] = json!({"type":"string","default":if self.name == "hbots_import_csv" {"text/csv"} else {"application/json"}});
            schema["not"] = json!({"required":["body","bodyBase64"]});
        }
        schema
    }
    pub(super) fn output_schema(&self) -> Value {
        let success = object(
            &[
                (
                    "status",
                    json!({"type":"integer","minimum":200,"maximum":299}),
                ),
                ("body", self.api_response_schema()),
            ],
            &["status", "body"],
        );
        let error = object(
            &[(
                "error",
                object(
                    &[
                        ("code", string()),
                        ("message", string()),
                        ("status", json!({"type":["integer","null"]})),
                        (
                            "details",
                            dynamic("Original structured gateway error details when available."),
                        ),
                    ],
                    &["code", "message"],
                ),
            )],
            &["error"],
        );
        json!({"type":"object","oneOf":[success,error]})
    }
}
