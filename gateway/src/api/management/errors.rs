use super::*;

#[derive(Debug)]
pub(super) struct ToolError {
    code: &'static str,
    pub message: String,
    status: Option<u16>,
    details: Option<Value>,
}
impl ToolError {
    pub(super) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            status: None,
            details: None,
        }
    }
    pub(super) fn api(status: StatusCode, bytes: &[u8]) -> Self {
        let code = match status.as_u16() {
            400 | 415 | 422 => "invalid_arguments",
            401 | 403 => "permission_denied",
            404 => "not_found",
            409 => "conflict",
            413 => "payload_too_large",
            429 => "rate_limited",
            500..=599 => "backend_unavailable",
            _ => "operation_failed",
        };
        let details = serde_json::from_slice::<Value>(bytes).ok();
        let message = details
            .as_ref()
            .and_then(|v| v["error"].as_str().or_else(|| v["message"].as_str()))
            .unwrap_or("The gateway operation failed")
            .to_owned();
        Self {
            code,
            message,
            status: Some(status.as_u16()),
            details,
        }
    }
    pub(super) fn result(&self) -> Value {
        json!({"isError":true,"content":[{"type":"text","text":self.message}],"structuredContent":{"error":{"code":self.code,"message":self.message,"status":self.status,"details":self.details}}})
    }
}
impl From<&str> for ToolError {
    fn from(message: &str) -> Self {
        Self::new("invalid_arguments", message)
    }
}
impl From<String> for ToolError {
    fn from(message: String) -> Self {
        Self::new("invalid_arguments", message)
    }
}
