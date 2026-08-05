//! End-to-end tests for `stave emit`.
//!
//! These tests exercise the binary via `assert_cmd` and verify that the
//! v0.1 stream contract round-trips cleanly through `--format jsonl` and
//! that `--format md` yields a markdown table with the expected columns.

use std::process::Command;

use assert_cmd::cargo::CommandCargoExt;

const DEVICE_LINE: &str = r#"{"_kind":"device","_source":{"operation_id":"get_devices","response_index":0,"fetched_at":"2026-07-15T10:00:00Z"},"device_id":"dev_001","device_name":"kestrel","platform":"Mac","last_check_in":"2026-07-15T09:12:00Z"}"#;

const VULN_LINE: &str = r#"{"_kind":"vulnerability","_source":{"operation_id":"get_vulnerability_management_vulnerabilities","response_index":0,"fetched_at":"2026-07-15T10:00:00Z"},"cve_id":"CVE-2026-1111","severity":"critical","first_detection_date":"2026-07-10T00:00:00Z"}"#;

#[test]
fn emit_jsonl_passes_records_through() {
    let input = format!("{DEVICE_LINE}\n{VULN_LINE}\n");
    let mut cmd = Command::cargo_bin("stave").expect("stave binary");
    cmd.args(["emit", "--format", "jsonl"]);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    let mut child = cmd.spawn().expect("spawn");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        use std::io::Write;
        stdin.write_all(input.as_bytes()).unwrap();
    }
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("\"_kind\":\"device\""));
    assert!(lines[0].contains("\"device_id\":\"dev_001\""));
    assert!(lines[1].contains("\"_kind\":\"vulnerability\""));
    assert!(lines[1].contains("\"cve_id\":\"CVE-2026-1111\""));
}

#[test]
fn emit_md_renders_a_markdown_table() {
    let input = format!("{DEVICE_LINE}\n{VULN_LINE}\n");
    let mut cmd = Command::cargo_bin("stave").expect("stave binary");
    cmd.args(["emit", "--format", "md"]);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    let mut child = cmd.spawn().expect("spawn");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        use std::io::Write;
        stdin.write_all(input.as_bytes()).unwrap();
    }
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).unwrap();
    let mut lines = stdout.lines();
    assert_eq!(lines.next(), Some("| _kind | id | severity | timestamp |"));
    assert_eq!(lines.next(), Some("|---|---|---|---|"));
    let row1 = lines.next().expect("row 1");
    assert!(row1.contains("device"));
    assert!(row1.contains("dev_001"), "id column uses device_id: {row1}");
    assert!(
        row1.contains("2026-07-15T09:12:00Z"),
        "timestamp column uses last_check_in: {row1}"
    );
    let row2 = lines.next().expect("row 2");
    assert!(row2.contains("vulnerability"));
    assert!(row2.contains("CVE-2026-1111"));
    assert!(row2.contains("critical"));
    assert!(row2.contains("2026-07-10T00:00:00Z"));
}

#[test]
fn emit_passes_through_empty_input() {
    let mut cmd = Command::cargo_bin("stave").expect("stave binary");
    cmd.args(["emit", "--format", "jsonl"]);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    let mut child = cmd.spawn().expect("spawn");
    drop(child.stdin.take()); // close stdin immediately
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn list_rejects_unknown_kind() {
    let mut cmd = Command::cargo_bin("stave").expect("stave binary");
    cmd.args(["list", "definitelyNotAKind"]);
    let out = cmd.output().expect("output");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    // clap rejects with `error: invalid value` per PossibleValuesParser.
    assert!(stderr.to_lowercase().contains("invalid value"));
}
