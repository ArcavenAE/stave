//! Named profiles, end to end through the binary.
//!
//! The SDK unit tests cover the decision half (`select_profile`) with
//! every input passed in. These cover the ambient half that unit tests
//! cannot reach from a crate that forbids `unsafe`: the selection chain
//! (`--profile` > `STAVE_PROFILE` > stored default), the config verbs,
//! and the two properties that make profiles a safety feature rather
//! than an ergonomic one.

use assert_cmd::Command;
use tempfile::TempDir;

/// A binary invocation with a private config file and the platform
/// keyring disabled, so a hermetic run never opens the real Keychain.
fn stave(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("stave").expect("binary builds");
    cmd.env("STAVE_CONFIG", dir.path().join("config.toml"))
        .env("STAVE_KEYRING", "off")
        .env("STAVE_AUDIT", "off")
        .env_remove("STAVE_PROFILE")
        .env_remove("STAVE_CLIENT_ID")
        .env_remove("STAVE_CLIENT_SECRET");
    cmd
}

fn add_reader(dir: &TempDir) {
    stave(dir)
        .args([
            "profile",
            "add",
            "reader",
            "--client-id",
            "id-reader",
            "--purpose",
            "day-to-day reads",
        ])
        .assert()
        .success();
}

fn add_provisioner(dir: &TempDir) {
    stave(dir)
        .args([
            "profile",
            "add",
            "provisioner",
            "--client-id",
            "id-provisioner",
            "--plane",
            "provision",
            "--purpose",
            "mints service accounts",
        ])
        .assert()
        .success();
}

#[test]
fn add_then_list_reports_the_profile() {
    let dir = TempDir::new().unwrap();
    add_reader(&dir);

    let out = stave(&dir).args(["profile", "list"]).output().unwrap();
    assert!(out.status.success());
    let body = String::from_utf8(out.stdout).unwrap();
    let record: serde_json::Value = serde_json::from_str(body.trim()).expect("one JSON line");

    assert_eq!(record["name"], "reader");
    assert_eq!(record["plane"], "read");
    assert_eq!(record["purpose"], "day-to-day reads");
    assert_eq!(record["enabled"], true);
}

#[test]
fn list_never_prints_the_client_id() {
    // Client IDs are credential identifiers (tenant-data-hygiene
    // class 4). An operator picks a profile by purpose, not by ID.
    let dir = TempDir::new().unwrap();
    add_reader(&dir);
    add_provisioner(&dir);

    let out = stave(&dir).args(["profile", "list"]).output().unwrap();
    let body = String::from_utf8(out.stdout).unwrap();
    assert!(!body.contains("id-reader"), "client id leaked: {body}");
    assert!(!body.contains("id-provisioner"), "client id leaked: {body}");

    let shown = stave(&dir)
        .args(["profile", "show", "reader"])
        .output()
        .unwrap();
    let shown = String::from_utf8(shown.stdout).unwrap();
    assert!(!shown.contains("id-reader"), "client id leaked: {shown}");
}

#[test]
fn a_disabled_profile_refuses_even_when_named() {
    let dir = TempDir::new().unwrap();
    add_reader(&dir);
    stave(&dir)
        .args(["profile", "disable", "reader"])
        .assert()
        .success();

    let out = stave(&dir)
        .args(["--profile", "reader", "auth", "status"])
        .output()
        .unwrap();
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(!out.status.success(), "should have refused");
    assert!(err.contains("disabled"), "{err}");

    stave(&dir)
        .args(["profile", "enable", "reader"])
        .assert()
        .success();
    let out = stave(&dir)
        .args(["--profile", "reader", "auth", "status"])
        .output()
        .unwrap();
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(
        !err.contains("disabled"),
        "still refusing after enable: {err}"
    );
}

#[test]
fn an_unknown_profile_errors_rather_than_silently_using_another_credential() {
    let dir = TempDir::new().unwrap();
    add_reader(&dir);

    let out = stave(&dir)
        .args(["--profile", "typo", "auth", "status"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("not configured"), "{err}");
    assert!(err.contains("reader"), "known names not listed: {err}");
}

#[test]
fn the_flag_outranks_the_environment_variable() {
    let dir = TempDir::new().unwrap();
    add_reader(&dir);

    // STAVE_PROFILE names a profile that does not exist; --profile names
    // one that does. If the flag wins, this succeeds.
    let out = stave(&dir)
        .env("STAVE_PROFILE", "does-not-exist")
        .args(["--profile", "reader", "profile", "show", "reader"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "flag did not outrank env: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn the_environment_variable_outranks_the_stored_default() {
    let dir = TempDir::new().unwrap();
    add_reader(&dir);
    stave(&dir)
        .args(["config", "set", "profile", "reader"])
        .assert()
        .success();

    let out = stave(&dir)
        .env("STAVE_PROFILE", "does-not-exist")
        .args(["auth", "status"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "stored default should not have won");
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("does-not-exist"), "{err}");
}

#[test]
fn a_provisioning_profile_cannot_be_the_stored_default() {
    // The load-bearing safety property. Every surveyed CLI (aws,
    // gcloud, spacectl, gh, kubectl) lets the active credential come
    // from global state and tells the operator to remember to check.
    // Here an unqualified command can never reach a minting credential.
    let dir = TempDir::new().unwrap();
    add_provisioner(&dir);
    stave(&dir)
        .args(["config", "set", "profile", "provisioner"])
        .assert()
        .success();

    let out = stave(&dir).args(["auth", "status"]).output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("cannot be the stored default"), "{err}");
    assert!(err.contains("--profile provisioner"), "{err}");
}

#[test]
fn the_read_binary_refuses_a_provisioning_profile_named_explicitly() {
    let dir = TempDir::new().unwrap();
    add_provisioner(&dir);

    let out = stave(&dir)
        .args(["--profile", "provisioner", "auth", "status"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("provision plane"), "{err}");
    assert!(err.contains("read plane"), "{err}");
}

#[test]
fn remove_drops_the_profile_and_clears_it_as_the_default() {
    let dir = TempDir::new().unwrap();
    add_reader(&dir);
    stave(&dir)
        .args(["config", "set", "profile", "reader"])
        .assert()
        .success();
    stave(&dir)
        .args(["profile", "remove", "reader"])
        .assert()
        .success();

    let out = stave(&dir).args(["profile", "list"]).output().unwrap();
    assert!(String::from_utf8(out.stdout).unwrap().trim().is_empty());

    // A dangling default would make every later command fail with
    // "profile not configured" for a profile the operator did remove.
    let out = stave(&dir).args(["auth", "status"]).output().unwrap();
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(!err.contains("not configured"), "dangling default: {err}");
}

#[test]
fn removing_an_absent_profile_is_an_error() {
    let dir = TempDir::new().unwrap();
    let out = stave(&dir)
        .args(["profile", "remove", "ghost"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn no_profile_configured_leaves_the_unnamed_credential_path_intact() {
    // An install that predates profiles must be unaffected.
    let dir = TempDir::new().unwrap();
    let out = stave(&dir)
        .env("STAVE_CLIENT_ID", "legacy-id")
        .args(["auth", "status"])
        .output()
        .unwrap();
    let body = String::from_utf8(out.stdout).unwrap() + &String::from_utf8(out.stderr).unwrap();
    assert!(
        !body.contains("not configured"),
        "unnamed path disturbed: {body}"
    );
}

// ---------------------------------------------------------------------------
// enrolment (`auth login --profile`)
// ---------------------------------------------------------------------------
//
// The decisions `auth login` makes (which client ID, which config slot,
// whether to mint) are covered by the `login_plan_tests` unit tests on
// `plan_login` in src/main.rs. They cannot be covered here: the login
// path stores to the platform keyring, the sandbox sets
// STAVE_KEYRING=off so a hermetic run never opens the real Keychain,
// and the store fails before any of those decisions take effect.
//
// That gap is why two defects shipped in the first cut of profiles: the
// client ID was persisted to `[auth]` while a profile was active, and
// the plane check ran after the token mint instead of before it.

#[test]
fn login_refuses_cleanly_when_the_keyring_is_unavailable() {
    // What IS testable here: the failure is the keyring's, reported as
    // such, rather than a partial enrolment that half-wrote config.
    let dir = TempDir::new().unwrap();
    add_reader(&dir);

    let out = stave(&dir)
        .args([
            "--profile",
            "reader",
            "auth",
            "login",
            "--stdin",
            "--no-verify",
        ])
        .write_stdin("the-secret\n")
        .output()
        .unwrap();
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(!out.status.success());
    assert!(err.contains("keyring is disabled"), "{err}");
    // The stored client ID was reused rather than reprompted, which is
    // observable even though the store then failed.
    assert!(
        err.contains("using the client ID stored for profile"),
        "reprompted despite a stored id: {err}"
    );
}
