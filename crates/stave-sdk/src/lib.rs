//! stave-sdk — the SDK that backs stave-cli and (future) stave-mcp.
//!
//! Modules:
//!   * `auth`    — credential + endpoint resolution chains, config file
//!   * `token`   — OAuth2 client-credentials mint + local cache
//!   * `audit`   — JSONL audit trail (see `docs/audit-trail-format.md`)
//!   * `client`  — GraphQL execution (`call_op` / `call_document`)
//!   * `ops`     — curated-operation registry + ad-hoc document classifier
//!   * `error`   — `StaveError` + `Result<T>`
//!   * `redact`  — argv + header redaction policy

#![forbid(unsafe_code)]

pub mod audit;
pub mod auth;
pub mod cel;
pub mod client;
pub mod enrich;
pub mod error;
pub mod kinds;
pub mod mcp;
pub mod ops;
pub mod redact;
pub mod stream;
pub mod token;

pub use auth::{ParamSource, ResolvedParam, ResolvedSecret, SecretSource};
pub use client::{ACCESS_TOKEN_ENV, BASE_URL_ENV, CallOptions, Client};
pub use error::{Result, StaveError};
pub use kinds::{KindSpec, all_kinds, extract_items, kind_spec};
pub use mcp::{McpClient, McpTool};
pub use ops::{DocumentMeta, OpType, OperationDoc, classify_document};

pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");
