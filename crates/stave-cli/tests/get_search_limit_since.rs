//! End-to-end tests for `stave get`, `stave search`, `--limit`,
//! `--since`, and the write-guard's CLI surface.
//!
//! These tests exercise validation and error paths that don't require
//! network; wire-level coverage lives in `wiremock_endpoint.rs`.

use std::process::Command;

use assert_cmd::cargo::CommandCargoExt;

fn run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::cargo_bin("stave").expect("stave binary");
    cmd.args(args);
    // Block the resolvers from finding real credentials/config.
    cmd.env_remove("STAVE_API_TOKEN");
    cmd.env_remove("STAVE_SUBDOMAIN");
    cmd.env_remove("STAVE_ALLOW_WRITE");
    cmd.env("STAVE_CONFIG", "/nonexistent/stave-test-config.toml");
    cmd.env("STAVE_AUDIT", "off");
    cmd.output().expect("output")
}

#[test]
fn get_rejects_kind_without_get_endpoint() {
    // tag has list but no get-by-id in v0.1.
    let out = run(&["get", "tag", "tag_001"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no get-by-id endpoint"), "stderr: {stderr}");
    assert!(stderr.contains("stave list tag"), "stderr: {stderr}");
}

#[test]
fn list_since_rejects_kind_without_primary_timestamp() {
    // blueprint has a list endpoint but no primary timestamp field, so
    // --since has nothing to compare against.
    let out = run(&["list", "blueprint", "--since", "24h"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no primary timestamp field"),
        "stderr: {stderr}"
    );
}

#[test]
fn list_since_rejects_garbage_duration() {
    let out = run(&["list", "device", "--since", "not-a-real-duration"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--since") || stderr.contains("CEL"),
        "stderr: {stderr}"
    );
}

#[test]
fn list_since_quote_in_value_is_rejected_explicitly() {
    let out = run(&["list", "device", "--since", r#"24h" || true"#]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("must not contain quotes"),
        "stderr: {stderr}"
    );
}

#[test]
fn list_since_rejects_days_unit_before_network() {
    // Go durations have no `d`; the pre-validation fires before auth
    // or network, so the message is about the duration, not the token.
    let out = run(&["list", "device", "--since", "7d"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid duration"), "stderr: {stderr}");
    assert!(
        !stderr.contains("no token found"),
        "duration validation must fire before auth: {stderr}"
    );
}

#[test]
fn api_mutating_op_refused_without_allow_write() {
    // The write-guard fires before auth/subdomain resolution errors
    // would even matter — but with no token we'd fail at auth first,
    // so provide a fake token + base URL override; the guard must
    // reject before any network I/O (port 1 would refuse anyway, and
    // the message must be the guard's, not a network error).
    let mut cmd = Command::cargo_bin("stave").expect("stave binary");
    cmd.args(["api", "delete_devices_device_id", "--param", "device_id=x"]);
    cmd.env("STAVE_API_TOKEN", "fake-tok");
    cmd.env("STAVE_BASE_URL", "http://127.0.0.1:1");
    cmd.env_remove("STAVE_ALLOW_WRITE");
    cmd.env("STAVE_CONFIG", "/nonexistent/stave-test-config.toml");
    cmd.env("STAVE_AUDIT", "off");
    let out = cmd.output().expect("output");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("write-guard"), "stderr: {stderr}");
    assert!(stderr.contains("--allow-write"), "stderr: {stderr}");
    assert!(stderr.contains("STAVE_ALLOW_WRITE"), "stderr: {stderr}");
    assert!(
        !stderr.contains("network"),
        "guard must fire before any network attempt: {stderr}"
    );
}

#[test]
fn mcp_call_non_read_tool_refused_without_allow_write() {
    // The MCP write-guard uses the read-shaped-name heuristic and
    // fires before credential resolution, so no MCP setup is needed.
    let mut cmd = Command::cargo_bin("stave").expect("stave binary");
    cmd.args([
        "mcp",
        "call",
        "erase-device",
        "--args",
        r#"{"device_id":"x"}"#,
    ]);
    cmd.env_remove("STAVE_ALLOW_WRITE");
    cmd.env_remove("STAVE_MCP_API_KEY");
    cmd.env("STAVE_CONFIG", "/nonexistent/stave-test-config.toml");
    cmd.env("STAVE_AUDIT", "off");
    let out = cmd.output().expect("output");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("write-guard"), "stderr: {stderr}");
    assert!(stderr.contains("erase-device"), "stderr: {stderr}");
    assert!(stderr.contains("--allow-write"), "stderr: {stderr}");
}

#[test]
fn ops_show_marks_mutating_operations() {
    let out = run(&["ops", "show", "delete_devices_device_id"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("write-guard: mutating"),
        "ops show must flag mutating ops: {stdout}"
    );

    let out = run(&["ops", "show", "get_devices"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("write-guard"),
        "reads carry no write-guard note: {stdout}"
    );
}

#[test]
fn list_help_mentions_limit_and_since() {
    let out = run(&["list", "--help"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--limit"));
    assert!(stdout.contains("--since"));
}

#[test]
fn get_help_mentions_id_path_param_concept() {
    let out = run(&["get", "--help"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("id path param"), "stdout: {stdout}");
}

#[test]
fn search_help_mentions_search_field_concept() {
    let out = run(&["search", "--help"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("search field"));
}
