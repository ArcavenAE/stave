//! Resolution-chain coverage: the client ID, the client secret, the
//! tenant GraphQL endpoint, and the standing write opt-in, each resolved
//! through flag, then env, then config, then (for the endpoint) derived
//! from the minted token's data-center claim.
//!
//! Also covers what the chains report and what they refuse to report:
//! `auth status` names every source without printing a secret value,
//! `config show` masks secrets that live in the file, and an unresolved
//! chain errors with a message naming each layer plus one concrete next
//! step for it (cli-philosophy.md, "The fix").
//!
//! Nothing here reaches the network or the platform keyring. Tests of
//! the "nothing resolved" path withhold the client ID rather than the
//! secret, because the ID resolves from env and config only, so the
//! keyring layer is never consulted. See `common/mod.rs`.

mod common;

use common::{Sandbox, jwt_with_dc_claim, run, stderr_of, stdout_of};
use serde_json::Value;

/// `auth status` writes one JSON object when stdout is a pipe, which is
/// always the case under a test harness.
fn auth_status(sandbox: &Sandbox, extra_env: &[(&str, &str)]) -> (bool, Value) {
    let mut cmd = sandbox.cmd();
    cmd.args(["auth", "status"]);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let out = run(&mut cmd);
    let stdout = stdout_of(&out);
    let parsed = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("auth status must emit one JSON object, got {stdout:?} ({e})"));
    (out.status.success(), parsed)
}

/// Credentials that satisfy the chain from env alone, so `auth status`
/// exits zero and the keyring is never reached.
fn env_credentials() -> Vec<(&'static str, &'static str)> {
    vec![
        ("STAVE_CLIENT_ID", "svc-example"),
        ("STAVE_CLIENT_SECRET", "example-secret"),
    ]
}

fn field(status: &Value, key: &str) -> String {
    status
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("auth status has no `{key}` field: {status}"))
        .to_string()
}

// ---------------------------------------------------------------------------
// client ID chain: flag, env, config
// ---------------------------------------------------------------------------

#[test]
fn client_id_resolves_from_env_and_names_the_source() {
    let sandbox = Sandbox::new();
    let (ok, status) = auth_status(&sandbox, &env_credentials());
    assert!(ok, "env credentials satisfy the chain: {status}");
    assert_eq!(field(&status, "client_id"), "svc-example (source: env)");
}

#[test]
fn client_id_resolves_from_config_when_env_is_absent() {
    let sandbox = Sandbox::new();
    sandbox.write_config(
        r#"
[auth]
client_id = "svc-from-config"
"#,
    );
    let (ok, status) = auth_status(&sandbox, &[("STAVE_CLIENT_SECRET", "example-secret")]);
    assert!(ok, "config ID plus env secret satisfy the chain: {status}");
    assert_eq!(
        field(&status, "client_id"),
        "svc-from-config (source: config)"
    );
}

#[test]
fn client_id_env_beats_config() {
    let sandbox = Sandbox::new();
    sandbox.write_config(
        r#"
[auth]
client_id = "svc-from-config"
"#,
    );
    let (_, status) = auth_status(
        &sandbox,
        &[
            ("STAVE_CLIENT_ID", "svc-from-env"),
            ("STAVE_CLIENT_SECRET", "example-secret"),
        ],
    );
    assert_eq!(field(&status, "client_id"), "svc-from-env (source: env)");
}

#[test]
fn client_id_unset_reports_every_layer_of_the_chain() {
    let sandbox = Sandbox::new();
    let (ok, status) = auth_status(&sandbox, &[("STAVE_CLIENT_SECRET", "example-secret")]);
    assert!(!ok, "incomplete credentials must exit nonzero: {status}");
    let line = field(&status, "client_id");
    assert!(line.contains("--client-id"), "missing flag layer: {line}");
    assert!(
        line.contains("STAVE_CLIENT_ID"),
        "missing env layer: {line}"
    );
    assert!(
        line.contains("stave config set client_id"),
        "missing config layer: {line}"
    );
}

// ---------------------------------------------------------------------------
// client secret: reported by source and length, never by value
// ---------------------------------------------------------------------------

#[test]
fn client_secret_from_env_is_reported_by_source_and_length_only() {
    let sandbox = Sandbox::new();
    let (_, status) = auth_status(&sandbox, &env_credentials());
    let line = field(&status, "client_secret");
    assert_eq!(line, "present (source: env, length: 14 bytes)");
    assert!(
        !status.to_string().contains("example-secret"),
        "the secret value must not appear anywhere in the report: {status}"
    );
}

#[test]
fn a_config_resident_secret_satisfies_the_chain_without_printing_itself() {
    // The secret chain is env, then keyring, then config. With env
    // cleared, the keyring layer runs before config, and the SDK offers
    // no way to bypass it, so which of the two answers depends on whether
    // the machine has a `stave` keyring entry. The source is therefore
    // deliberately not asserted here; what is asserted is what cannot
    // vary: the chain resolves, and the value never reaches the report.
    // Env-sourced provenance is pinned exactly in the test above.
    let sandbox = Sandbox::new();
    sandbox.write_config(
        r#"
[auth]
client_id = "svc-example"
client_secret = "config-resident-secret"
"#,
    );
    let (ok, status) = auth_status(&sandbox, &[]);
    assert!(ok, "config supplies both halves: {status}");
    let line = field(&status, "client_secret");
    assert!(
        line.starts_with("present (source: "),
        "the secret must be reported as present with a named source: {line}"
    );
    assert!(
        line.contains("bytes"),
        "a secret is reported by shape, so its length stands in for it: {line}"
    );
    assert!(
        !status.to_string().contains("config-resident-secret"),
        "the secret value leaked: {status}"
    );
}

// ---------------------------------------------------------------------------
// endpoint chain: env, config, derived from the token's dc claim
// ---------------------------------------------------------------------------

#[test]
fn api_url_resolves_from_env_and_names_the_source() {
    let sandbox = Sandbox::new();
    let mut env = env_credentials();
    env.push(("STAVE_API_URL", "https://api.example1.app.wiz.io/graphql"));
    let (_, status) = auth_status(&sandbox, &env);
    assert_eq!(
        field(&status, "api_url"),
        "https://api.example1.app.wiz.io/graphql (source: env)"
    );
}

#[test]
fn api_url_resolves_from_config_when_env_is_absent() {
    let sandbox = Sandbox::new();
    sandbox.write_config(
        r#"
[default]
api_url = "https://api.example1.app.wiz.io/graphql"
"#,
    );
    let (_, status) = auth_status(&sandbox, &env_credentials());
    assert_eq!(
        field(&status, "api_url"),
        "https://api.example1.app.wiz.io/graphql (source: config)"
    );
}

#[test]
fn api_url_env_beats_config() {
    let sandbox = Sandbox::new();
    sandbox.write_config(
        r#"
[default]
api_url = "https://api.config.app.wiz.io/graphql"
"#,
    );
    let mut env = env_credentials();
    env.push(("STAVE_API_URL", "https://api.env.app.wiz.io/graphql"));
    let (_, status) = auth_status(&sandbox, &env);
    assert_eq!(
        field(&status, "api_url"),
        "https://api.env.app.wiz.io/graphql (source: env)"
    );
}

#[test]
fn api_url_derives_from_the_tokens_data_center_claim() {
    // The chain's derivation layer, and the one place stave invents a
    // hostname: no flag, env, or config value, but a token in hand names
    // the tenant's data center. Asserted through `auth status`, which
    // makes no network call, so the derived host is never dialled.
    let sandbox = Sandbox::new();
    let token = jwt_with_dc_claim();
    let mut env = env_credentials();
    env.push(("STAVE_ACCESS_TOKEN", token.as_str()));
    let (_, status) = auth_status(&sandbox, &env);
    assert_eq!(
        field(&status, "api_url"),
        "https://api.example1.app.wiz.io/graphql (source: derived)"
    );
}

#[test]
fn api_url_derivation_is_skipped_for_a_token_without_the_claim() {
    let sandbox = Sandbox::new();
    let token = common::fake_jwt(serde_json::json!({"sub": "svc-example"}));
    let mut env = env_credentials();
    env.push(("STAVE_ACCESS_TOKEN", token.as_str()));
    let (_, status) = auth_status(&sandbox, &env);
    let line = field(&status, "api_url");
    assert!(line.starts_with("unset."), "want unresolved, got {line}");
    assert!(line.contains("STAVE_API_URL"), "{line}");
    assert!(line.contains("stave config set api_url"), "{line}");
}

// ---------------------------------------------------------------------------
// chain-naming errors on the working path
// ---------------------------------------------------------------------------

#[test]
fn list_without_credentials_errors_naming_every_credential_source() {
    // The secret is supplied so the failure is unambiguously the missing
    // client ID, and so the keyring layer is never consulted.
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue"])
        .env("STAVE_CLIENT_SECRET", "example-secret"));
    assert!(!out.status.success(), "expected failure: {out:?}");
    let err = stderr_of(&out);
    assert!(
        err.contains("no Wiz service-account credentials resolved"),
        "should name what is missing: {err}"
    );
    assert!(
        err.contains("stave auth login"),
        "missing login step: {err}"
    );
    assert!(err.contains("STAVE_CLIENT_ID"), "missing env layer: {err}");
    assert!(
        err.contains("STAVE_CLIENT_SECRET"),
        "missing secret env layer: {err}"
    );
    assert!(
        err.contains("stave config set client_id"),
        "missing config layer: {err}"
    );
}

#[test]
fn list_without_an_endpoint_errors_naming_every_endpoint_source() {
    // A pre-minted opaque token satisfies auth and defeats derivation,
    // which leaves the endpoint chain as the only thing that can fail.
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue"])
        .env("STAVE_ACCESS_TOKEN", "opaque-not-a-jwt"));
    assert!(!out.status.success(), "expected failure: {out:?}");
    let err = stderr_of(&out);
    assert!(
        err.contains("no API endpoint resolved"),
        "should name what is missing: {err}"
    );
    assert!(err.contains("--api-url"), "missing flag layer: {err}");
    assert!(err.contains("STAVE_API_URL"), "missing env layer: {err}");
    assert!(
        err.contains("stave config set api_url"),
        "missing config layer: {err}"
    );
    assert!(
        err.contains("data-center claim"),
        "missing derivation layer: {err}"
    );
}

#[test]
fn base_url_override_skips_the_endpoint_chain_entirely() {
    // Port 1 refuses the connection, which is the point: the failure
    // must be the network, not the chain, proving the override bypassed
    // resolution.
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue"])
        .env("STAVE_ACCESS_TOKEN", "opaque-not-a-jwt")
        .env("STAVE_BASE_URL", "http://127.0.0.1:1/graphql"));
    assert!(!out.status.success(), "expected a network failure: {out:?}");
    let err = stderr_of(&out);
    assert!(
        !err.contains("no API endpoint resolved"),
        "the chain error must not fire when STAVE_BASE_URL is set: {err}"
    );
    assert!(err.contains("network"), "want the network error: {err}");
}

// ---------------------------------------------------------------------------
// standing write opt-in
// ---------------------------------------------------------------------------

#[test]
fn writes_are_guarded_by_default() {
    let sandbox = Sandbox::new();
    let (_, status) = auth_status(&sandbox, &env_credentials());
    assert_eq!(
        field(&status, "writes"),
        "guarded (read-only; pass --allow-write per call to override)"
    );
}

#[test]
fn writes_opt_in_from_env_accepts_the_three_affirmative_spellings() {
    for value in ["1", "true", "yes"] {
        let sandbox = Sandbox::new();
        let mut env = env_credentials();
        env.push(("STAVE_ALLOW_WRITE", value));
        let (_, status) = auth_status(&sandbox, &env);
        assert_eq!(
            field(&status, "writes"),
            "allowed by standing opt-in (source: STAVE_ALLOW_WRITE)",
            "STAVE_ALLOW_WRITE={value} should open the standing opt-in"
        );
    }
}

#[test]
fn writes_stay_guarded_for_a_negative_env_value() {
    let sandbox = Sandbox::new();
    let mut env = env_credentials();
    env.push(("STAVE_ALLOW_WRITE", "0"));
    let (_, status) = auth_status(&sandbox, &env);
    assert!(
        field(&status, "writes").starts_with("guarded"),
        "only affirmative values open the gate: {status}"
    );
}

#[test]
fn writes_opt_in_from_config() {
    let sandbox = Sandbox::new();
    sandbox.write_config(
        r#"
[default]
allow_writes = true
"#,
    );
    let (_, status) = auth_status(&sandbox, &env_credentials());
    assert_eq!(
        field(&status, "writes"),
        "allowed by standing opt-in (source: config)"
    );
}

#[test]
fn env_opt_in_wins_over_a_config_refusal() {
    let sandbox = Sandbox::new();
    sandbox.write_config(
        r#"
[default]
allow_writes = false
"#,
    );
    let mut env = env_credentials();
    env.push(("STAVE_ALLOW_WRITE", "1"));
    let (_, status) = auth_status(&sandbox, &env);
    assert_eq!(
        field(&status, "writes"),
        "allowed by standing opt-in (source: STAVE_ALLOW_WRITE)"
    );
}

// ---------------------------------------------------------------------------
// auth status: the rest of the report
// ---------------------------------------------------------------------------

#[test]
fn auth_status_reports_the_sandbox_config_and_audit_paths() {
    let sandbox = Sandbox::new();
    let (_, status) = auth_status(&sandbox, &env_credentials());
    assert_eq!(
        field(&status, "config"),
        sandbox.config_path().display().to_string()
    );
    assert_eq!(
        field(&status, "audit_dir"),
        sandbox.audit_dir().display().to_string()
    );
    assert!(
        field(&status, "token_cache").starts_with("empty ("),
        "a fresh sandbox has no cached token: {status}"
    );
}

#[test]
fn auth_status_reports_a_disabled_audit_trail() {
    let sandbox = Sandbox::new();
    let mut env = env_credentials();
    env.push(("STAVE_AUDIT", "off"));
    let (_, status) = auth_status(&sandbox, &env);
    assert_eq!(field(&status, "audit_dir"), "disabled (STAVE_AUDIT=off)");
}

// ---------------------------------------------------------------------------
// config subcommand
// ---------------------------------------------------------------------------

#[test]
fn config_path_prints_the_override() {
    let sandbox = Sandbox::new();
    let out = run(sandbox.cmd().args(["config", "path"]));
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        stdout_of(&out).trim(),
        sandbox.config_path().display().to_string()
    );
}

#[test]
fn config_show_on_a_missing_file_says_so_without_failing() {
    let sandbox = Sandbox::new();
    let out = run(sandbox.cmd().args(["config", "show"]));
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("(file does not exist yet)"), "{stdout}");
    assert!(!sandbox.config_exists(), "show must not create the file");
}

#[test]
fn config_set_then_show_then_unset_round_trip() {
    let sandbox = Sandbox::new();

    let out = run(sandbox
        .cmd()
        .args(["config", "set", "client_id", "svc-example"]));
    assert!(out.status.success(), "set failed: {out:?}");
    assert!(
        sandbox
            .read_config()
            .contains(r#"client_id = "svc-example""#),
        "{}",
        sandbox.read_config()
    );

    let out = run(sandbox.cmd().args(["config", "show"]));
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("[auth]"), "{stdout}");
    assert!(stdout.contains(r#"client_id = "svc-example""#), "{stdout}");

    let out = run(sandbox.cmd().args(["config", "unset", "client_id"]));
    assert!(out.status.success(), "unset failed: {out:?}");
    assert!(
        !sandbox.read_config().contains("client_id"),
        "client_id not cleared: {}",
        sandbox.read_config()
    );
}

#[test]
fn config_set_preserves_every_other_section() {
    let sandbox = Sandbox::new();
    sandbox.write_config(
        r#"
[auth]
client_id = "svc-preexisting"

[registry]
host = "registry.example.test"
username = "tenant-00000000"
"#,
    );

    let out = run(sandbox.cmd().args([
        "config",
        "set",
        "api_url",
        "https://api.example1.app.wiz.io/graphql",
    ]));
    assert!(out.status.success(), "{out:?}");

    let body = sandbox.read_config();
    assert!(
        body.contains(r#"client_id = "svc-preexisting""#),
        "existing client_id must survive: {body}"
    );
    assert!(
        body.contains(r#"host = "registry.example.test""#),
        "registry section must survive: {body}"
    );
    assert!(
        body.contains(r#"api_url = "https://api.example1.app.wiz.io/graphql""#),
        "new key must land: {body}"
    );
}

#[test]
fn config_show_masks_a_client_secret_that_lives_in_the_file() {
    let sandbox = Sandbox::new();
    sandbox.write_config(
        r#"
[auth]
client_id = "svc-example"
client_secret = "abcdef"
"#,
    );
    let out = run(sandbox.cmd().args(["config", "show"]));
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(
        stdout.contains("<redacted, length=6>"),
        "secret must be shown as a shape: {stdout}"
    );
    assert!(!stdout.contains("abcdef"), "raw secret leaked: {stdout}");
    assert!(
        stdout.contains(r#"client_id = "svc-example""#),
        "non-secret values still print: {stdout}"
    );
}

#[test]
fn config_show_masks_a_registry_password_that_lives_in_the_file() {
    let sandbox = Sandbox::new();
    sandbox.write_config(
        r#"
[registry]
host = "registry.example.test"
password = "0123456789"
"#,
    );
    let out = run(sandbox.cmd().args(["config", "show"]));
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("<redacted, length=10>"), "{stdout}");
    assert!(!stdout.contains("0123456789"), "{stdout}");
}

#[test]
fn config_set_refuses_secret_keys_and_points_at_the_keyring() {
    let sandbox = Sandbox::new();
    for key in ["client_secret", "auth.client_secret", "registry.password"] {
        let out = run(sandbox.cmd().args(["config", "set", key, "shh"]));
        assert!(!out.status.success(), "{key} must be refused: {out:?}");
        let err = stderr_of(&out);
        assert!(err.contains("is a secret"), "{err}");
        assert!(
            err.contains("stave auth login") || err.contains("stave registry login"),
            "the refusal must name the command that stores it safely: {err}"
        );
        assert!(
            !sandbox.config_exists(),
            "a refused set must not create the file"
        );
    }
}

#[test]
fn config_set_allow_writes_requires_a_boolean() {
    let sandbox = Sandbox::new();

    let out = run(sandbox
        .cmd()
        .args(["config", "set", "allow_writes", "definitely"]));
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("true") && err.contains("false"), "{err}");

    let out = run(sandbox
        .cmd()
        .args(["config", "set", "allow_writes", "true"]));
    assert!(out.status.success(), "{out:?}");
    assert!(
        sandbox.read_config().contains("allow_writes = true"),
        "{}",
        sandbox.read_config()
    );
}

#[test]
fn config_set_rejects_an_empty_value_and_names_unset() {
    let sandbox = Sandbox::new();
    let out = run(sandbox.cmd().args(["config", "set", "client_id", "   "]));
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("must not be empty"), "{err}");
    assert!(err.contains("stave config unset client_id"), "{err}");
}

#[test]
fn config_set_unknown_key_lists_the_known_keys() {
    let sandbox = Sandbox::new();
    let out = run(sandbox.cmd().args(["config", "set", "bogus", "value"]));
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    for key in [
        "client_id",
        "api_url",
        "allow_writes",
        "token_url",
        "mcp.url",
        "registry.host",
        "registry.username",
    ] {
        assert!(err.contains(key), "known-key list is missing {key}: {err}");
    }
}

#[test]
fn config_unset_unknown_key_lists_the_known_keys() {
    let sandbox = Sandbox::new();
    let out = run(sandbox.cmd().args(["config", "unset", "bogus"]));
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("unknown key"), "{err}");
    assert!(err.contains("client_id"), "{err}");
}

#[test]
fn config_set_token_url_persists_the_oauth_override() {
    // The token endpoint is constant across commercial tenants; the
    // chain exists for isolated clouds and for the wiremock harness.
    let sandbox = Sandbox::new();
    let out = run(sandbox.cmd().args([
        "config",
        "set",
        "token_url",
        "https://auth.example.test/oauth/token",
    ]));
    assert!(out.status.success(), "{out:?}");
    assert!(
        sandbox
            .read_config()
            .contains(r#"token_url = "https://auth.example.test/oauth/token""#),
        "{}",
        sandbox.read_config()
    );
}
