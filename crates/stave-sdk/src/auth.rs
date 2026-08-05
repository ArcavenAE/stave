//! Credential and endpoint resolution chains, plus config handling
//! for the OAuth2 client-credentials flow.
//!
//! All chains are instantiations of the `val-resolution-chain` bedrock
//! pattern (see charter.md and `.claude/rules/cli-philosophy.md`):
//!
//! * **Client-ID chain** — flag → env → config → error.
//! * **Client-secret chain** — env → keyring → config → error. The
//!   keyring is the intended home (`stave auth login` puts it there);
//!   config is a discouraged fallback for headless machines.
//! * **API-endpoint chain** — flag → env → config → *derived from the
//!   minted token's data-center claim* → error. The endpoint is
//!   constant for the life of a tenant, so it resolves through a
//!   chain instead of being a required flag — and unlike the earlier
//!   siblings, the chain's derivation layer is real here.
//! * **Registry chain** — the container-registry password (env →
//!   keyring → config) for registry pulls.
//!
//! Minted access tokens are cached in the XDG state dir (mode 0600),
//! never in config: they are short-lived derivatives of the client
//! secret, and the state dir keeps them out of both the keyring's
//! prompt surface and the config file's plain TOML.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Result, StaveError};

pub const CLIENT_ID_ENV: &str = "STAVE_CLIENT_ID";
pub const CLIENT_SECRET_ENV: &str = "STAVE_CLIENT_SECRET";
pub const API_URL_ENV: &str = "STAVE_API_URL";
pub const TOKEN_URL_ENV: &str = "STAVE_TOKEN_URL";
pub const ALLOW_WRITE_ENV: &str = "STAVE_ALLOW_WRITE";
pub const CONFIG_ENV: &str = "STAVE_CONFIG";
pub const REGISTRY_PASSWORD_ENV: &str = "STAVE_REGISTRY_PASSWORD";
pub const KEYRING_SERVICE: &str = "stave";
pub const KEYRING_CLIENT_SECRET_USER: &str = "client-secret";
pub const KEYRING_REGISTRY_USER: &str = "registry-password";

/// Default OAuth token endpoint. Constant across commercial Wiz
/// tenants; the chain exists for gov/isolated clouds and tests.
pub const DEFAULT_TOKEN_URL: &str = "https://auth.app.wiz.io/oauth/token";

/// OAuth audience for the Wiz API.
pub const TOKEN_AUDIENCE: &str = "wiz-api";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretSource {
    Env,
    Keyring,
    Config,
}

impl SecretSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            SecretSource::Env => "env",
            SecretSource::Keyring => "keyring",
            SecretSource::Config => "config",
        }
    }
}

/// Source of a chain-resolved non-secret value. Drops `Keyring`
/// (non-secrets don't live there) and adds `Flag` for explicit
/// per-call overrides plus `Derived` for values recovered from the
/// minted token (the endpoint's data-center claim).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamSource {
    Flag,
    Env,
    Config,
    Derived,
}

impl ParamSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            ParamSource::Flag => "flag",
            ParamSource::Env => "env",
            ParamSource::Config => "config",
            ParamSource::Derived => "derived",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedSecret {
    pub value: String,
    pub source: SecretSource,
}

#[derive(Clone, Debug)]
pub struct ResolvedParam {
    pub value: String,
    pub source: ParamSource,
}

/// Resolve the service-account client ID: flag → env → config.
pub fn resolve_client_id(flag: Option<&str>) -> Result<Option<ResolvedParam>> {
    resolve_param(flag, CLIENT_ID_ENV, |c| c.auth.client_id.clone())
}

/// Resolve the service-account client secret: env → keyring → config.
///
/// Keyring backend errors (no daemon, denied access) fall through to
/// the next layer rather than surfacing as fatal — so a user with only
/// env or only a config file isn't blocked by a missing Secret
/// Service. A malformed config file IS fatal: silent failure there
/// would mask a real auth misconfiguration.
pub fn resolve_client_secret() -> Result<Option<ResolvedSecret>> {
    if let Ok(v) = std::env::var(CLIENT_SECRET_ENV) {
        if !v.is_empty() {
            return Ok(Some(ResolvedSecret {
                value: v,
                source: SecretSource::Env,
            }));
        }
    }
    if let Some(v) = read_keyring_entry(KEYRING_CLIENT_SECRET_USER) {
        return Ok(Some(ResolvedSecret {
            value: v,
            source: SecretSource::Keyring,
        }));
    }
    if let Some(v) = read_config()?.and_then(|c| c.auth.client_secret.filter(|s| !s.is_empty())) {
        return Ok(Some(ResolvedSecret {
            value: v,
            source: SecretSource::Config,
        }));
    }
    Ok(None)
}

/// Error naming every layer of the credential chain with a concrete
/// next step for each, per cli-philosophy.md.
pub fn credentials_chain_error() -> StaveError {
    StaveError::Auth(format!(
        "no Wiz service-account credentials resolved through any layer of the chain. \
         Provide both a client ID and a client secret:\n  \
         - `stave auth login`  (prompts; secret goes to the platform keyring)\n  \
         - {CLIENT_ID_ENV}=<id> + {CLIENT_SECRET_ENV}=<secret>  (per-shell)\n  \
         - `stave config set client_id <id>`  (persisted in {})\n\
         Create a service account in the Wiz portal under Settings → Access Management.",
        config_path_display()
    ))
}

/// Resolve the GraphQL API endpoint: flag → env → config. Returns
/// `Ok(None)` when no explicit source supplies a value — the caller
/// (Client construction) then tries the derivation layer (the minted
/// token's data-center claim) before raising a chain-naming error.
pub fn resolve_api_url(flag: Option<&str>) -> Result<Option<ResolvedParam>> {
    resolve_param(flag, API_URL_ENV, |c| c.default.api_url.clone())
}

/// Resolve the OAuth token endpoint: env → config → built-in default.
pub fn resolve_token_url() -> Result<ResolvedParam> {
    Ok(
        resolve_param(None, TOKEN_URL_ENV, |c| c.auth.token_url.clone())?.unwrap_or(
            ResolvedParam {
                value: DEFAULT_TOKEN_URL.to_string(),
                source: ParamSource::Config,
            },
        ),
    )
}

/// Build the tenant GraphQL endpoint from a data-center identifier
/// (the `dc` claim in a minted Wiz token), e.g. `example1` →
/// `https://api.example1.app.wiz.io/graphql`.
pub fn api_url_from_dc(dc: &str) -> String {
    format!("https://api.{dc}.app.wiz.io/graphql")
}

/// True when write operations are allowed without a per-call
/// `--allow-write`. Walks env (`STAVE_ALLOW_WRITE`, any of
/// `1`/`true`/`yes`) → config (`[default] allow_writes = true`).
pub fn writes_allowed_by_default() -> Result<bool> {
    if let Ok(v) = std::env::var(ALLOW_WRITE_ENV) {
        let v = v.trim().to_ascii_lowercase();
        if v == "1" || v == "true" || v == "yes" {
            return Ok(true);
        }
    }
    Ok(read_config()?
        .map(|c| c.default.allow_writes.unwrap_or(false))
        .unwrap_or(false))
}

/// Resolve the container-registry password: env → keyring → config.
pub fn resolve_registry_password() -> Result<Option<ResolvedSecret>> {
    if let Ok(v) = std::env::var(REGISTRY_PASSWORD_ENV) {
        if !v.is_empty() {
            return Ok(Some(ResolvedSecret {
                value: v,
                source: SecretSource::Env,
            }));
        }
    }
    if let Some(v) = read_keyring_entry(KEYRING_REGISTRY_USER) {
        return Ok(Some(ResolvedSecret {
            value: v,
            source: SecretSource::Keyring,
        }));
    }
    if let Some(v) = read_config()?.and_then(|c| c.registry.password.filter(|s| !s.is_empty())) {
        return Ok(Some(ResolvedSecret {
            value: v,
            source: SecretSource::Config,
        }));
    }
    Ok(None)
}

fn resolve_param(
    flag: Option<&str>,
    env_var: &str,
    from_config: impl FnOnce(&Config) -> Option<String>,
) -> Result<Option<ResolvedParam>> {
    if let Some(v) = flag {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Ok(Some(ResolvedParam {
                value: trimmed.to_string(),
                source: ParamSource::Flag,
            }));
        }
    }
    if let Ok(v) = std::env::var(env_var) {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Ok(Some(ResolvedParam {
                value: trimmed.to_string(),
                source: ParamSource::Env,
            }));
        }
    }
    if let Some(cfg) = read_config()? {
        if let Some(v) = from_config(&cfg) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Ok(Some(ResolvedParam {
                    value: trimmed.to_string(),
                    source: ParamSource::Config,
                }));
            }
        }
    }
    Ok(None)
}

fn read_keyring_entry(user: &str) -> Option<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, user).ok()?;
    entry.get_password().ok()
}

/// Store the client secret in the platform keyring, replacing any
/// existing entry.
pub fn store_client_secret(secret: &str) -> Result<()> {
    store_keyring_entry(KEYRING_CLIENT_SECRET_USER, secret)
}

/// Read the client secret from the keyring (`None` = absent/unavailable).
pub fn read_client_secret_keyring() -> Option<String> {
    read_keyring_entry(KEYRING_CLIENT_SECRET_USER)
}

/// Delete the client-secret keyring entry (`Ok(false)` = nothing there).
pub fn delete_client_secret_keyring() -> Result<bool> {
    delete_keyring_entry(KEYRING_CLIENT_SECRET_USER)
}

/// Store the registry password in the platform keyring.
pub fn store_registry_password(secret: &str) -> Result<()> {
    store_keyring_entry(KEYRING_REGISTRY_USER, secret)
}

/// Read the registry password from the keyring (`None` = absent/unavailable).
pub fn read_registry_keyring() -> Option<String> {
    read_keyring_entry(KEYRING_REGISTRY_USER)
}

/// Delete the registry-password keyring entry (`Ok(false)` = nothing there).
pub fn delete_registry_keyring() -> Result<bool> {
    delete_keyring_entry(KEYRING_REGISTRY_USER)
}

fn store_keyring_entry(user: &str, secret: &str) -> Result<()> {
    if secret.is_empty() {
        return Err(StaveError::Auth("value must not be empty".into()));
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, user)
        .map_err(|e| StaveError::Auth(format!("keyring open: {e}")))?;
    entry
        .set_password(secret)
        .map_err(|e| StaveError::Auth(format!("keyring write: {e}")))?;
    Ok(())
}

fn delete_keyring_entry(user: &str) -> Result<bool> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, user)
        .map_err(|e| StaveError::Auth(format!("keyring open: {e}")))?;
    match entry.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(StaveError::Auth(format!("keyring delete: {e}"))),
    }
}

/// Configuration loaded from `~/.config/stave/config.toml` (or the
/// path in `STAVE_CONFIG`). The struct is `#[serde(default)]` so
/// future sections can be added without breaking parsing.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    #[serde(skip_serializing_if = "AuthConfig::is_empty")]
    pub auth: AuthConfig,
    #[serde(skip_serializing_if = "DefaultConfig::is_empty")]
    pub default: DefaultConfig,
    #[serde(skip_serializing_if = "RegistryConfig::is_empty")]
    pub registry: RegistryConfig,
    #[serde(skip_serializing_if = "McpConfig::is_empty")]
    pub mcp: McpConfig,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct McpConfig {
    /// MCP endpoint override. When unset, the hosted default applies
    /// (`https://mcp.app.wiz.io`). Auth rides the same OAuth bearer
    /// token as the GraphQL API — no separate MCP credential.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl McpConfig {
    fn is_empty(&self) -> bool {
        self.url.is_none()
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Service-account client ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Service-account client secret. Prefer the keyring
    /// (`stave auth login`) over storing this in plain TOML.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// OAuth token endpoint override (gov/isolated clouds, tests).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
}

impl AuthConfig {
    fn is_empty(&self) -> bool {
        self.client_id.is_none() && self.client_secret.is_none() && self.token_url.is_none()
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct DefaultConfig {
    /// Tenant GraphQL endpoint — `https://api.<region>.app.wiz.io/graphql`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
    /// Standing opt-in for write (mutation) operations. Defaults to
    /// false — stave is read-only against the tenant unless the
    /// caller passes `--allow-write` or sets this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_writes: Option<bool>,
}

impl DefaultConfig {
    fn is_empty(&self) -> bool {
        self.api_url.is_none() && self.allow_writes.is_none()
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RegistryConfig {
    /// Registry hostname (e.g. `wizio.azurecr.io`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Registry username (embeds the tenant ID — tenant-identifying).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Registry password. Prefer the keyring (`stave registry login`)
    /// over storing this in plain TOML.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl RegistryConfig {
    fn is_empty(&self) -> bool {
        self.host.is_none() && self.username.is_none() && self.password.is_none()
    }
}

/// Resolve the config file path. `STAVE_CONFIG` overrides; otherwise
/// the XDG config dir + `stave/config.toml` is used. Returns `None`
/// only when neither override nor a home directory is discoverable.
pub fn config_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(CONFIG_ENV) {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    dirs::config_dir().map(|d| d.join("stave").join("config.toml"))
}

pub(crate) fn config_path_display() -> String {
    config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/.config/stave/config.toml".into())
}

/// Read and parse the config file. Returns:
///   * `Ok(Some(cfg))` — file present, parsed
///   * `Ok(None)`      — file absent, or no discoverable config path
///   * `Err(...)`      — file present but malformed (TOML parse failed)
pub fn read_config() -> Result<Option<Config>> {
    let Some(path) = config_path() else {
        return Ok(None);
    };
    let body = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(StaveError::Auth(format!(
                "read config {}: {e}",
                path.display()
            )));
        }
    };
    let parsed: Config = toml::from_str(&body)
        .map_err(|e| StaveError::Auth(format!("parse config {}: {e}", path.display())))?;
    Ok(Some(parsed))
}

/// Read-merge-write the config file. Loads the existing config (or a
/// fresh default), applies `mutate`, then writes the result to disk —
/// preserving every section the caller did not touch. Creates the
/// parent directory if missing. Returns the path that was written.
pub fn write_config(mutate: impl FnOnce(&mut Config)) -> Result<PathBuf> {
    let path =
        config_path().ok_or_else(|| StaveError::Auth("no discoverable config path".into()))?;
    let mut cfg = read_config()?.unwrap_or_default();
    mutate(&mut cfg);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            StaveError::Auth(format!("create config dir {}: {e}", parent.display()))
        })?;
    }
    let body = toml::to_string_pretty(&cfg)
        .map_err(|e| StaveError::Auth(format!("serialize config: {e}")))?;
    std::fs::write(&path, body)
        .map_err(|e| StaveError::Auth(format!("write config {}: {e}", path.display())))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_source_as_str_is_stable() {
        assert_eq!(SecretSource::Env.as_str(), "env");
        assert_eq!(SecretSource::Keyring.as_str(), "keyring");
        assert_eq!(SecretSource::Config.as_str(), "config");
    }

    #[test]
    fn param_source_as_str_is_stable() {
        assert_eq!(ParamSource::Flag.as_str(), "flag");
        assert_eq!(ParamSource::Env.as_str(), "env");
        assert_eq!(ParamSource::Config.as_str(), "config");
        assert_eq!(ParamSource::Derived.as_str(), "derived");
    }

    #[test]
    fn api_url_from_dc_builds_graphql_endpoint() {
        assert_eq!(
            api_url_from_dc("example1"),
            "https://api.example1.app.wiz.io/graphql"
        );
    }

    #[test]
    fn parse_full_config() {
        let body = r#"
[auth]
client_id = "svc-abc"
token_url = "https://auth.example.test/oauth/token"

[default]
api_url = "https://api.example.test/graphql"
allow_writes = false

[registry]
host = "registry.example.test"
username = "repo-user"
"#;
        let cfg: Config = toml::from_str(body).expect("parse");
        assert_eq!(cfg.auth.client_id.as_deref(), Some("svc-abc"));
        assert_eq!(
            cfg.auth.token_url.as_deref(),
            Some("https://auth.example.test/oauth/token")
        );
        assert_eq!(
            cfg.default.api_url.as_deref(),
            Some("https://api.example.test/graphql")
        );
        assert_eq!(cfg.default.allow_writes, Some(false));
        assert_eq!(cfg.registry.username.as_deref(), Some("repo-user"));
    }

    #[test]
    fn parse_empty_config() {
        let cfg: Config = toml::from_str("").expect("parse");
        assert_eq!(cfg.auth.client_id, None);
        assert_eq!(cfg.default.api_url, None);
        assert_eq!(cfg.registry.username, None);
    }

    #[test]
    fn parse_unrelated_sections_ok() {
        // Future sections must not break parsing of known sections.
        let body = r#"
[future_section]
flag = true

[auth]
client_id = "svc-xyz"
"#;
        let cfg: Config = toml::from_str(body).expect("parse");
        assert_eq!(cfg.auth.client_id.as_deref(), Some("svc-xyz"));
    }

    #[test]
    fn parse_malformed_errors() {
        let body = "this is = not = toml";
        let result: std::result::Result<Config, _> = toml::from_str(body);
        assert!(result.is_err());
    }

    #[test]
    fn serialize_skips_empty_sections() {
        let cfg = Config::default();
        let body = toml::to_string_pretty(&cfg).expect("serialize");
        assert!(
            !body.contains("[auth]"),
            "empty auth section should be skipped: {body:?}"
        );
        assert!(
            !body.contains("[default]"),
            "empty default section should be skipped: {body:?}"
        );
        assert!(
            !body.contains("[registry]"),
            "empty registry section should be skipped: {body:?}"
        );
    }

    #[test]
    fn serialize_round_trip_default_only() {
        let mut cfg = Config::default();
        cfg.default.api_url = Some("https://api.example.test/graphql".into());
        let body = toml::to_string_pretty(&cfg).expect("serialize");
        assert!(body.contains("[default]"), "want [default] in {body:?}");
        assert!(
            body.contains("api_url = \"https://api.example.test/graphql\""),
            "want api_url in {body:?}"
        );
        assert!(
            !body.contains("[auth]"),
            "empty auth section should be skipped: {body:?}"
        );
    }
}
