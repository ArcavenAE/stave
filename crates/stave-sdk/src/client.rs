//! SDK client. GraphQL execution against the Wiz API.
//!
//! Two safety properties live here:
//!
//! * **Endpoint chain** — the tenant GraphQL endpoint resolves through
//!   flag → env → config → *derived from the minted token's
//!   data-center claim* (see `auth::resolve_api_url` + `token::dc_claim`).
//! * **Write-guard** — stave is read-only by default. Any mutation
//!   (curated `OpType::Mutation`, or an ad-hoc document containing a
//!   mutation/subscription) errors with [`StaveError::WriteGuard`]
//!   unless the caller opted in (`CallOptions::allow_write`, driven by
//!   `--allow-write`, `STAVE_ALLOW_WRITE`, or
//!   `[default] allow_writes = true`). The live tenant is production;
//!   the guard makes mutation a deliberate act.

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::audit::{AuditOp, Outcome, Outcomes, Span, shape_hash};
use crate::auth::{self, ParamSource, SecretSource};
use crate::error::{Result, StaveError};
use crate::ops::{self, OpType};
use crate::token;

/// Override the base URL the Client posts GraphQL to. Intended for
/// testing (wiremock, integration harnesses) and dev work — not a
/// supported production knob. When set, the endpoint chain is not
/// consulted.
pub const BASE_URL_ENV: &str = "STAVE_BASE_URL";

/// Provide a pre-minted access token directly, skipping the
/// client-credentials mint. Useful for CI that already holds a token
/// and for the wiremock harness.
pub const ACCESS_TOKEN_ENV: &str = "STAVE_ACCESS_TOKEN";

/// Read `STAVE_BASE_URL`; treat empty string as unset so a stray
/// `export STAVE_BASE_URL=` in a shell doesn't silently break
/// production calls.
fn base_url_from_env() -> Option<String> {
    std::env::var(BASE_URL_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn access_token_from_env() -> Option<String> {
    std::env::var(ACCESS_TOKEN_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    api_url: String,
    auth_source: Option<SecretSource>,
    api_url_source: Option<ParamSource>,
}

#[derive(Clone, Debug, Default)]
pub struct CallOptions {
    /// Trace ID to attach to the audit span. If `None`, a fresh UUIDv7
    /// is generated. Pass an existing trace_id to group multiple calls
    /// (e.g. paginated reads) under one logical operation.
    pub trace_id: Option<Uuid>,
    /// If true, the SDK records a stub audit line marked
    /// `result=redacted_block` instead of the operation detail.
    pub no_audit: bool,
    /// Verb-phase tag for the audit emission. CLI primitives set this
    /// (`list`, `get`, `search`, `api`).
    pub verb_phase: Option<&'static str>,
    /// Per-record synthesis keys for the v2 audit. Typically the
    /// kind's primary key field, e.g. `["id"]`.
    pub synthesis_keys: Vec<String>,
    /// Provenance of each chain-resolved parameter (recorded in the
    /// audit emission as the `path_params_source` sibling of
    /// `operation`, keeping the mining signal uniform with stave's
    /// siblings). The Client adds `_api_url` itself.
    pub path_params_source: BTreeMap<String, ParamSource>,
    /// Opt-in for mutating operations. When false (the default), the
    /// write-guard rejects the call before any request is sent. See
    /// `auth::writes_allowed_by_default` for the standing opt-in chain
    /// the CLI resolves before setting this.
    pub allow_write: bool,
}

impl Client {
    /// Resolve credentials (client ID: flag → env → config; secret:
    /// env → keyring → config), mint or reuse a cached token, and
    /// resolve the endpoint chain (flag → env → config → derived from
    /// the token's data-center claim). Honors `STAVE_BASE_URL` and
    /// `STAVE_ACCESS_TOKEN` (testing conveniences).
    pub async fn from_env() -> Result<Self> {
        Self::from_env_with_api_url(None).await
    }

    /// Same as [`Client::from_env`] but with an explicit per-call
    /// endpoint override (the CLI's `--api-url` flag) as the first
    /// layer of the endpoint chain.
    pub async fn from_env_with_api_url(api_url_flag: Option<&str>) -> Result<Self> {
        // Pre-minted token (env) short-circuits the credential chains.
        if let Some(tok) = access_token_from_env() {
            if let Some(override_url) = base_url_from_env() {
                return Self::build(&tok, None, override_url, None);
            }
            let (api_url, source) = resolve_api_url_with_derivation(api_url_flag, Some(&tok))?;
            return Self::build(&tok, None, api_url, Some(source));
        }

        let client_id = auth::resolve_client_id(None)?.ok_or_else(auth::credentials_chain_error)?;
        let client_secret =
            auth::resolve_client_secret()?.ok_or_else(auth::credentials_chain_error)?;
        let token_url = auth::resolve_token_url()?;

        let mint_http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| StaveError::Network(e.to_string()))?;
        let minted = token::cached_or_mint(
            &mint_http,
            &token_url.value,
            &client_id.value,
            &client_secret.value,
        )
        .await?;

        if let Some(override_url) = base_url_from_env() {
            return Self::build(
                &minted.access_token,
                Some(client_secret.source),
                override_url,
                None,
            );
        }
        let (api_url, source) =
            resolve_api_url_with_derivation(api_url_flag, Some(&minted.access_token))?;
        Self::build(
            &minted.access_token,
            Some(client_secret.source),
            api_url,
            Some(source),
        )
    }

    /// Construct with an explicit bearer token (skips the credential
    /// chains; the endpoint chain still applies, including derivation
    /// from this token). The audit trail records `auth_source` as
    /// `None` for this path. Honors `STAVE_BASE_URL`.
    pub fn with_token(token: &str) -> Result<Self> {
        if let Some(override_url) = base_url_from_env() {
            return Self::build(token, None, override_url, None);
        }
        let (api_url, source) = resolve_api_url_with_derivation(None, Some(token))?;
        Self::build(token, None, api_url, Some(source))
    }

    /// Construct with an explicit token and an explicit endpoint.
    /// Bypasses every chain and `STAVE_BASE_URL`. Intended for tests
    /// (wiremock, recorded fixtures) and library callers that need a
    /// non-production endpoint deterministically. The audit trail
    /// records `auth_source` as `None`.
    pub fn with_base_url(token: &str, api_url: impl Into<String>) -> Result<Self> {
        Self::build(token, None, api_url.into(), None)
    }

    fn build(
        token: &str,
        auth_source: Option<SecretSource>,
        api_url: String,
        api_url_source: Option<ParamSource>,
    ) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let auth_header = format!("Bearer {token}");
        let mut auth_value = HeaderValue::from_str(&auth_header)
            .map_err(|_| StaveError::Auth("invalid token characters".into()))?;
        auth_value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, auth_value);
        headers.insert(
            reqwest::header::USER_AGENT,
            HeaderValue::from_static(concat!("stave/", env!("CARGO_PKG_VERSION"))),
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| StaveError::Network(e.to_string()))?;

        Ok(Self {
            http,
            api_url: api_url.trim_end_matches('/').to_string(),
            auth_source,
            api_url_source,
        })
    }

    pub fn api_url(&self) -> &str {
        &self.api_url
    }

    pub fn auth_source(&self) -> Option<SecretSource> {
        self.auth_source
    }

    pub fn api_url_source(&self) -> Option<ParamSource> {
        self.api_url_source
    }

    /// Execute a curated operation by registry name. `variables` is
    /// the GraphQL variables object (`{"first": 50, "after": "..."}`).
    ///
    /// Mutations are rejected with [`StaveError::WriteGuard`] unless
    /// `opts.allow_write` is set. Returns the response's `data` value.
    pub async fn call_op(&self, name: &str, variables: &Value, opts: CallOptions) -> Result<Value> {
        let op = ops::find(name)?;

        if op.op_type == OpType::Mutation && !opts.allow_write {
            return Err(StaveError::WriteGuard {
                operation: op.name.to_string(),
                op_type: op.op_type.as_str().to_string(),
            });
        }

        self.execute(
            op.name,
            op.op_type,
            Some(op.root_field),
            op.document,
            variables,
            opts,
        )
        .await
    }

    /// Execute an ad-hoc GraphQL document (`stave api --query`). The
    /// document is parsed first: unparseable documents never reach the
    /// wire, and any mutation/subscription in the document trips the
    /// write-guard exactly like a curated mutation.
    pub async fn call_document(
        &self,
        document: &str,
        variables: &Value,
        opts: CallOptions,
    ) -> Result<Value> {
        let meta = ops::classify_document(document)?;
        let name = meta
            .operation_name
            .clone()
            .unwrap_or_else(|| "adhoc".into());
        let op_type = if meta.is_mutating {
            OpType::Mutation
        } else {
            OpType::Query
        };

        if meta.is_mutating && !opts.allow_write {
            return Err(StaveError::WriteGuard {
                operation: name,
                op_type: op_type.as_str().to_string(),
            });
        }

        self.execute(&name, op_type, None, document, variables, opts)
            .await
    }

    async fn execute(
        &self,
        op_name: &str,
        op_type: OpType,
        root_field: Option<&str>,
        document: &str,
        variables: &Value,
        opts: CallOptions,
    ) -> Result<Value> {
        let trace_id = opts.trace_id.unwrap_or_else(Uuid::now_v7);
        let mut span = Span::start(trace_id);
        span.auth_source = self.auth_source;
        span.api_url_source = self.api_url_source;
        if let Some(phase) = opts.verb_phase {
            span = span.with_verb_phase(phase);
        }
        if !opts.synthesis_keys.is_empty() {
            span = span.with_synthesis_keys(opts.synthesis_keys.clone());
        }
        let mut sources = opts.path_params_source.clone();
        if let Some(s) = self.api_url_source {
            sources.insert("_api_url".to_string(), s);
        }
        if !sources.is_empty() {
            span = span.with_path_params_source(sources);
        }

        if opts.no_audit {
            let result = self.send(document, variables).await;
            span.finish(Outcomes {
                outcome: Outcome::RedactedBlock,
                status: result.as_ref().ok().map(|r| r.status),
                size_bytes: None,
                items_returned: None,
                next_cursor: None,
                shape_hash: None,
                redacted_fields: vec!["operation".to_string(), "response".to_string()],
            });
            return result.map(|r| r.data);
        }

        span = span.with_op(AuditOp {
            id: op_name.to_string(),
            method: op_type.as_str().to_string(),
            url_template: root_field.unwrap_or("").to_string(),
            path_params: variables.clone(),
            query_params: Value::Object(serde_json::Map::new()),
        });

        let result = self.send(document, variables).await;
        let outcomes = match &result {
            Ok(sent) => Outcomes {
                outcome: Outcome::Ok,
                status: Some(sent.status),
                size_bytes: Some(estimated_size(&sent.data)),
                items_returned: count_items(&sent.data, root_field),
                next_cursor: extract_cursor(&sent.data, root_field),
                shape_hash: Some(shape_hash(&sent.data)),
                redacted_fields: vec!["authorization".to_string()],
            },
            Err(StaveError::Http { status, body }) => Outcomes {
                outcome: Outcome::HttpError,
                status: Some(*status),
                size_bytes: Some(body.len()),
                items_returned: None,
                next_cursor: None,
                shape_hash: None,
                redacted_fields: vec!["authorization".to_string()],
            },
            Err(StaveError::GraphQl { messages }) => Outcomes {
                outcome: Outcome::GraphQlError,
                status: Some(200),
                size_bytes: None,
                items_returned: Some(messages.len()),
                next_cursor: None,
                shape_hash: None,
                redacted_fields: vec!["authorization".to_string()],
            },
            Err(StaveError::Network(_)) => Outcomes {
                outcome: Outcome::NetworkError,
                status: None,
                size_bytes: None,
                items_returned: None,
                next_cursor: None,
                shape_hash: None,
                redacted_fields: vec!["authorization".to_string()],
            },
            Err(StaveError::Auth(_)) => Outcomes {
                outcome: Outcome::AuthError,
                status: None,
                size_bytes: None,
                items_returned: None,
                next_cursor: None,
                shape_hash: None,
                redacted_fields: vec!["authorization".to_string()],
            },
            Err(_) => Outcomes {
                outcome: Outcome::HttpError,
                status: None,
                size_bytes: None,
                items_returned: None,
                next_cursor: None,
                shape_hash: None,
                redacted_fields: vec!["authorization".to_string()],
            },
        };
        span.finish(outcomes);
        result.map(|r| r.data)
    }

    async fn send(&self, document: &str, variables: &Value) -> Result<SentResponse> {
        let payload = json!({
            "query": document,
            "variables": variables,
        });
        let response = self
            .http
            .post(&self.api_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| StaveError::Network(e.to_string()))?;
        let status = response.status();
        let body_text = response
            .text()
            .await
            .map_err(|e| StaveError::Network(e.to_string()))?;
        let status_code = status.as_u16();

        if !status.is_success() {
            return Err(StaveError::Http {
                status: status_code,
                body: body_text,
            });
        }

        let body: Value = serde_json::from_str(&body_text)
            .map_err(|e| StaveError::Network(format!("non-JSON GraphQL response: {e}")))?;

        if let Some(errors) = body.get("errors").and_then(Value::as_array) {
            if !errors.is_empty() {
                let messages = errors
                    .iter()
                    .map(|e| {
                        e.get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown error")
                            .to_string()
                    })
                    .collect();
                return Err(StaveError::GraphQl { messages });
            }
        }

        let data = body.get("data").cloned().unwrap_or(Value::Null);
        Ok(SentResponse {
            data,
            status: status_code,
        })
    }
}

struct SentResponse {
    data: Value,
    status: u16,
}

/// Walk the endpoint chain, adding the derivation layer: when no
/// flag/env/config source supplies a URL and a token is at hand, the
/// token's data-center claim names the tenant's region. Errors with a
/// chain-naming message per cli-philosophy.md.
fn resolve_api_url_with_derivation(
    flag: Option<&str>,
    access_token: Option<&str>,
) -> Result<(String, ParamSource)> {
    if let Some(resolved) = auth::resolve_api_url(flag)? {
        return Ok((resolved.value, resolved.source));
    }
    if let Some(dc) = access_token.and_then(token::dc_claim) {
        return Ok((auth::api_url_from_dc(&dc), ParamSource::Derived));
    }
    Err(StaveError::Auth(format!(
        "no API endpoint resolved through any layer of the chain. Set one of:\n  \
         - --api-url <url>  (per-call override)\n  \
         - {env}=<url>  (per-shell default)\n  \
         - `stave config set api_url <url>`  (persisted)\n  \
         - or let stave derive it from a minted token's data-center claim\n\
         The endpoint is shown in the Wiz portal (user profile → tenant info): \
         https://api.<region>.app.wiz.io/graphql",
        env = auth::API_URL_ENV,
    )))
}

fn estimated_size(v: &Value) -> usize {
    serde_json::to_string(v).map(|s| s.len()).unwrap_or(0)
}

/// Count the items in a GraphQL connection response. With a known
/// root field, look there; otherwise scan `data`'s top-level objects
/// for the first `nodes` array.
fn count_items(data: &Value, root_field: Option<&str>) -> Option<usize> {
    connection(data, root_field)?
        .get("nodes")
        .and_then(Value::as_array)
        .map(|a| a.len())
}

/// Extract the pagination cursor from a connection's `pageInfo` when
/// there are more pages.
fn extract_cursor(data: &Value, root_field: Option<&str>) -> Option<String> {
    let page_info = connection(data, root_field)?.get("pageInfo")?;
    if page_info.get("hasNextPage").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    page_info
        .get("endCursor")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
}

fn connection<'a>(data: &'a Value, root_field: Option<&str>) -> Option<&'a Value> {
    if let Some(field) = root_field {
        return data.get(field);
    }
    data.as_object()?
        .values()
        .find(|v| v.get("nodes").is_some_and(Value::is_array))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_base_url_replaces_chain() {
        let client =
            Client::with_base_url("fake-token", "http://127.0.0.1:9999/graphql").expect("client");
        assert_eq!(client.api_url(), "http://127.0.0.1:9999/graphql");
    }

    #[test]
    fn with_base_url_trims_trailing_slash() {
        let client =
            Client::with_base_url("fake-token", "http://127.0.0.1:9999/graphql/").expect("client");
        assert_eq!(client.api_url(), "http://127.0.0.1:9999/graphql");
    }

    #[test]
    fn with_base_url_records_no_auth_source() {
        // Explicit-token paths intentionally omit `auth_source` from
        // the audit so `with_token` and `with_base_url` look identical
        // to a miner — they're both "I provided the token directly".
        let client = Client::with_base_url("fake-token", "http://127.0.0.1:9999").expect("client");
        assert!(client.auth_source().is_none());
        assert!(client.api_url_source().is_none());
    }

    #[test]
    fn count_items_reads_connection_nodes() {
        let data = json!({"issuesV2": {"nodes": [{"id": "a"}, {"id": "b"}],
            "pageInfo": {"hasNextPage": false, "endCursor": null}}});
        assert_eq!(count_items(&data, Some("issuesV2")), Some(2));
    }

    #[test]
    fn count_items_scans_without_root_field() {
        let data = json!({"projects": {"nodes": [{"id": "p"}],
            "pageInfo": {"hasNextPage": false}}});
        assert_eq!(count_items(&data, None), Some(1));
    }

    #[test]
    fn extract_cursor_only_when_more_pages() {
        let more = json!({"issuesV2": {"nodes": [],
            "pageInfo": {"hasNextPage": true, "endCursor": "abc"}}});
        assert_eq!(
            extract_cursor(&more, Some("issuesV2")).as_deref(),
            Some("abc")
        );
        let done = json!({"issuesV2": {"nodes": [],
            "pageInfo": {"hasNextPage": false, "endCursor": "abc"}}});
        assert_eq!(extract_cursor(&done, Some("issuesV2")), None);
    }
}
