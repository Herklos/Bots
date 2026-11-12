//! Explicit management permissions. `admin` grants the full application catalog;
//! the other capabilities do not imply one another.
use super::*;

pub(super) const SCOPES: [&str; 5] = ["read", "write", "secrets", "computer", "admin"];

#[derive(Clone, Debug)]
pub(super) struct Actor {
    pub id: String,
    pub scopes: Vec<String>,
}
impl Actor {
    pub(super) fn permits(&self, operation: &Operation) -> bool {
        self.scopes
            .iter()
            .any(|s| s == "admin" || s == operation.permission())
    }
}

impl Operation {
    pub(super) fn permission(&self) -> &'static str {
        if self.path.starts_with("/api/vault") {
            return "secrets";
        }
        if self.path.contains("/computer") {
            return "computer";
        }
        if ["/api/storage", "/api/access", "/api/system"]
            .iter()
            .any(|p| self.path.starts_with(p))
            || (self.path == "/api/settings" && self.method != "get")
        {
            return "admin";
        }
        if self.method == "get" {
            "read"
        } else {
            "write"
        }
    }
}

pub(super) fn parse_scopes(value: Option<&Value>) -> Result<Vec<String>, &'static str> {
    // Credentials issued by the first implementation had full administrative access.
    let Some(value) = value else {
        return Ok(vec!["admin".into()]);
    };
    let array = value
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or("scopes must be a non-empty array")?;
    let mut out = Vec::new();
    for scope in array {
        let scope = scope
            .as_str()
            .filter(|s| SCOPES.contains(s))
            .ok_or("Unknown MCP permission")?;
        if !out.iter().any(|s| s == scope) {
            out.push(scope.to_string());
        }
    }
    Ok(out)
}
