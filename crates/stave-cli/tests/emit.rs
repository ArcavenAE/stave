//! End-to-end coverage for `stave emit`.
//!
//! `emit` is the sink of the stream contract, so these tests pin the
//! contract itself: `jsonl` round-trips a record byte-for-byte in
//! meaning, `md` renders the four columns the kind table can supply for
//! any kind, and `json` collects the stream into one array. A kind that
//! declares no severity or timestamp renders blank cells rather than
//! failing, because the v0.1 kind metadata is provisional (charter F1)
//! and a wrong guess must degrade instead of breaking a pipeline.

mod common;

use common::{Sandbox, fixture, jsonl, run_with_stdin, stderr_of, stdout_of};
use serde_json::Value;

/// One synthetic record per kind, the mixed stream `emit` is meant to
/// render. Written out literally rather than read from a fixture so the
/// expected column values sit next to the assertions.
const ISSUE_LINE: &str = r#"{"_kind":"issue","_source":{"operation_id":"list_issues","response_index":0,"fetched_at":"2026-08-05T10:00:00Z"},"id":"issue_01","type":"TOXIC_COMBINATION","severity":"CRITICAL","status":"OPEN","createdAt":"2026-07-28T09:15:00Z"}"#;

const FINDING_LINE: &str = r#"{"_kind":"vulnerability_finding","_source":{"operation_id":"list_vulnerability_findings","response_index":0,"fetched_at":"2026-08-05T10:00:00Z"},"id":"vf_01","name":"CVE-2026-10001","vendorSeverity":"CRITICAL","firstDetectedAt":"2026-07-20T02:00:00Z"}"#;

const RESOURCE_LINE: &str = r#"{"_kind":"cloud_resource","_source":{"operation_id":"list_cloud_resources","response_index":0,"fetched_at":"2026-08-05T10:00:00Z"},"id":"res_01","name":"example-corp-audit-logs","type":"BUCKET","cloudPlatform":"AWS"}"#;

fn emit(input: &str, args: &[&str]) -> std::process::Output {
    let sandbox = Sandbox::new();
    run_with_stdin(
        sandbox
            .cmd()
            .arg("emit")
            .args(args)
            .env("STAVE_AUDIT", "off"),
        input,
    )
}

// ---------------------------------------------------------------------------
// jsonl
// ---------------------------------------------------------------------------

#[test]
fn jsonl_passes_records_through() {
    let input = format!("{ISSUE_LINE}\n{FINDING_LINE}\n");
    let out = emit(&input, &["--format", "jsonl"]);
    assert!(out.status.success(), "{}", stderr_of(&out));

    let records = jsonl(&stdout_of(&out));
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["_kind"], "issue");
    assert_eq!(records[0]["id"], "issue_01");
    assert_eq!(records[0]["_source"]["operation_id"], "list_issues");
    assert_eq!(records[1]["_kind"], "vulnerability_finding");
    assert_eq!(records[1]["name"], "CVE-2026-10001");
}

#[test]
fn jsonl_round_trips_a_nested_entity_snapshot() {
    // The issue fixture is the only kind with a nested object, and a
    // lossy passthrough there would break `entity-hoist` downstream.
    let out = emit(&fixture("issue"), &["--format", "jsonl"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let records = jsonl(&stdout_of(&out));
    assert_eq!(records.len(), 4);
    assert_eq!(
        records[0]["entitySnapshot"]["subscriptionExternalId"],
        "123456789012"
    );
    assert!(
        records[3]["entitySnapshot"].is_null(),
        "an explicit null must stay null: {}",
        records[3]
    );
}

#[test]
fn an_empty_stream_emits_nothing_and_succeeds() {
    let out = emit("", &["--format", "jsonl"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(out.stdout.is_empty());
}

#[test]
fn blank_lines_in_the_input_are_skipped() {
    let input = format!("\n{ISSUE_LINE}\n\n{FINDING_LINE}\n\n");
    let out = emit(&input, &["--format", "jsonl"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(stdout_of(&out).lines().count(), 2);
}

#[test]
fn a_line_that_is_not_a_stream_record_fails_loudly() {
    let out = emit("{\"not\":\"a record\"}\n", &["--format", "jsonl"]);
    assert!(
        !out.status.success(),
        "a record without _kind/_source is not in the contract: {out:?}"
    );
    assert!(!stderr_of(&out).is_empty());
}

// ---------------------------------------------------------------------------
// md
// ---------------------------------------------------------------------------

#[test]
fn md_renders_the_kind_table_columns_for_each_kind() {
    let input = format!("{ISSUE_LINE}\n{FINDING_LINE}\n");
    let out = emit(&input, &["--format", "md"]);
    assert!(out.status.success(), "{}", stderr_of(&out));

    let stdout = stdout_of(&out);
    let mut lines = stdout.lines();
    assert_eq!(lines.next(), Some("| _kind | id | severity | timestamp |"));
    assert_eq!(lines.next(), Some("|---|---|---|---|"));

    let issue_row = lines.next().expect("issue row");
    assert!(issue_row.contains("issue_01"), "{issue_row}");
    assert!(issue_row.contains("CRITICAL"), "{issue_row}");
    assert!(
        issue_row.contains("2026-07-28T09:15:00Z"),
        "the timestamp column reads createdAt for an issue: {issue_row}"
    );

    let finding_row = lines.next().expect("finding row");
    assert!(finding_row.contains("vf_01"), "{finding_row}");
    assert!(
        finding_row.contains("CRITICAL"),
        "the severity column reads vendorSeverity for a finding: {finding_row}"
    );
    assert!(
        finding_row.contains("2026-07-20T02:00:00Z"),
        "the timestamp column reads firstDetectedAt for a finding: {finding_row}"
    );
}

#[test]
fn md_leaves_cells_blank_for_a_kind_with_no_severity_or_timestamp() {
    // cloud_resource declares neither. Provisional kind metadata must
    // degrade to blanks, never to an error (charter F1).
    let out = emit(&format!("{RESOURCE_LINE}\n"), &["--format", "md"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let row = stdout_of(&out)
        .lines()
        .nth(2)
        .expect("one data row")
        .to_string();
    assert_eq!(
        row, "| cloud_resource | res_01 |  |  |",
        "unexpected row shape: {row}"
    );
}

#[test]
fn md_renders_a_header_even_for_an_empty_stream() {
    let out = emit("", &["--format", "md"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let stdout = stdout_of(&out);
    assert_eq!(
        stdout.lines().count(),
        2,
        "header and rule only: {stdout:?}"
    );
}

#[test]
fn md_emits_one_header_for_a_whole_fixture() {
    let out = emit(&fixture("vulnerability_finding"), &["--format", "md"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let stdout = stdout_of(&out);
    assert_eq!(
        stdout
            .matches("| _kind | id | severity | timestamp |")
            .count(),
        1,
        "the header must land once, ahead of the rows: {stdout}"
    );
    assert_eq!(stdout.lines().count(), 7, "2 header lines plus 5 records");
}

// ---------------------------------------------------------------------------
// json
// ---------------------------------------------------------------------------

#[test]
fn json_collects_the_stream_into_one_pretty_array() {
    let input = format!("{ISSUE_LINE}\n{FINDING_LINE}\n");
    let out = emit(&input, &["--format", "json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));

    let stdout = stdout_of(&out);
    assert!(
        stdout.contains("\n  {"),
        "output must be pretty-printed: {stdout}"
    );
    let parsed: Value = serde_json::from_str(&stdout).expect("one JSON document");
    let array = parsed.as_array().expect("a JSON array");
    assert_eq!(array.len(), 2);
    assert_eq!(array[0]["_kind"], "issue");
    assert_eq!(array[1]["_kind"], "vulnerability_finding");
}

#[test]
fn json_renders_an_empty_stream_as_an_empty_array() {
    let out = emit("", &["--format", "json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let parsed: Value = serde_json::from_str(stdout_of(&out).trim()).expect("one JSON document");
    assert_eq!(parsed.as_array().map(Vec::len), Some(0));
}

// ---------------------------------------------------------------------------
// defaults
// ---------------------------------------------------------------------------

#[test]
fn the_default_format_on_a_pipe_is_jsonl() {
    // TTY auto-detection, never auto-coercion: with stdout piped (which
    // is what a pipeline and an agent both see), the machine-readable
    // form has to be the default.
    let out = emit(&format!("{ISSUE_LINE}\n"), &[]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let stdout = stdout_of(&out);
    assert!(
        !stdout.contains("| _kind |"),
        "a pipe must not get a markdown table: {stdout}"
    );
    assert_eq!(jsonl(&stdout).len(), 1);
}

#[test]
fn emit_writes_nothing_to_stderr_on_the_happy_path() {
    // Rule of silence: stdout carries the contract, and a successful run
    // has nothing to say.
    let out = emit(&format!("{ISSUE_LINE}\n"), &["--format", "jsonl"]);
    assert!(out.status.success());
    assert!(
        out.stderr.is_empty(),
        "unexpected chatter: {}",
        stderr_of(&out)
    );
}

#[test]
fn emit_leaves_no_audit_line() {
    // `emit` is a formatter, not an operation. Auditing it would add a
    // line per pipeline stage without adding a fact.
    let sandbox = Sandbox::new();
    let out = run_with_stdin(
        sandbox.cmd().args(["emit", "--format", "jsonl"]),
        &format!("{ISSUE_LINE}\n"),
    );
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(
        sandbox.audit_lines().is_empty(),
        "emit must not audit: {:?}",
        sandbox.audit_lines()
    );
}

#[test]
fn rejects_an_unknown_format() {
    let out = emit("", &["--format", "yaml"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        stderr_of(&out).to_lowercase().contains("invalid value"),
        "{}",
        stderr_of(&out)
    );
}
