//! MCP client for iru's published MCP server.
//!
//! iru exposes its Enterprise API surface as MCP tools at
//! `https://<subdomain>.connect.iru.com/mcp-server/connector/kandji/tools`
//! (streamable-HTTP transport, JSON-RPC 2.0). Authentication is two
//! headers from the token's one-time MCP configuration:
//! `X-API-Key` (`sk_live:`-prefixed) and `X-MCP-Profile`.
//!
//! stave operates this server as a *client* — `stave mcp tools`,
//! `stave mcp call <tool>` — so the audit trail captures MCP usage
//! alongside REST usage. The same write-guard posture applies: tools
//! whose names are not read-shaped require an explicit write opt-in.
//!
//! Verified against the live server 2026-07-15: `kandji-mcp` v3.4.4,
//! 131 tools, responses delivered as `text/event-stream` frames each
//! carrying one `data: <json-rpc>` line.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::auth;
use crate::error::{StaveError, Result};

/// MCP protocol version stave speaks. The live iru server
/// negotiated this version at scaffold time.
pub const PROTOCOL_VERSION: &str = "2025-03-26";

/// Derive the default MCP server URL from a tenant subdomain.
pub fn default_url(subdomain: &str) -> String {
    format!("https://{subdomain}.connect.iru.com/mcp-server/connector/kandji/tools")
}

/// True when an MCP tool name is read-shaped and therefore exempt from
/// the write-guard. The iru MCP tool vocabulary uses `get-*` and
/// `list-*` prefixes for reads; everything else (create/update/delete/
/// erase/lock/…) mutates tenant state or triggers device actions.
pub fn is_read_only_tool(name: &str) -> bool {
    name.starts_with("get-") || name.starts_with("list-")
}

/// Resolved MCP credentials + endpoint.
#[derive(Clone, Debug)]
pub struct McpCredentials {
    pub url: String,
    pub api_key: String,
    pub profile: String,
    /// Where the api key came from (audit signal).
    pub api_key_source: auth::TokenSource,
}

/// Resolve MCP credentials: api key (env → keyring → config), profile
/// (env → config), URL (env → config → derived from the subdomain
/// chain). Errors name every missing layer.
pub fn resolve_credentials() -> Result<McpCredentials> {
    let key = auth::resolve_mcp_api_key()?.ok_or_else(|| {
        StaveError::Auth(format!(
            "no MCP API key found. Set {env}=<sk_live:...>, run \
             `stave mcp login --stdin` to store one in the platform \
             keyring, or write `[mcp] api_key = \"<value>\"` to the config \
             file. The key comes from the one-time MCP configuration shown \
             when an MCP-enabled API token is created in iru Access.",
            env = auth::MCP_API_KEY_ENV
        ))
    })?;
    let profile = auth::resolve_mcp_profile()?.ok_or_else(|| {
        StaveError::Auth(format!(
            "no MCP profile found. Set {env}=<hex-profile>, or persist it \
             via `stave mcp login --profile <hex-profile>`. The profile \
             is the X-MCP-Profile value from the token's MCP configuration.",
            env = auth::MCP_PROFILE_ENV
        ))
    })?;
    let url = match auth::resolve_mcp_url()? {
        Some(u) => u.value,
        None => {
            let subdomain = auth::resolve_subdomain(None)?.ok_or_else(|| {
                StaveError::Auth(
                    "no MCP URL configured and no subdomain to derive it from. \
                     Set the subdomain (`stave auth login --subdomain <name>`) \
                     or the URL directly (`stave config set mcp.url <url>`)."
                        .into(),
                )
            })?;
            default_url(&subdomain.value)
        }
    };
    Ok(McpCredentials {
        url,
        api_key: key.token,
        profile: profile.value,
        api_key_source: key.source,
    })
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
    creds: McpCredentials,
}

impl McpClient {
    /// Build a client from resolved credentials.
    pub fn new(creds: McpCredentials) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| StaveError::Network(e.to_string()))?;
        Ok(Self { http, creds })
    }

    /// Resolve credentials from the chains and build a client.
    pub fn from_env() -> Result<Self> {
        Self::new(resolve_credentials()?)
    }

    pub fn url(&self) -> &str {
        &self.creds.url
    }

    pub fn api_key_source(&self) -> auth::TokenSource {
        self.creds.api_key_source
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

    /// JSON-RPC `tools/list`. The iru server returns the full tool set
    /// in one page.
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

    /// POST one JSON-RPC request and parse the response, which the iru
    /// server delivers either as `application/json` or as an SSE frame
    /// (`text/event-stream` with `data: <json>` lines).
    async fn rpc(&self, method: &str, params: Value, id: u64) -> Result<Value> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let response = self
            .http
            .post(&self.creds.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("X-API-Key", &self.creds.api_key)
            .header("X-MCP-Profile", &self.creds.profile)
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
/// `content` array. The iru server wraps its REST envelope as one
/// `{"type": "text", "text": "<json>"}` item; when the text parses as
/// JSON we return the parsed value, otherwise the raw string.
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
    fn default_url_embeds_subdomain() {
        assert_eq!(
            default_url("accuhive"),
            "https://accuhive.connect.iru.com/mcp-server/connector/kandji/tools"
        );
    }

    #[test]
    fn read_only_heuristic_accepts_reads() {
        assert!(is_read_only_tool("get-devices"));
        assert!(is_read_only_tool("get-settings-licensing"));
        assert!(is_read_only_tool("list-apple-ade-token-devices"));
    }

    #[test]
    fn read_only_heuristic_rejects_mutations() {
        for tool in [
            "erase-device",
            "delete-blueprint",
            "create-tag",
            "update-device",
            "lock-device",
            "restart-device",
            "blank-push",
            "export-prism-data",
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
            "content": [{"type": "text", "text": "{\"operation\":\"get-devices\",\"result\":[1,2]}"}]
        });
        let payload = extract_call_payload(&result);
        assert_eq!(payload["operation"], "get-devices");
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
