//! OAuth2 client-credentials token flow with a local cache.
//!
//! Wiz service accounts mint short-lived bearer tokens from the auth
//! endpoint (`grant_type=client_credentials`, `audience=wiz-api`,
//! form-encoded). Minting on every CLI invocation would tax the auth
//! endpoint (rate-limited) and add latency to every pipeline stage, so
//! minted tokens are cached in the XDG state dir with a safety margin
//! before expiry.
//!
//! The cache is keyed by (token_url, client_id) so switching service
//! accounts or clouds never replays a stale token. Cache files are
//! written with mode 0600.
//!
//! The minted token is a JWT whose payload carries a data-center
//! claim (`dc`) naming the tenant's region. `dc_claim` recovers it
//! WITHOUT verifying the signature — fine here, because the value is
//! only used to derive the API endpoint the token will be presented
//! to, not to make a trust decision.

use std::path::PathBuf;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use crate::error::{Result, StaveError};

/// Override the token-cache directory (tests, multi-profile setups).
pub const TOKEN_CACHE_DIR_ENV: &str = "STAVE_TOKEN_CACHE_DIR";

/// Seconds of remaining validity below which a cached token is
/// discarded and re-minted.
const EXPIRY_MARGIN_SECS: i64 = 300;

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedToken {
    pub access_token: String,
    /// RFC 3339 UTC instant after which the token must not be used.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// Cache key half 1: the endpoint that minted this token.
    pub token_url: String,
    /// Cache key half 2: the client the token belongs to.
    pub client_id: String,
}

impl CachedToken {
    fn is_fresh(&self, token_url: &str, client_id: &str) -> bool {
        self.token_url == token_url
            && self.client_id == client_id
            && self.expires_at - chrono::Utc::now()
                > chrono::TimeDelta::seconds(EXPIRY_MARGIN_SECS)
    }
}

/// Resolve the token-cache file path. Env override → XDG state dir →
/// `~/.stave` fallback (macOS has no XDG state dir by default).
pub fn cache_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(TOKEN_CACHE_DIR_ENV) {
        if !p.is_empty() {
            return Some(PathBuf::from(p).join("token.json"));
        }
    }
    dirs::state_dir()
        .map(|d| d.join("stave"))
        .or_else(|| dirs::home_dir().map(|h| h.join(".stave")))
        .map(|d| d.join("token.json"))
}

fn read_cache() -> Option<CachedToken> {
    let path = cache_path()?;
    let body = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&body).ok()
}

fn write_cache(token: &CachedToken) -> Result<()> {
    let Some(path) = cache_path() else {
        return Ok(()); // no discoverable home — mint-per-call, don't fail
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| StaveError::Auth(format!("create state dir {}: {e}", parent.display())))?;
    }
    let body = serde_json::to_string(token)
        .map_err(|e| StaveError::Auth(format!("serialize token cache: {e}")))?;
    std::fs::write(&path, body)
        .map_err(|e| StaveError::Auth(format!("write token cache {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| StaveError::Auth(format!("chmod token cache: {e}")))?;
    }
    Ok(())
}

/// Remove the cached token (auth logout, credential rotation).
/// `Ok(false)` = nothing to remove.
pub fn clear_cache() -> Result<bool> {
    let Some(path) = cache_path() else {
        return Ok(false);
    };
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(StaveError::Auth(format!(
            "remove token cache {}: {e}",
            path.display()
        ))),
    }
}

/// Mint a fresh token from the OAuth endpoint. No cache interaction.
pub async fn mint(
    http: &reqwest::Client,
    token_url: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<CachedToken> {
    let form = [
        ("grant_type", "client_credentials"),
        ("audience", crate::auth::TOKEN_AUDIENCE),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];
    let response = http
        .post(token_url)
        .form(&form)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| StaveError::Network(format!("token endpoint: {e}")))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| StaveError::Network(format!("token endpoint body: {e}")))?;
    if !status.is_success() {
        return Err(StaveError::Auth(format!(
            "token mint failed ({status}): check the service-account client ID/secret \
             and its scopes. Endpoint: {token_url}. Response: {}",
            truncate(&body, 300)
        )));
    }
    let parsed: TokenResponse = serde_json::from_str(&body)
        .map_err(|e| StaveError::Auth(format!("token endpoint returned non-JSON: {e}")))?;
    let expires_in = parsed.expires_in.unwrap_or(3600);
    Ok(CachedToken {
        access_token: parsed.access_token,
        expires_at: chrono::Utc::now() + chrono::TimeDelta::seconds(expires_in),
        token_url: token_url.to_string(),
        client_id: client_id.to_string(),
    })
}

/// Return a fresh-enough cached token, or mint one and cache it.
pub async fn cached_or_mint(
    http: &reqwest::Client,
    token_url: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<CachedToken> {
    if let Some(cached) = read_cache() {
        if cached.is_fresh(token_url, client_id) {
            return Ok(cached);
        }
    }
    let minted = mint(http, token_url, client_id, client_secret).await?;
    write_cache(&minted)?;
    Ok(minted)
}

/// Extract the data-center claim (`dc`) from a Wiz JWT without
/// verifying its signature. Returns `None` for non-JWT tokens or
/// tokens without the claim.
pub fn dc_claim(access_token: &str) -> Option<String> {
    let payload_b64 = access_token.split('.').nth(1)?;
    let payload = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    value
        .get("dc")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_jwt(payload: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let body = URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
        format!("{header}.{body}.fakesig")
    }

    #[test]
    fn dc_claim_reads_payload() {
        let token = fake_jwt(serde_json::json!({"dc": "us17", "sub": "svc"}));
        assert_eq!(dc_claim(&token).as_deref(), Some("us17"));
    }

    #[test]
    fn dc_claim_absent_is_none() {
        let token = fake_jwt(serde_json::json!({"sub": "svc"}));
        assert_eq!(dc_claim(&token), None);
    }

    #[test]
    fn dc_claim_non_jwt_is_none() {
        assert_eq!(dc_claim("not-a-jwt"), None);
        assert_eq!(dc_claim(""), None);
    }

    #[test]
    fn cached_token_freshness_respects_margin() {
        let fresh = CachedToken {
            access_token: "t".into(),
            expires_at: chrono::Utc::now() + chrono::TimeDelta::seconds(3600),
            token_url: "https://auth.example.test/oauth/token".into(),
            client_id: "abc".into(),
        };
        assert!(fresh.is_fresh("https://auth.example.test/oauth/token", "abc"));
        // Wrong client — never fresh.
        assert!(!fresh.is_fresh("https://auth.example.test/oauth/token", "other"));
        // Inside the expiry margin — stale.
        let nearly_expired = CachedToken {
            expires_at: chrono::Utc::now() + chrono::TimeDelta::seconds(EXPIRY_MARGIN_SECS - 10),
            ..fresh
        };
        assert!(!nearly_expired.is_fresh("https://auth.example.test/oauth/token", "abc"));
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let s = "héllo wörld";
        let t = truncate(s, 3);
        assert!(t.len() <= 3);
        assert!(s.starts_with(t));
    }
}
