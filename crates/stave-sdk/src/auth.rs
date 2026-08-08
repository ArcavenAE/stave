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
//! * **Profile chain** — `--profile` → `STAVE_PROFILE` → `[default]
//!   profile` → none. A profile names one service account; when one is
//!   active it supplies the client ID, the keyring account, and any
//!   endpoint override, sitting *above* `[auth]` rather than replacing
//!   it so an install predating profiles is untouched.
//!
//! The profile layer carries two refusals that are safety properties
//! rather than ergonomics, and no surveyed CLI (aws, gcloud, spacectl,
//! gh, kubectl) has either: a provision-plane profile can never be
//! reached through the stored default, and a credential may only be
//! used by the binary whose plane it belongs to. See
//! [`select_profile`] and `docs/design/profiles-and-credential-selection.md`.
//!
//! Minted access tokens are cached in the XDG state dir (mode 0600),
//! never in config: they are short-lived derivatives of the client
//! secret, and the state dir keeps them out of both the keyring's
//! prompt surface and the config file's plain TOML.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Result, StaveError};

pub const CLIENT_ID_ENV: &str = "STAVE_CLIENT_ID";
pub const CLIENT_SECRET_ENV: &str = "STAVE_CLIENT_SECRET";
pub const API_URL_ENV: &str = "STAVE_API_URL";
pub const TOKEN_URL_ENV: &str = "STAVE_TOKEN_URL";
pub const CONFIG_ENV: &str = "STAVE_CONFIG";
pub const PROFILE_ENV: &str = "STAVE_PROFILE";
pub const REGISTRY_PASSWORD_ENV: &str = "STAVE_REGISTRY_PASSWORD";
pub const KEYRING_SERVICE: &str = "stave";
pub const KEYRING_CLIENT_SECRET_USER: &str = "client-secret";
pub const KEYRING_REGISTRY_USER: &str = "registry-password";

/// Keyring account for a named profile's client secret. Unnamed
/// (profile-less) setups keep using [`KEYRING_CLIENT_SECRET_USER`], so
/// an existing install is unaffected by the introduction of profiles.
pub fn keyring_client_secret_user(profile: &str) -> String {
    format!("{KEYRING_CLIENT_SECRET_USER}:{profile}")
}

/// Process-wide `--profile` override, set once from argv before any
/// resolution runs.
///
/// A global is the honest shape here rather than a threaded parameter:
/// the profile selects a *credential*, which every chain in this module
/// already resolves from ambient sources (env, config), and threading it
/// through ~8 call sites would put the same value in every signature
/// while changing nothing about where it comes from.
static PROFILE_OVERRIDE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Record the `--profile` flag. Call once, from argv parsing, before
/// any credential resolution. Later calls are ignored.
pub fn set_profile_override(name: &str) {
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        let _ = PROFILE_OVERRIDE.set(trimmed.to_string());
    }
}

/// The `--profile` value, if one was given on this invocation.
pub fn profile_override() -> Option<String> {
    PROFILE_OVERRIDE.get().cloned()
}

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
/// per-call overrides, `Derived` for values recovered from the
/// minted token (the endpoint's data-center claim), and `Default`
/// for built-in constants at the bottom of a chain (the token
/// endpoint).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamSource {
    Flag,
    Env,
    Config,
    Derived,
    Default,
}

impl ParamSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            ParamSource::Flag => "flag",
            ParamSource::Env => "env",
            ParamSource::Config => "config",
            ParamSource::Derived => "derived",
            ParamSource::Default => "default",
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

/// Resolve the service-account client ID: flag → env → active
/// profile → config `[auth]`.
///
/// The profile layer sits above `[auth]` rather than replacing it, so
/// an install that predates profiles keeps working untouched.
pub fn resolve_client_id(flag: Option<&str>) -> Result<Option<ResolvedParam>> {
    if let Some(p) = resolve_profile()? {
        if let Some(id) = p.client_id {
            return Ok(Some(ResolvedParam {
                value: id,
                source: ParamSource::Config,
            }));
        }
    }
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
    if let Some(p) = resolve_profile()? {
        if let Some(v) = read_keyring_entry(&keyring_client_secret_user(&p.name)) {
            return Ok(Some(ResolvedSecret {
                value: v,
                source: SecretSource::Keyring,
            }));
        }
        // A named profile does NOT fall back to the unnamed keyring
        // entry or to `[auth] client_secret`. Falling back would run
        // the command under whichever credential happens to be there,
        // which is precisely the wrong-account-used-by-accident failure
        // profiles exist to stop. Absent here means absent.
        return Ok(None);
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
    if let Some(explicit) = resolve_param(flag, API_URL_ENV, |_| None)? {
        return Ok(Some(explicit));
    }
    if let Some(p) = resolve_profile()? {
        if let Some(url) = p.api_url {
            return Ok(Some(ResolvedParam {
                value: url,
                source: ParamSource::Config,
            }));
        }
    }
    resolve_param(None, API_URL_ENV, |c| c.default.api_url.clone())
}

/// Resolve the OAuth token endpoint: flag → env → config → built-in
/// default.
pub fn resolve_token_url(flag: Option<&str>) -> Result<ResolvedParam> {
    Ok(
        resolve_param(flag, TOKEN_URL_ENV, |c| c.auth.token_url.clone())?.unwrap_or(
            ResolvedParam {
                value: DEFAULT_TOKEN_URL.to_string(),
                source: ParamSource::Default,
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

/// Read posture (D11): under `Curated` (the default) ad-hoc GraphQL
/// documents are refused; `Exploratory` permits ad-hoc READ documents.
/// Mutations refuse unconditionally in both postures. The posture is a
/// persistent, operator-set config value — there is deliberately no
/// per-call or per-shell override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Posture {
    Curated,
    Exploratory,
}

impl Posture {
    pub fn as_str(&self) -> &'static str {
        match self {
            Posture::Curated => "curated",
            Posture::Exploratory => "exploratory",
        }
    }
}

/// Resolve the read posture from config. Absent means `Curated`. An
/// unrecognized value is fatal: a misspelled posture must not silently
/// widen or narrow the surface.
pub fn resolve_posture() -> Result<Posture> {
    match read_config()?.and_then(|c| c.default.posture) {
        None => Ok(Posture::Curated),
        Some(v) => match v.trim() {
            "curated" => Ok(Posture::Curated),
            "exploratory" => Ok(Posture::Exploratory),
            other => Err(StaveError::Auth(format!(
                "config `posture` must be `curated` or `exploratory`, got {other:?}"
            ))),
        },
    }
}

/// Which plane a profile's credential belongs to.
///
/// The read plane and the credential plane are separate binaries with
/// separate credentials (see `docs/design/credential-plane.md`). The
/// plane is recorded on the profile so a credential cannot be used by
/// the wrong binary, which is the one property no surveyed CLI
/// (AWS, gcloud, spacectl, gh, kubectl) attempts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Plane {
    #[default]
    Read,
    Provision,
}

impl Plane {
    pub fn as_str(&self) -> &'static str {
        match self {
            Plane::Read => "read",
            Plane::Provision => "provision",
        }
    }

    fn parse(v: &str) -> Result<Plane> {
        match v.trim() {
            "read" => Ok(Plane::Read),
            "provision" => Ok(Plane::Provision),
            other => Err(StaveError::Auth(format!(
                "profile `plane` must be `read` or `provision`, got {other:?}"
            ))),
        }
    }
}

/// The plane of the *running binary*. Defaults to [`Plane::Read`]: an
/// unset value must resolve to the stricter answer, so a binary that
/// forgets to declare itself cannot reach a provisioning credential.
static BINARY_PLANE: std::sync::OnceLock<Plane> = std::sync::OnceLock::new();

/// Declare the running binary's plane. Call once at startup.
pub fn set_binary_plane(plane: Plane) {
    let _ = BINARY_PLANE.set(plane);
}

fn binary_plane() -> Plane {
    *BINARY_PLANE.get().unwrap_or(&Plane::Read)
}

/// A profile selected through the chain, with the layer that named it.
#[derive(Clone, Debug)]
pub struct ResolvedProfile {
    pub name: String,
    pub source: ParamSource,
    pub plane: Plane,
    pub client_id: Option<String>,
    pub api_url: Option<String>,
    pub purpose: Option<String>,
}

/// Resolve the active profile: flag → env → config `[default] profile`
/// → none (the unnamed legacy credential).
///
/// Three refusals, each of which would otherwise be a silent footgun:
///
/// * A named profile that does not exist is an error, never a silent
///   fall-through to the unnamed credential. Falling through would run
///   the command under a *different* identity than the one named.
/// * A disabled profile is an error even when named explicitly. That is
///   what `disable` is for.
/// * **A provision-plane profile may not come from the stored default.**
///   Every surveyed CLI makes the active credential invisible at the
///   point of use and tells the operator to remember to check
///   (`gh auth status`, `spacectl profile current`). Remembered controls
///   are exactly what this repo keeps finding insufficient, and here the
///   consequence is minting credentials in a production tenant under a
///   profile nobody recalled was active. A provisioning profile is named
///   per invocation or it is not used.
pub fn resolve_profile() -> Result<Option<ResolvedProfile>> {
    let (name, source) = match selected_profile_name()? {
        Some(v) => v,
        None => return Ok(None),
    };
    let cfg = read_config()?.unwrap_or_default();
    select_profile(&name, source, &cfg, binary_plane()).map(Some)
}

/// The decision half of [`resolve_profile`], with every ambient input
/// passed in.
///
/// Split out so the refusals below are unit-testable without touching
/// the environment: this crate forbids `unsafe`, so `set_var` is not
/// available, and a control worth having is a control worth testing
/// directly rather than only through a subprocess.
pub fn select_profile(
    name: &str,
    source: ParamSource,
    cfg: &Config,
    binary: Plane,
) -> Result<ResolvedProfile> {
    let name = name.to_string();
    let entry = cfg.profile.get(&name).ok_or_else(|| {
        let mut known: Vec<&str> = cfg.profile.keys().map(String::as_str).collect();
        known.sort_unstable();
        let known = if known.is_empty() {
            "none are configured".to_string()
        } else {
            known.join(", ")
        };
        StaveError::Auth(format!(
            "profile {name:?} is not configured (named via {}). Known profiles: {known}.\n  \
             - `stave profile list`  (what exists)\n  \
             - `stave profile add {name} --client-id <id>`  (create it)",
            source.as_str()
        ))
    })?;

    if !entry.enabled {
        return Err(StaveError::Auth(format!(
            "profile {name:?} is disabled and will not be used.\n  \
             - `stave profile enable {name}`  (re-enable it)"
        )));
    }

    let plane = match &entry.plane {
        Some(v) => Plane::parse(v)?,
        None => Plane::Read,
    };

    if plane == Plane::Provision && source == ParamSource::Config {
        return Err(StaveError::Auth(format!(
            "profile {name:?} is a provisioning credential and cannot be the stored default. \
             Name it explicitly on the call that needs it:\n  \
             - `--profile {name}`\n  \
             - `{PROFILE_ENV}={name}`\n\
             A provisioning profile reached by stored default would let an unqualified \
             command mint credentials."
        )));
    }

    if plane != binary {
        return Err(StaveError::Auth(format!(
            "profile {name:?} belongs to the {} plane; this binary is the {} plane. \
             Planes are separate binaries with separate credentials.",
            plane.as_str(),
            binary.as_str()
        )));
    }

    Ok(ResolvedProfile {
        name,
        source,
        plane,
        client_id: entry.client_id.clone().filter(|s| !s.is_empty()),
        api_url: entry.api_url.clone().filter(|s| !s.is_empty()),
        purpose: entry.purpose.clone().filter(|s| !s.is_empty()),
    })
}

fn selected_profile_name() -> Result<Option<(String, ParamSource)>> {
    if let Some(v) = PROFILE_OVERRIDE.get() {
        return Ok(Some((v.clone(), ParamSource::Flag)));
    }
    if let Ok(v) = std::env::var(PROFILE_ENV) {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Ok(Some((trimmed.to_string(), ParamSource::Env)));
        }
    }
    if let Some(v) = read_config()?.and_then(|c| c.default.profile.filter(|s| !s.is_empty())) {
        return Ok(Some((v.trim().to_string(), ParamSource::Config)));
    }
    Ok(None)
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

/// `STAVE_KEYRING=off` disables the platform keyring entirely: reads
/// resolve as absent, deletes are no-ops, and writes error. For test
/// harnesses (a hermetic sandbox must never open the user's real
/// keychain — a macOS access-control prompt hangs a headless run) and
/// for environments without a keyring daemon.
pub const KEYRING_ENV: &str = "STAVE_KEYRING";

fn keyring_disabled() -> bool {
    std::env::var(KEYRING_ENV).is_ok_and(|v| v.trim().eq_ignore_ascii_case("off"))
}

fn read_keyring_entry(user: &str) -> Option<String> {
    if keyring_disabled() {
        return None;
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, user).ok()?;
    entry.get_password().ok()
}

/// Store the client secret in the platform keyring, replacing any
/// existing entry.
pub fn store_client_secret(secret: &str) -> Result<()> {
    store_keyring_entry(KEYRING_CLIENT_SECRET_USER, secret)
}

/// Store a named profile's client secret in the platform keyring.
pub fn store_profile_secret(profile: &str, secret: &str) -> Result<()> {
    store_keyring_entry(&keyring_client_secret_user(profile), secret)
}

/// Delete a named profile's keyring entry (`Ok(false)` = nothing there).
pub fn delete_profile_secret(profile: &str) -> Result<bool> {
    delete_keyring_entry(&keyring_client_secret_user(profile))
}

/// Whether a named profile has a secret in the keyring. Reports
/// presence only; the value is never returned to a caller that asked
/// this question.
pub fn profile_secret_present(profile: &str) -> bool {
    read_keyring_entry(&keyring_client_secret_user(profile)).is_some()
}

/// A profile's stored client ID, read directly from config.
///
/// Deliberately bypasses [`resolve_profile`]: enrolment (`auth login`)
/// must see a profile whose plane this binary may not *use*, and must
/// also work for a profile being created in the same breath.
pub fn profile_client_id(profile: &str) -> Result<Option<String>> {
    Ok(read_config()?
        .and_then(|c| c.profile.get(profile).and_then(|p| p.client_id.clone()))
        .filter(|s| !s.is_empty()))
}

/// A profile's declared plane, read directly from config. `None` when
/// the profile does not exist; absent-but-present means [`Plane::Read`].
pub fn profile_plane(profile: &str) -> Result<Option<Plane>> {
    let Some(cfg) = read_config()? else {
        return Ok(None);
    };
    let Some(entry) = cfg.profile.get(profile) else {
        return Ok(None);
    };
    match &entry.plane {
        Some(v) => Plane::parse(v).map(Some),
        None => Ok(Some(Plane::Read)),
    }
}

/// The plane this binary declared. Exposed so enrolment can report the
/// mismatch it is refusing to act on.
pub fn current_binary_plane() -> Plane {
    binary_plane()
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
    if keyring_disabled() {
        return Err(StaveError::Auth(format!(
            "the platform keyring is disabled ({KEYRING_ENV}=off); unset it to store \
             secrets in the keyring, or provide the value via env or config instead"
        )));
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, user)
        .map_err(|e| StaveError::Auth(format!("keyring open: {e}")))?;
    entry
        .set_password(secret)
        .map_err(|e| StaveError::Auth(format!("keyring write: {e}")))?;
    Ok(())
}

fn delete_keyring_entry(user: &str) -> Result<bool> {
    if keyring_disabled() {
        return Ok(false);
    }
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
    /// Named profiles, one per service account. `[profile.<name>]`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub profile: BTreeMap<String, ProfileConfig>,
}

/// One named service account. The secret never lives here: it goes to
/// the platform keyring under [`keyring_client_secret_user`].
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ProfileConfig {
    /// Service-account client ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Endpoint override for this account. Usually absent: the
    /// endpoint derives from the minted token's data-center claim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
    /// What this credential is for, in the operator's words. Shown by
    /// `stave profile list`, which is the point: a list of client IDs
    /// tells you nothing about which one you want.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// `read` (default) or `provision`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plane: Option<String>,
    /// Locally disabled profiles refuse to resolve even when named.
    /// Absent means enabled: a profile someone just wrote by hand into
    /// the config file should work, and opting *out* is the explicit act.
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}

fn enabled_default() -> bool {
    true
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            client_id: None,
            api_url: None,
            purpose: None,
            plane: None,
            enabled: true,
        }
    }
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
    /// Read posture (D11): `curated` (default) or `exploratory`.
    /// Under `curated`, ad-hoc GraphQL documents (`stave api --query`)
    /// are refused; `exploratory` permits ad-hoc READ documents.
    /// Mutations refuse unconditionally in both postures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub posture: Option<String>,
    /// Name of the profile used when neither `--profile` nor
    /// `STAVE_PROFILE` names one. May not name a provision-plane
    /// profile; see [`resolve_profile`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

impl DefaultConfig {
    fn is_empty(&self) -> bool {
        self.api_url.is_none() && self.posture.is_none() && self.profile.is_none()
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
    // D10: `allow_writes` was removed from the typed model. A stale key
    // parses harmlessly (#[serde(default)]) but is obsolete and ignored.
    if body.contains("allow_writes") {
        tracing::warn!(
            "config contains obsolete `allow_writes`; the key is ignored — stave \
             is read-only against live tenants (docs/design/read-only-posture-and-permissions-report.md)"
        );
    }
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
posture = "exploratory"
allow_writes = false

[registry]
host = "registry.example.test"
username = "repo-user"
"#;
        // `allow_writes` above is the D10 stale-key case: it has no
        // typed field and must parse harmlessly via #[serde(default)].
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
        assert_eq!(cfg.default.posture.as_deref(), Some("exploratory"));
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

#[cfg(test)]
mod profile_tests {
    use super::*;

    fn cfg() -> Config {
        let mut c = Config::default();
        c.default.profile = Some("reader".into());
        c.profile.insert(
            "reader".into(),
            ProfileConfig {
                client_id: Some("id-reader".into()),
                purpose: Some("day-to-day reads".into()),
                plane: Some("read".into()),
                ..Default::default()
            },
        );
        c.profile.insert(
            "provisioner".into(),
            ProfileConfig {
                client_id: Some("id-provisioner".into()),
                plane: Some("provision".into()),
                ..Default::default()
            },
        );
        c.profile.insert(
            "retired".into(),
            ProfileConfig {
                client_id: Some("id-retired".into()),
                enabled: false,
                ..Default::default()
            },
        );
        c
    }

    #[test]
    fn a_read_profile_resolves_and_carries_its_source() {
        let p = select_profile("reader", ParamSource::Config, &cfg(), Plane::Read).unwrap();
        assert_eq!(p.name, "reader");
        assert_eq!(p.source, ParamSource::Config);
        assert_eq!(p.plane, Plane::Read);
        assert_eq!(p.client_id.as_deref(), Some("id-reader"));
    }

    #[test]
    fn an_unknown_profile_errors_and_lists_the_known_ones() {
        let err = select_profile("typo", ParamSource::Env, &cfg(), Plane::Read)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not configured"), "{err}");
        assert!(err.contains("provisioner"), "{err}");
    }

    #[test]
    fn a_disabled_profile_refuses_even_when_named_explicitly() {
        let err = select_profile("retired", ParamSource::Flag, &cfg(), Plane::Read)
            .unwrap_err()
            .to_string();
        assert!(err.contains("disabled"), "{err}");
        assert!(err.contains("stave profile enable retired"), "{err}");
    }

    #[test]
    fn a_provisioning_profile_is_refused_from_the_stored_default() {
        let err = select_profile("provisioner", ParamSource::Config, &cfg(), Plane::Provision)
            .unwrap_err()
            .to_string();
        assert!(err.contains("cannot be the stored default"), "{err}");
        assert!(err.contains("--profile provisioner"), "{err}");
    }

    #[test]
    fn a_provisioning_profile_named_explicitly_is_allowed_on_its_own_plane() {
        let p = select_profile("provisioner", ParamSource::Flag, &cfg(), Plane::Provision).unwrap();
        assert_eq!(p.plane, Plane::Provision);
    }

    #[test]
    fn the_read_binary_refuses_a_provisioning_profile() {
        let err = select_profile("provisioner", ParamSource::Flag, &cfg(), Plane::Read)
            .unwrap_err()
            .to_string();
        assert!(err.contains("provision plane"), "{err}");
        assert!(err.contains("read plane"), "{err}");
    }

    #[test]
    fn an_undeclared_binary_plane_defaults_to_the_stricter_answer() {
        // BINARY_PLANE unset must mean Read, so a binary that forgets
        // to declare itself cannot reach a provisioning credential.
        assert_eq!(binary_plane(), Plane::Read);
    }

    #[test]
    fn an_unrecognized_plane_is_fatal_rather_than_defaulting() {
        let mut c = cfg();
        c.profile.get_mut("reader").unwrap().plane = Some("admin".into());
        let err = select_profile("reader", ParamSource::Flag, &c, Plane::Read)
            .unwrap_err()
            .to_string();
        assert!(err.contains("must be `read` or `provision`"), "{err}");
    }

    #[test]
    fn an_absent_plane_means_read() {
        let mut c = cfg();
        c.profile.get_mut("reader").unwrap().plane = None;
        let p = select_profile("reader", ParamSource::Flag, &c, Plane::Read).unwrap();
        assert_eq!(p.plane, Plane::Read);
    }

    #[test]
    fn an_absent_enabled_key_means_enabled() {
        let parsed: Config = toml::from_str("[profile.p]\nclient_id = \"x\"\n").expect("parses");
        assert!(parsed.profile["p"].enabled);
    }

    #[test]
    fn keyring_account_is_namespaced_per_profile() {
        assert_eq!(keyring_client_secret_user("reader"), "client-secret:reader");
        assert_ne!(
            keyring_client_secret_user("reader"),
            KEYRING_CLIENT_SECRET_USER
        );
    }

    #[test]
    fn a_profile_round_trips_through_toml() {
        let rendered = toml::to_string(&cfg()).expect("serializes");
        let parsed: Config = toml::from_str(&rendered).expect("parses");
        assert_eq!(parsed.default.profile.as_deref(), Some("reader"));
        assert!(!parsed.profile["retired"].enabled);
        assert_eq!(
            parsed.profile["provisioner"].plane.as_deref(),
            Some("provision")
        );
    }
}
