//! End-to-end coverage for `auth login --subdomain/--region`,
//! `auth status` reporting, the `config` subcommand, and the
//! subdomain chain-naming error. Uses a per-test tempdir +
//! `STAVE_CONFIG` so the user's real config is never touched.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::cargo::CommandCargoExt;

static TEMPDIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn tempdir() -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = TEMPDIR_COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("stave-auth-{}-{n}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cmd() -> Command {
    let mut c = Command::cargo_bin("stave").expect("stave");
    // Neutralize ambient credentials/config so tests are hermetic.
    c.env_remove("STAVE_API_TOKEN")
        .env_remove("STAVE_SUBDOMAIN")
        .env_remove("STAVE_REGION")
        .env_remove("STAVE_ALLOW_WRITE")
        .env_remove("STAVE_BASE_URL")
        .env_remove("STAVE_MCP_API_KEY")
        .env_remove("STAVE_MCP_PROFILE")
        .env_remove("STAVE_MCP_URL");
    c
}

#[test]
fn auth_login_with_no_source_errors_naming_the_chain() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");

    let out = cmd()
        .args(["auth", "login"])
        .env("STAVE_CONFIG", &cfg)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();

    assert!(!out.status.success(), "expected failure: {out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--token") && err.contains("--subdomain") && err.contains("--region"),
        "error must name the sources: {err}"
    );
    assert!(!cfg.exists(), "config should not be created on error");
}

#[test]
fn auth_login_subdomain_persists_to_config_without_token() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");

    let out = cmd()
        .args(["auth", "login", "--subdomain", "accuhive"])
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "auth login --subdomain failed: {out:?}"
    );
    let body = std::fs::read_to_string(&cfg).expect("config written");
    assert!(
        body.contains("subdomain = \"accuhive\""),
        "config must persist subdomain: {body}"
    );
    assert!(
        body.contains("[default]"),
        "config must carry [default] section: {body}"
    );
    assert!(
        !body.contains("[auth]"),
        "no token supplied — [auth] section should not be written: {body}"
    );
}

#[test]
fn auth_login_subdomain_and_region_together() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");

    let out = cmd()
        .args(["auth", "login", "--subdomain", "accuhive", "--region", "eu"])
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();

    assert!(out.status.success(), "{out:?}");
    let body = std::fs::read_to_string(&cfg).unwrap();
    assert!(body.contains("subdomain = \"accuhive\""), "{body}");
    assert!(body.contains("region = \"eu\""), "{body}");
}

#[test]
fn auth_login_rejects_bogus_region() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");

    let out = cmd()
        .args(["auth", "login", "--region", "mars"])
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("us") && err.contains("eu"), "{err}");
}

#[test]
fn auth_login_subdomain_does_not_clobber_existing_token_or_region() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");
    std::fs::write(
        &cfg,
        r#"
[auth]
token = "preexisting-token"

[default]
region = "eu"
"#,
    )
    .unwrap();

    let out = cmd()
        .args(["auth", "login", "--subdomain", "accuhive"])
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");

    let body = std::fs::read_to_string(&cfg).unwrap();
    assert!(
        body.contains("token = \"preexisting-token\""),
        "token must be preserved: {body}"
    );
    assert!(
        body.contains("region = \"eu\""),
        "region must be preserved: {body}"
    );
    assert!(
        body.contains("subdomain = \"accuhive\""),
        "subdomain must be added: {body}"
    );
}

#[test]
fn auth_status_reports_subdomain_source_from_env() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");

    let out = cmd()
        .args(["auth", "status"])
        .env("STAVE_CONFIG", &cfg)
        .env("STAVE_API_TOKEN", "fake-token")
        .env("STAVE_SUBDOMAIN", "from-env")
        .output()
        .unwrap();

    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("subdomain:   from-env (source: env)"),
        "subdomain-from-env not reported: {stdout}"
    );
    assert!(
        stdout.contains("region:      us (default)"),
        "region default not reported: {stdout}"
    );
    assert!(
        stdout.contains("writes:      guarded"),
        "write-guard posture must be reported: {stdout}"
    );
}

#[test]
fn auth_status_reports_subdomain_source_from_config_when_env_absent() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");
    std::fs::write(
        &cfg,
        r#"
[default]
subdomain = "from-config"
"#,
    )
    .unwrap();

    let out = cmd()
        .args(["auth", "status"])
        .env("STAVE_CONFIG", &cfg)
        .env("STAVE_API_TOKEN", "fake-token")
        .output()
        .unwrap();

    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("subdomain:   from-config (source: config)"),
        "config-source not reported: {stdout}"
    );
}

#[test]
fn auth_status_env_beats_config() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");
    std::fs::write(
        &cfg,
        r#"
[default]
subdomain = "from-config"
"#,
    )
    .unwrap();

    let out = cmd()
        .args(["auth", "status"])
        .env("STAVE_CONFIG", &cfg)
        .env("STAVE_API_TOKEN", "fake-token")
        .env("STAVE_SUBDOMAIN", "from-env")
        .output()
        .unwrap();

    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("subdomain:   from-env (source: env)"),
        "env should win over config: {stdout}"
    );
}

#[test]
fn auth_status_reports_standing_write_optin() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");
    std::fs::write(
        &cfg,
        r#"
[default]
allow_writes = true
"#,
    )
    .unwrap();

    let out = cmd()
        .args(["auth", "status"])
        .env("STAVE_CONFIG", &cfg)
        .env("STAVE_API_TOKEN", "fake-token")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("writes:      ALLOWED"),
        "standing opt-in must be surfaced loudly: {stdout}"
    );
}

#[test]
fn config_show_redacts_token_length() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");
    std::fs::write(
        &cfg,
        r#"
[auth]
token = "abcdef"

[default]
subdomain = "accuhive"
"#,
    )
    .unwrap();

    let out = cmd()
        .args(["config", "show"])
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("<redacted, length=6>"),
        "token must be redacted: {stdout}"
    );
    assert!(!stdout.contains("abcdef"), "raw token leaked: {stdout}");
    assert!(stdout.contains("subdomain = \"accuhive\""), "{stdout}");
}

#[test]
fn config_set_then_unset_subdomain_round_trip() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");

    let out = cmd()
        .args(["config", "set", "subdomain", "accuhive"])
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();
    assert!(out.status.success(), "set failed: {out:?}");
    let body = std::fs::read_to_string(&cfg).unwrap();
    assert!(body.contains("subdomain = \"accuhive\""), "{body}");

    let out = cmd()
        .args(["config", "unset", "subdomain"])
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();
    assert!(out.status.success(), "unset failed: {out:?}");
    let body = std::fs::read_to_string(&cfg).unwrap();
    assert!(
        !body.contains("subdomain ="),
        "subdomain not cleared: {body}"
    );
}

#[test]
fn config_set_allow_writes_requires_boolean() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");

    let out = cmd()
        .args(["config", "set", "allow_writes", "definitely"])
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("true") && err.contains("false"), "{err}");

    let out = cmd()
        .args(["config", "set", "allow_writes", "true"])
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let body = std::fs::read_to_string(&cfg).unwrap();
    assert!(body.contains("allow_writes = true"), "{body}");
}

#[test]
fn config_set_unknown_key_errors_with_known_keys_listed() {
    let dir = tempdir();
    let cfg = dir.join("config.toml");

    let out = cmd()
        .args(["config", "set", "bogus", "value"])
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("subdomain"), "{err}");
    assert!(err.contains("region"), "{err}");
    assert!(err.contains("allow_writes"), "{err}");
    assert!(err.contains("auth.token"), "{err}");
    assert!(err.contains("mcp.profile"), "{err}");
}

#[test]
fn list_without_subdomain_errors_with_chain_naming_message() {
    // cli-philosophy.md "The fix" rule, third instantiation: when the
    // tenant subdomain is unresolved, the error names every layer of
    // the chain plus a concrete next step for each.
    let dir = tempdir();
    let cfg = dir.join("nonexistent.toml");

    let out = cmd()
        .args(["list", "device"])
        .env("STAVE_API_TOKEN", "fake-tok")
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();

    assert!(!out.status.success(), "expected failure: {out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("no tenant subdomain resolved"),
        "should name what's missing: {err}"
    );
    // Every layer of the chain must be reachable from the message.
    assert!(err.contains("--subdomain"), "missing flag layer: {err}");
    assert!(
        err.contains("STAVE_SUBDOMAIN"),
        "missing env layer: {err}"
    );
    assert!(
        err.contains("stave auth login --subdomain"),
        "missing auth-login persistence layer: {err}"
    );
    assert!(
        err.contains("stave config set subdomain"),
        "missing config-set persistence layer: {err}"
    );
}

#[test]
fn list_with_base_url_override_skips_subdomain_chain() {
    // STAVE_BASE_URL bypasses the subdomain chain entirely; we fall
    // through to whatever happens next (here, network failure to a
    // closed port). Chain message must NOT appear.
    let out = cmd()
        .args(["list", "device"])
        .env("STAVE_API_TOKEN", "fake-tok")
        .env("STAVE_BASE_URL", "http://127.0.0.1:1")
        .output()
        .unwrap();

    assert!(
        !out.status.success(),
        "expected network failure (port 1 is closed): {out:?}"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("no tenant subdomain resolved"),
        "chain error must not fire when STAVE_BASE_URL is set: {err}"
    );
}

#[test]
fn list_with_subdomain_flag_skips_chain_error() {
    // Providing the flag short-circuits the chain; DNS resolution of
    // the fabricated tenant host fails afterwards, which is fine.
    let dir = tempdir();
    let cfg = dir.join("nonexistent.toml");
    let out = cmd()
        .args(["list", "device", "--subdomain", "stave-test-nonexistent"])
        .env("STAVE_API_TOKEN", "fake-tok")
        .env("STAVE_CONFIG", &cfg)
        .output()
        .unwrap();

    assert!(!out.status.success(), "expected network failure: {out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("no tenant subdomain resolved"),
        "chain error must not fire when --subdomain is provided: {err}"
    );
}
