//! End-to-end integration tests against a wiremock mock server.
//!
//! Closes three loops at once:
//!   * the SDK base-URL override (`STAVE_BASE_URL`) actually
//!     reaches the network layer, with path/query/header construction
//!     matching the OpenAPI spec.
//!   * both iru response shapes (bare array + DRF `{results: [...]}`
//!     wrapper) stream correctly through `list`.
//!   * the write-guard refuses mutating operations before any request
//!     is sent, and `--allow-write` deliberately opens the gate.
//!
//! Pattern: `#[tokio::test]` spins up a `MockServer`, mounts a `Mock`
//! with explicit method/path/query/header expectations, then runs the
//! `stave` CLI synchronously via `assert_cmd` with
//! `STAVE_BASE_URL=<server.uri()>`. Mock expectations are verified
//! on `MockServer::drop` — failure to match raises a panic with the
//! actual requests received.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::cargo::CommandCargoExt;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

static TEMPDIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn tempdir(prefix: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = TEMPDIR_COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "stave-{prefix}-{}-{n}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn read_audit_lines(dir: &PathBuf) -> Vec<Value> {
    let mut out = Vec::new();
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for entry in read.flatten() {
        if entry.path().extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let body = std::fs::read_to_string(entry.path()).unwrap();
        for line in body.lines() {
            if line.trim().is_empty() {
                continue;
            }
            out.push(serde_json::from_str(line).expect("audit line is JSON"));
        }
    }
    out
}

/// API-shape lines (those carrying an `operation` block) — distinct
/// from verb-shape lines emitted by `filter` / `enrich`.
fn api_audit_lines(audit_dir: &PathBuf) -> Vec<Value> {
    read_audit_lines(audit_dir)
        .into_iter()
        .filter(|v| v.get("operation").is_some())
        .collect()
}

fn cmd() -> Command {
    let mut c = Command::cargo_bin("stave").expect("stave binary built");
    // Hermetic: no ambient subdomain/region/config/write opt-ins.
    c.env_remove("STAVE_SUBDOMAIN")
        .env_remove("STAVE_REGION")
        .env_remove("STAVE_ALLOW_WRITE")
        .env("STAVE_CONFIG", "/nonexistent/stave-test-config.toml");
    c
}

fn devices_bare_array() -> Value {
    json!([
        {
            "device_id": "dev_001",
            "device_name": "kestrel",
            "platform": "Mac",
            "os_version": "15.5",
            "last_check_in": "2026-07-15T09:12:00Z",
        },
        {
            "device_id": "dev_002",
            "device_name": "osprey",
            "platform": "Mac",
            "os_version": "14.7.1",
            "last_check_in": "2026-05-01T03:40:00Z",
        },
    ])
}

#[tokio::test]
async fn list_devices_streams_bare_array_and_audits() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/devices"))
        .and(query_param("limit", "2"))
        .and(header("authorization", "Bearer fake-tok"))
        .respond_with(ResponseTemplate::new(200).set_body_json(devices_bare_array()))
        .expect(1)
        .mount(&server)
        .await;

    let audit_dir = tempdir("audit");

    let out = cmd()
        .args(["list", "device", "--param", "limit=2"])
        .env("STAVE_API_TOKEN", "fake-tok")
        .env("STAVE_BASE_URL", server.uri())
        .env("STAVE_AUDIT_DIR", &audit_dir)
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "list failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = std::str::from_utf8(&out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 2, "expected 2 records, got {lines:?}");
    let first: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first.get("_kind").and_then(Value::as_str), Some("device"));
    assert_eq!(
        first.get("device_id").and_then(Value::as_str),
        Some("dev_001")
    );

    let api = api_audit_lines(&audit_dir);
    assert_eq!(api.len(), 1, "expected one API audit line, got {api:?}");
    let line = &api[0];
    assert_eq!(line["schema_version"], 2);
    assert_eq!(line["verb_phase"], "list");
    assert_eq!(line["synthesis_keys"][0], "device_id");
    assert_eq!(line["operation"]["id"], "get_devices");
    assert_eq!(
        line["invocation"]["auth_source"], "env",
        "token came from STAVE_API_TOKEN"
    );
    assert_eq!(line["response"]["status"], 200);
    assert_eq!(line["response"]["items_returned"], 2);
}

#[tokio::test]
async fn list_blueprints_unwraps_drf_results_wrapper() {
    let server = MockServer::start().await;
    let body = json!({
        "count": 2,
        "next": null,
        "previous": null,
        "results": [
            {"id": "bp_001", "name": "Mac Fleet"},
            {"id": "bp_002", "name": "Kiosk iPads"},
        ]
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/blueprints"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let audit_dir = tempdir("audit");

    let out = cmd()
        .args(["list", "blueprint"])
        .env("STAVE_API_TOKEN", "fake-tok")
        .env("STAVE_BASE_URL", server.uri())
        .env("STAVE_AUDIT_DIR", &audit_dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = std::str::from_utf8(&out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        2,
        "wrapper response must yield one record per results element, got {lines:?}"
    );
    let first: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(
        first.get("_kind").and_then(Value::as_str),
        Some("blueprint")
    );
}

#[tokio::test]
async fn list_users_records_pagination_cursor_in_audit() {
    let server = MockServer::start().await;
    let body = json!({
        "next": "https://tenant.api.kandji.io/api/v1/users?cursor=abc123",
        "previous": null,
        "results": [ {"id": 7, "email": "amos@example.com"} ]
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let audit_dir = tempdir("audit");

    let out = cmd()
        .args(["list", "user"])
        .env("STAVE_API_TOKEN", "fake-tok")
        .env("STAVE_BASE_URL", server.uri())
        .env("STAVE_AUDIT_DIR", &audit_dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let api = api_audit_lines(&audit_dir);
    assert_eq!(api.len(), 1);
    assert!(
        api[0]["response"]["next_cursor"]
            .as_str()
            .unwrap()
            .contains("cursor=abc123"),
        "DRF next URL must land in response.next_cursor: {:?}",
        api[0]
    );
}

#[tokio::test]
async fn get_device_routes_id_path_param() {
    let server = MockServer::start().await;
    let body = json!({
        "device_id": "dev_001",
        "device_name": "kestrel",
        "platform": "Mac",
        "serial_number": "C02XA0AAAA01",
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/devices/dev_001"))
        .and(header("authorization", "Bearer fake-tok"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .expect(1)
        .mount(&server)
        .await;

    let audit_dir = tempdir("audit");

    let out = cmd()
        .args(["get", "device", "dev_001"])
        .env("STAVE_API_TOKEN", "fake-tok")
        .env("STAVE_BASE_URL", server.uri())
        .env("STAVE_AUDIT_DIR", &audit_dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = std::str::from_utf8(&out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 1, "get emits one record");
    let rec: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(rec.get("_kind").and_then(Value::as_str), Some("device"));
    assert_eq!(
        rec.get("device_id").and_then(Value::as_str),
        Some("dev_001")
    );
}

#[tokio::test]
async fn write_guard_blocks_mutating_op_before_any_request() {
    let server = MockServer::start().await;
    // expect(0): the guard must fire before the request is built.
    Mock::given(method("PATCH"))
        .and(path("/api/v1/devices/dev_001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&server)
        .await;

    let out = cmd()
        .args([
            "api",
            "patch_devices_device_id",
            "--param",
            "device_id=dev_001",
            "--body",
            r#"{"asset_tag":"A-1"}"#,
        ])
        .env("STAVE_API_TOKEN", "fake-tok")
        .env("STAVE_BASE_URL", server.uri())
        .env("STAVE_AUDIT", "off")
        .output()
        .unwrap();

    assert!(!out.status.success(), "guard must reject: {out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("write-guard"), "stderr: {err}");
    assert!(err.contains("patch_devices_device_id"), "stderr: {err}");
    assert!(err.contains("--allow-write"), "stderr: {err}");
    // MockServer::drop verifies expect(0) — no request went out.
}

#[tokio::test]
async fn write_guard_opens_with_allow_write_flag() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v1/devices/dev_001"))
        .and(header("authorization", "Bearer fake-tok"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"device_id": "dev_001", "asset_tag": "A-1"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let audit_dir = tempdir("audit");

    let out = cmd()
        .args([
            "api",
            "patch_devices_device_id",
            "--param",
            "device_id=dev_001",
            "--body",
            r#"{"asset_tag":"A-1"}"#,
            "--allow-write",
        ])
        .env("STAVE_API_TOKEN", "fake-tok")
        .env("STAVE_BASE_URL", server.uri())
        .env("STAVE_AUDIT_DIR", &audit_dir)
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "allow-write PATCH failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = std::str::from_utf8(&out.stdout).unwrap();
    assert!(stdout.contains("asset_tag"), "response echoed: {stdout}");

    let api = api_audit_lines(&audit_dir);
    assert_eq!(api.len(), 1, "mutating call must still audit: {api:?}");
    assert_eq!(api[0]["operation"]["method"], "PATCH");
}

#[tokio::test]
async fn write_guard_opens_with_env_standing_optin() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v1/devices/dev_002"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;

    let out = cmd()
        .args([
            "api",
            "patch_devices_device_id",
            "--param",
            "device_id=dev_002",
            "--body",
            r#"{"asset_tag":"A-2"}"#,
        ])
        .env("STAVE_API_TOKEN", "fake-tok")
        .env("STAVE_BASE_URL", server.uri())
        .env("STAVE_ALLOW_WRITE", "1")
        .env("STAVE_AUDIT", "off")
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "STAVE_ALLOW_WRITE=1 PATCH failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}
