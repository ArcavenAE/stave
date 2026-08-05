//! SDK client. Spec-aware HTTP execution against the iru Endpoint
//! Management API.
//!
//! Two safety properties live here:
//!
//! * **Subdomain chain** — the tenant subdomain resolves through
//!   flag → env → config (see `auth::resolve_subdomain`) and is
//!   substituted into the spec's templated server URL.
//! * **Write-guard** — stave is read-only by default. Any
//!   non-GET operation errors with [`StaveError::WriteGuard`]
//!   unless the caller opted in (`CallOptions::allow_write`, driven
//!   by `--allow-write`, `STAVE_ALLOW_WRITE`, or
//!   `[default] allow_writes = true`). The live tenant is an in-use
//!   production instance; the guard makes mutation a deliberate act.

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;
use uuid::Uuid;

use crate::audit::{AuditOp, Outcome, Outcomes, Span, shape_hash};
use crate::auth::{self, ParamSource, TokenSource};
use crate::error::{StaveError, Result};
use crate::spec::{OperationMeta, registry};

/// Override the base URL the Client builds requests against. Intended
/// for testing (wiremock, integration harnesses) and dev work — not
/// a supported production knob. When set, the subdomain chain is not
/// consulted.
pub const BASE_URL_ENV: &str = "STAVE_BASE_URL";

/// Read `STAVE_BASE_URL`; treat empty string as unset so a stray
/// `export STAVE_BASE_URL=` in a shell doesn't silently break
/// production calls.
fn base_url_from_env() -> Option<String> {
    std::env::var(BASE_URL_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    auth_source: Option<TokenSource>,
    subdomain_source: Option<ParamSource>,
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
    /// (`list`, `get`, `search`, `api`); leave `None` for the legacy
    /// shape if you're driving the SDK directly without a CLI verb.
    pub verb_phase: Option<&'static str>,
    /// Per-record synthesis keys for the v2 audit. Typically the
    /// kind's primary key field, e.g. `["device_id"]`.
    pub synthesis_keys: Vec<String>,
    /// Provenance of each path parameter the caller resolved through
    /// the val-resolution-chain (flag → env → config). Recorded in the
    /// audit emission as the `path_params_source` sibling of
    /// `operation`. Params not present here are treated as
    /// flag/explicit and produce no source entry.
    pub path_params_source: BTreeMap<String, ParamSource>,
    /// Opt-in for mutating (non-GET) operations. When false (the
    /// default), the write-guard rejects the call before any request
    /// is sent. See `auth::writes_allowed_by_default` for the standing
    /// opt-in chain the CLI resolves before setting this.
    pub allow_write: bool,
}

impl Client {
    /// Resolve token (env → keyring → config) and subdomain
    /// (env → config) chains and construct a Client. Honors
    /// `STAVE_BASE_URL` when set (testing convenience) — the
    /// subdomain chain is skipped entirely in that case.
    pub fn from_env() -> Result<Self> {
        Self::from_env_with_subdomain(None)
    }

    /// Same as [`Client::from_env`] but with an explicit per-call
    /// subdomain override (the CLI's `--subdomain` flag) as the first
    /// layer of the subdomain chain.
    pub fn from_env_with_subdomain(subdomain_flag: Option<&str>) -> Result<Self> {
        let resolved = auth::resolve()?;
        if let Some(override_url) = base_url_from_env() {
            return Self::build(&resolved.token, Some(resolved.source), override_url, None);
        }
        let (base_url, subdomain_source) = resolve_base_url(subdomain_flag)?;
        Self::build(
            &resolved.token,
            Some(resolved.source),
            base_url,
            Some(subdomain_source),
        )
    }

    /// Construct with an explicit token (skips the token resolver
    /// chain; the subdomain chain still applies). The audit trail
    /// records `auth_source` as `None` for this path. Honors
    /// `STAVE_BASE_URL` when set.
    pub fn with_token(token: &str) -> Result<Self> {
        if let Some(override_url) = base_url_from_env() {
            return Self::build(token, None, override_url, None);
        }
        let (base_url, subdomain_source) = resolve_base_url(None)?;
        Self::build(token, None, base_url, Some(subdomain_source))
    }

    /// Construct with an explicit token and an explicit base URL.
    /// Bypasses the auth resolver chain, the subdomain chain, and
    /// `STAVE_BASE_URL`. Intended for tests (wiremock, recorded
    /// fixtures) and library callers that need to point at a
    /// non-production endpoint deterministically. The audit trail
    /// records `auth_source` as `None`.
    pub fn with_base_url(token: &str, base_url: impl Into<String>) -> Result<Self> {
        Self::build(token, None, base_url.into(), None)
    }

    fn build(
        token: &str,
        auth_source: Option<TokenSource>,
        base_url: String,
        subdomain_source: Option<ParamSource>,
    ) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let auth = format!("Bearer {token}");
        let mut auth_value = HeaderValue::from_str(&auth)
            .map_err(|_| StaveError::Auth("invalid token characters".into()))?;
        auth_value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, auth_value);
        headers.insert(
            reqwest::header::USER_AGENT,
            HeaderValue::from_static(concat!("stave/", env!("CARGO_PKG_VERSION"))),
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| StaveError::Network(e.to_string()))?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_source,
            subdomain_source,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn auth_source(&self) -> Option<TokenSource> {
        self.auth_source
    }

    pub fn subdomain_source(&self) -> Option<ParamSource> {
        self.subdomain_source
    }

    /// Execute an operation by ID. `params` is a JSON object whose
    /// fields are mapped to path / query parameters per the spec.
    /// Body content goes under the `body` key.
    ///
    /// Mutating operations (anything non-GET) are rejected with
    /// [`StaveError::WriteGuard`] unless `opts.allow_write` is set.
    pub async fn call_op(
        &self,
        operation_id: &str,
        params: &Value,
        opts: CallOptions,
    ) -> Result<Value> {
        let op = registry().find(operation_id)?.clone();

        if op.is_mutating() && !opts.allow_write {
            return Err(StaveError::WriteGuard {
                operation: op.id.clone(),
                method: op.method.as_str().to_string(),
            });
        }

        let trace_id = opts.trace_id.unwrap_or_else(Uuid::now_v7);
        let mut span = Span::start(trace_id);
        span.auth_source = self.auth_source;
        span.subdomain_source = self.subdomain_source;
        if let Some(phase) = opts.verb_phase {
            span = span.with_verb_phase(phase);
        }
        if !opts.synthesis_keys.is_empty() {
            span = span.with_synthesis_keys(opts.synthesis_keys.clone());
        }
        if !opts.path_params_source.is_empty() {
            span = span.with_path_params_source(opts.path_params_source.clone());
        }

        if opts.no_audit {
            self.execute_silent(&op, params, span).await
        } else {
            let audit_op = audit_op_from(&op, params);
            span = span.with_op(audit_op);
            self.execute_audited(&op, params, span).await
        }
    }

    async fn execute_audited(
        &self,
        op: &OperationMeta,
        params: &Value,
        span: Span,
    ) -> Result<Value> {
        let result = self.send(op, params).await;
        let outcomes = match &result {
            Ok((value, status)) => Outcomes {
                outcome: Outcome::Ok,
                status: Some(*status),
                size_bytes: Some(estimated_size(value)),
                items_returned: count_items(value),
                next_cursor: extract_cursor(value),
                shape_hash: Some(shape_hash(value)),
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
        result.map(|(value, _)| value)
    }

    async fn execute_silent(
        &self,
        op: &OperationMeta,
        params: &Value,
        span: Span,
    ) -> Result<Value> {
        let result = self.send(op, params).await;
        span.finish(Outcomes {
            outcome: Outcome::RedactedBlock,
            status: result.as_ref().ok().map(|(_, s)| *s),
            size_bytes: None,
            items_returned: None,
            next_cursor: None,
            shape_hash: None,
            redacted_fields: vec!["operation".to_string(), "response".to_string()],
        });
        result.map(|(value, _)| value)
    }

    async fn send(&self, op: &OperationMeta, params: &Value) -> Result<(Value, u16)> {
        let url = build_url(&self.base_url, op, params)?;
        let mut req = self.http.request(op.method.as_reqwest(), &url);

        // Query parameters
        let mut query: Vec<(String, String)> = Vec::new();
        for q in &op.query_params {
            if let Some(v) = params.get(q) {
                query.push((q.clone(), value_to_query_string(v)));
            }
        }
        if !query.is_empty() {
            req = req.query(&query);
        }

        // Body for write methods
        if op.has_body {
            if let Some(body) = params.get("body") {
                req = req.json(body);
            }
        }

        let response = req
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

        let value = if body_text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&body_text).unwrap_or_else(|_| Value::String(body_text.clone()))
        };
        Ok((value, status_code))
    }
}

/// Walk the subdomain + region chains and produce the tenant base URL.
/// Errors with a chain-naming message when no subdomain resolves —
/// every layer and its concrete next step, per cli-philosophy.md.
fn resolve_base_url(subdomain_flag: Option<&str>) -> Result<(String, ParamSource)> {
    let resolved = auth::resolve_subdomain(subdomain_flag)?.ok_or_else(|| {
        let config_path = auth::config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/.config/stave/config.toml".into());
        StaveError::Auth(format!(
            "no tenant subdomain resolved through any layer of the chain. Set one of:\n  \
             - --subdomain <name>  (per-call override)\n  \
             - {env}=<name>  (per-shell default)\n  \
             - `stave auth login --subdomain <name>`  (persisted in {config_path})\n  \
             - `stave config set subdomain <name>`  (same persistence, no token write)\n\
             The subdomain is the tenant name in your iru API URL: https://<subdomain>.api.kandji.io",
            env = auth::SUBDOMAIN_ENV,
        ))
    })?;
    let region = auth::resolve_region(None)?;
    let base_url =
        registry().base_url_for(&resolved.value, region.as_ref().map(|r| r.value.as_str()))?;
    Ok((base_url, resolved.source))
}

fn build_url(base: &str, op: &OperationMeta, params: &Value) -> Result<String> {
    let mut path = op.path_template.clone();
    for name in &op.path_params {
        let value = params
            .get(name)
            .ok_or_else(|| StaveError::MissingParam(name.clone(), op.id.clone()))?;
        let s = value_to_path_string(value).ok_or_else(|| {
            StaveError::InvalidParam(name.clone(), "path params must be scalar".into())
        })?;
        let placeholder = format!("{{{name}}}");
        path = path.replace(&placeholder, &s);
    }
    Ok(format!("{base}{path}"))
}

fn value_to_path_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(urlencoding::encode(s).into_owned()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn value_to_query_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => v.to_string(),
    }
}

fn audit_op_from(op: &OperationMeta, params: &Value) -> AuditOp {
    let mut path_params = serde_json::Map::new();
    for name in &op.path_params {
        if let Some(v) = params.get(name) {
            path_params.insert(name.clone(), v.clone());
        }
    }
    let mut query_params = serde_json::Map::new();
    for name in &op.query_params {
        if let Some(v) = params.get(name) {
            query_params.insert(name.clone(), v.clone());
        }
    }
    AuditOp {
        id: op.id.clone(),
        method: op.method.as_str().to_string(),
        url_template: op.path_template.clone(),
        path_params: Value::Object(path_params),
        query_params: Value::Object(query_params),
    }
}

fn estimated_size(v: &Value) -> usize {
    serde_json::to_string(v).map(|s| s.len()).unwrap_or(0)
}

/// If the response is an array or has a top-level array under common
/// pagination keys, return its length. iru endpoints return either
/// bare arrays (`GET /devices`) or DRF-style
/// `{count, next, previous, results}` wrappers.
fn count_items(v: &Value) -> Option<usize> {
    if let Some(arr) = v.as_array() {
        return Some(arr.len());
    }
    for key in ["results", "items", "data", "devices", "detections"] {
        if let Some(arr) = v.get(key).and_then(|x| x.as_array()) {
            return Some(arr.len());
        }
    }
    None
}

/// Extract a pagination cursor. iru uses DRF-style `next` (full URL)
/// and cursor-based `cursor`/`after` on newer endpoints.
fn extract_cursor(v: &Value) -> Option<String> {
    for key in ["next", "next_cursor", "cursor", "after", "next_page"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_base_url_replaces_spec_default() {
        let client =
            Client::with_base_url("fake-token", "http://127.0.0.1:9999/v1").expect("client built");
        assert_eq!(client.base_url(), "http://127.0.0.1:9999/v1");
    }

    #[test]
    fn with_base_url_trims_trailing_slash() {
        // Path templates start with `/`; a trailing slash on the base
        // would produce `//api/v1/...`. Override trims to match.
        let client =
            Client::with_base_url("fake-token", "http://127.0.0.1:9999/v1/").expect("client built");
        assert_eq!(client.base_url(), "http://127.0.0.1:9999/v1");
    }

    #[test]
    fn with_base_url_records_no_auth_source() {
        // Explicit-token paths intentionally omit `auth_source` from
        // the audit so `with_token` and `with_base_url` look identical
        // to a miner — they're both "I provided the token directly".
        let client =
            Client::with_base_url("fake-token", "http://127.0.0.1:9999").expect("client built");
        assert!(client.auth_source().is_none());
        assert!(client.subdomain_source().is_none());
    }

    #[test]
    fn count_items_handles_drf_wrapper() {
        let v = serde_json::json!({"count": 2, "next": null, "previous": null,
            "results": [{"id": 1}, {"id": 2}]});
        assert_eq!(count_items(&v), Some(2));
    }

    #[test]
    fn count_items_handles_bare_array() {
        let v = serde_json::json!([{"device_id": "a"}]);
        assert_eq!(count_items(&v), Some(1));
    }

    #[test]
    fn extract_cursor_reads_drf_next_url() {
        let v = serde_json::json!({"next": "https://x.api.kandji.io/api/v1/users?cursor=abc",
            "results": []});
        assert!(extract_cursor(&v).unwrap().contains("cursor=abc"));
    }
}
