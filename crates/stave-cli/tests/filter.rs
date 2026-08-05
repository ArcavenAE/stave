//! End-to-end tests for `stave filter`.

use std::io::Write;
use std::process::{Command, Stdio};

use assert_cmd::cargo::CommandCargoExt;

fn fixture(name: &str) -> String {
    let path = format!("../../examples/fixtures/{name}.jsonl");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {path}: {e}"))
}

fn run_filter(input: &str, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::cargo_bin("stave").expect("stave binary");
    cmd.arg("filter").args(args);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write");
    child.wait_with_output().expect("wait")
}

#[test]
fn keeps_records_matching_string_equality() {
    let out = run_filter(
        &fixture("vulnerability"),
        &["--where", r#"severity == "critical""#],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("\"cve_id\":\"CVE-2026-1111\""));
}

#[test]
fn keeps_records_matching_in_operator() {
    let out = run_filter(
        &fixture("vulnerability"),
        &["--where", r#"severity in ["critical", "high"]"#],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);
}

#[test]
fn supports_platform_triage_predicate() {
    // The device-fleet analog of sidestep's triage predicate: Macs on
    // an out-of-date OS line.
    let out = run_filter(
        &fixture("device"),
        &[
            "--where",
            r#"platform == "Mac" && os_version.startsWith("14.")"#,
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "only osprey is a 14.x Mac: {lines:?}");
    assert!(lines[0].contains("\"device_name\":\"osprey\""));
}

#[test]
fn has_macro_works_via_record_view() {
    let out = run_filter(&fixture("device"), &["--where", "has(record.asset_tag)"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    // kestrel and rocinante carry asset tags in the fixture.
    assert_eq!(lines.len(), 2);
}

#[test]
fn explain_prints_predicate_and_schema_without_consuming_stdin() {
    let mut cmd = Command::cargo_bin("stave").expect("stave binary");
    cmd.args(["filter", "--where", r#"severity == "high""#, "--explain"]);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = cmd.output().expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("predicate: severity == \"high\""));
    assert!(stdout.contains("now:"));
    assert!(stdout.contains("ast:"));
    assert!(stdout.contains("v0.1 kind schemas"));
    assert!(stdout.contains("device"));
    assert!(stdout.contains("vulnerability"));
}

#[test]
fn rejects_predicate_returning_non_bool() {
    let out = run_filter(&fixture("vulnerability"), &["--where", r#"severity"#]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("must return bool"), "stderr: {stderr}");
}

#[test]
fn date_suffix_field_promotes_for_comparison_with_now() {
    // `first_detection_date` exercises the `*_date` promotion rule the
    // iru adapter adds on top of sidestep's `*_at`/`ts` set. All
    // fixture dates are in the past, so `< now` keeps everything.
    let out = run_filter(
        &fixture("vulnerability"),
        &["--where", "first_detection_date < now"],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        stdout.lines().count(),
        fixture("vulnerability").lines().count()
    );
}

#[test]
fn last_check_in_promotes_for_comparison_with_now() {
    // Device staleness is the canonical iru fleet question: which
    // devices haven't checked in for N days?
    let out = run_filter(
        &fixture("device"),
        &["--where", r#"last_check_in < now - duration("720h")"#],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    // Only osprey (2026-05-01) is more than 30 days stale relative to
    // any plausible test-run date after 2026-07-15.
    assert_eq!(lines.len(), 1, "expected only osprey stale: {lines:?}");
    assert!(lines[0].contains("osprey"));
}

#[test]
fn filter_then_emit_md_pipeline() {
    // Compose `filter` and `emit` in one process tree.
    let mut filter = Command::cargo_bin("stave").expect("stave");
    filter.args([
        "filter",
        "--where",
        r#"_kind == "vulnerability" && severity == "critical""#,
    ]);
    filter.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut filter_child = filter.spawn().expect("spawn filter");
    filter_child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(fixture("vulnerability").as_bytes())
        .unwrap();
    drop(filter_child.stdin.take());
    let filter_out = filter_child.wait_with_output().expect("wait filter");
    assert!(filter_out.status.success());

    let mut emit = Command::cargo_bin("stave").expect("stave");
    emit.args(["emit", "--format", "md"]);
    emit.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut emit_child = emit.spawn().expect("spawn emit");
    emit_child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&filter_out.stdout)
        .unwrap();
    drop(emit_child.stdin.take());
    let emit_out = emit_child.wait_with_output().expect("wait emit");
    assert!(emit_out.status.success());

    let table = String::from_utf8(emit_out.stdout).unwrap();
    assert!(table.contains("| _kind | id | severity | timestamp |"));
    assert!(table.contains("CVE-2026-1111"));
    assert!(table.contains("critical"));
    assert!(!table.contains("CVE-2026-2222"));
}
