//! MCP client for Wiz's hosted (remote) MCP server.
//!
//! Wiz publishes a remote MCP server at `https://mcp.app.wiz.io`
//! (streamable-HTTP transport, JSON-RPC 2.0), authenticated with the
//! same OAuth bearer tokens the GraphQL API uses. stave operates the
//! server as a *client* — `stave mcp tools`, `stave mcp call <tool>` —
//! so the audit trail captures MCP usage alongside GraphQL usage. The
//! same write-guard posture applies: tools whose names are not
//! read-shaped require an explicit write opt-in.
//!
//! Transport, handshake, and the tool vocabulary are **provisional
//! until live-validated** (charter F3) — the shape below follows the
//! MCP streamable-HTTP spec and the sibling implementation verified
//! against another vendor's server.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::auth;
use crate::error::{Result, StaveError};

/// MCP protocol version stave speaks.
pub const PROTOCOL_VERSION: &str = "2025-03-26";

/// Default Wiz remote MCP endpoint.
pub const DEFAULT_MCP_URL: &str = "https://mcp.app.wiz.io";

/// Env override for the MCP endpoint.
pub const MCP_URL_ENV: &str = "STAVE_MCP_URL";

/// True when an MCP tool name is read-shaped and therefore exempt from
/// the write-guard. Conservative: unknown shapes are write-gated.
/// Accepts common read prefixes across `-`/`_` separators and
/// case-insensitively.
pub fn is_read_only_tool(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase().replace('_', "-");
    ["get-", "list-", "search-", "read-", "query-", "describe-", "find-"]
        .iter()
        .any(|p| normalized.starts_with(p))
}

/// Resolve the MCP endpoint: env → config (`[mcp] url`) → default.
pub fn resolve_url() -> Result<String> {
    if let Ok(v) = std::env::var(MCP_URL_ENV) {
        let v = v.trim();
        if !v.is_empty() {
            return Ok(v.to_string());
        }
    }
    if let Some(cfg) = auth::read_config()? {
        if let Some(u) = cfg.mcp.url.filter(|u| !u.trim().is_empty()) {
            return Ok(u.trim().to_string());
        }
    }
    Ok(DEFAULT_MCP_URL.to_string())
}

/// One tool from `tools/list`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Option<Value>,
}

/// Server identity from `initialize`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

pub struct McpClient {
    http: reqwest::Client,
    url: String,
    bearer: String,
}

impl McpClient {
    /// Build a client from an explicit endpoint + bearer token.
    pub fn new(url: impl Into<String>, bearer: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| StaveError::Network(e.to_string()))?;
        Ok(Self {
            http,
            url: url.into(),
            bearer: bearer.into(),
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// JSON-RPC `initialize` handshake. Returns the server info.
    pub async fn initialize(&self) -> Result<McpServerInfo> {
        let result = self
            .rpc(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {
                        "name": "stave",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                }),
                1,
            )
            .await?;
        let info = result
            .get("serverInfo")
            .cloned()
            .ok_or_else(|| StaveError::Network("initialize result missing serverInfo".into()))?;
        serde_json::from_value(info).map_err(StaveError::from)
    }

    /// JSON-RPC `tools/list`.
    pub async fn tools_list(&self) -> Result<Vec<McpTool>> {
        let result = self.rpc("tools/list", json!({}), 2).await?;
        let tools = result
            .get("tools")
            .cloned()
            .ok_or_else(|| StaveError::Network("tools/list result missing tools".into()))?;
        serde_json::from_value(tools).map_err(StaveError::from)
    }

    /// JSON-RPC `tools/call`. Returns the raw `result` object
    /// (`content` array + optional `isError`).
    pub async fn tools_call(&self, name: &str, arguments: Value) -> Result<Value> {
        self.rpc(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
            3,
        )
        .await
    }

    /// POST one JSON-RPC request and parse the response, which a
    /// streamable-HTTP server delivers either as `application/json` or
    /// as an SSE frame (`text/event-stream` with `data: <json>` lines).
    async fn rpc(&self, method: &str, params: Value, id: u64) -> Result<Value> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let response = self
            .http
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .bearer_auth(&self.bearer)
            .json(&payload)
            .send()
            .await
            .map_err(|e| StaveError::Network(e.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| StaveError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(StaveError::Http {
                status: status.as_u16(),
                body,
            });
        }

        let message = parse_rpc_body(&body)?;
        if let Some(err) = message.get("error") {
            return Err(StaveError::Network(format!(
                "MCP JSON-RPC error from {method}: {err}"
            )));
        }
        message
            .get("result")
            .cloned()
            .ok_or_else(|| StaveError::Network(format!("{method}: response has no result")))
    }
}

/// Parse a JSON-RPC response body that may be plain JSON or an SSE
/// stream. For SSE, the first `data:` line carrying a JSON-RPC message
/// wins.
fn parse_rpc_body(body: &str) -> Result<Value> {
    let trimmed = body.trim_start();
    if trimmed.starts_with('{') {
        return serde_json::from_str(trimmed).map_err(StaveError::from);
    }
    for line in body.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if v.get("jsonrpc").is_some() {
                    return Ok(v);
                }
            }
        }
    }
    Err(StaveError::Network(
        "MCP response was neither JSON nor an SSE frame with a JSON-RPC message".into(),
    ))
}

/// Extract the primary text payload from a `tools/call` result's
/// `content` array. When the text parses as JSON we return the parsed
/// value, otherwise the raw string.
pub fn extract_call_payload(result: &Value) -> Value {
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                (item.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| item.get("text").and_then(Value::as_str))
                    .flatten()
            })
        });
    match text {
        Some(t) => serde_json::from_str(t).unwrap_or_else(|_| Value::String(t.to_string())),
        None => result.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_heuristic_accepts_reads() {
        assert!(is_read_only_tool("get-issues"));
        assert!(is_read_only_tool("list_projects"));
        assert!(is_read_only_tool("Search-Resources"));
        assert!(is_read_only_tool("query-graph"));
    }

    #[test]
    fn read_only_heuristic_rejects_mutations_and_unknowns() {
        for tool in [
            "resolve-issue",
            "delete-report",
            "create-project",
            "update-control",
            "run-report",
            "rotate-service-account-secret",
            "ambiguous-tool",
        ] {
            assert!(!is_read_only_tool(tool), "{tool} must be write-gated");
        }
    }

    #[test]
    fn parse_rpc_body_handles_plain_json() {
        let v = parse_rpc_body(r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#).unwrap();
        assert_eq!(v["result"]["ok"], true);
    }

    #[test]
    fn parse_rpc_body_handles_sse_frames() {
        let body =
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[]}}\n\n";
        let v = parse_rpc_body(body).unwrap();
        assert!(v["result"]["tools"].as_array().unwrap().is_empty());
    }

    #[test]
    fn parse_rpc_body_rejects_garbage() {
        assert!(parse_rpc_body("nope").is_err());
    }

    #[test]
    fn extract_call_payload_parses_inner_json() {
        let result = serde_json::json!({
            "content": [{"type": "text", "text": "{\"operation\":\"get-issues\",\"result\":[1,2]}"}]
        });
        let payload = extract_call_payload(&result);
        assert_eq!(payload["operation"], "get-issues");
    }

    #[test]
    fn extract_call_payload_falls_back_to_raw() {
        let result = serde_json::json!({"content": [{"type": "text", "text": "plain words"}]});
        assert_eq!(
            extract_call_payload(&result),
            Value::String("plain words".into())
        );
    }
}
