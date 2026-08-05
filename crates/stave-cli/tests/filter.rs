//! End-to-end coverage for `stave filter --where '<CEL>'` over the
//! synthetic Wiz fixtures.
//!
//! The predicate language is the part of stave an agent is most likely
//! to get wrong, so these tests pin the adapter rules the CLI documents:
//! top-level fields bind as bare variables, the whole record binds as
//! `record` for `has()`, camelCase `*At` fields promote to timestamps so
//! they compare against `now`, and a predicate that does not return a
//! boolean is an error rather than a silent drop.

mod common;

use common::{Sandbox, fixture, ids, jsonl, run, run_with_stdin, stderr_of, stdout_of};

fn filter(kind: &str, predicate: &str) -> std::process::Output {
    let sandbox = Sandbox::new();
    run_with_stdin(
        sandbox
            .cmd()
            .args(["filter", "--where", predicate])
            .env("STAVE_AUDIT", "off"),
        &fixture(kind),
    )
}

fn kept(kind: &str, predicate: &str) -> Vec<String> {
    let out = filter(kind, predicate);
    assert!(
        out.status.success(),
        "predicate `{predicate}` failed: {}",
        stderr_of(&out)
    );
    ids(&stdout_of(&out))
}

// ---------------------------------------------------------------------------
// scalar predicates on Wiz nouns
// ---------------------------------------------------------------------------

#[test]
fn keeps_issues_matching_a_severity_enum() {
    assert_eq!(kept("issue", r#"severity == "CRITICAL""#), ["issue_01"]);
}

#[test]
fn keeps_issues_matching_a_severity_set() {
    assert_eq!(
        kept("issue", r#"severity in ["CRITICAL", "HIGH"]"#),
        ["issue_01", "issue_02"]
    );
}

#[test]
fn combines_kind_and_status_in_one_predicate() {
    // `_kind` is re-exposed so a mixed stream can be narrowed in one pass.
    assert_eq!(
        kept("issue", r#"_kind == "issue" && status == "OPEN""#),
        ["issue_01", "issue_03"]
    );
}

#[test]
fn vulnerability_findings_carry_vendor_severity_not_severity() {
    // The severity carrier differs per kind, which is exactly why the
    // severity-roll-up recipe exists. Here the raw field is asserted.
    assert_eq!(
        kept("vulnerability_finding", r#"vendorSeverity == "HIGH""#),
        ["vf_02", "vf_03"]
    );
}

#[test]
fn string_methods_work_on_finding_names() {
    assert_eq!(
        kept("vulnerability_finding", r#"name.startsWith("CVE-2026")"#),
        ["vf_01", "vf_02", "vf_03"]
    );
}

#[test]
fn keeps_cloud_resources_matching_a_resource_type() {
    assert_eq!(
        kept("cloud_resource", r#"type == "VIRTUAL_MACHINE""#),
        ["res_02", "res_03"]
    );
}

#[test]
fn keeps_cloud_resources_by_platform() {
    assert_eq!(
        kept("cloud_resource", r#"cloudPlatform == "Azure""#),
        ["res_03", "res_04"]
    );
}

#[test]
fn matches_nothing_without_failing() {
    let out = filter("issue", r#"severity == "INFORMATIONAL""#);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(
        stdout_of(&out).trim().is_empty(),
        "an empty result is success, not failure"
    );
}

// ---------------------------------------------------------------------------
// null and absence
// ---------------------------------------------------------------------------

#[test]
fn a_null_field_is_bound_and_comparable() {
    // Wiz sends `resolvedAt: null` on an open issue rather than omitting
    // the key, so the null-vs-absent distinction is real in this data.
    assert_eq!(kept("issue", "record.resolvedAt != null"), ["issue_04"]);
}

#[test]
fn has_macro_answers_presence_through_the_record_view() {
    // Every fixture issue carries the key, so `has()` is true throughout.
    // The point under test is that the macro works at all, since CEL only
    // accepts field access on a map.
    assert_eq!(
        kept("issue", "has(record.resolvedAt)").len(),
        4,
        "the key is present on every record, null or not"
    );
}

#[test]
fn has_macro_returns_false_for_a_field_the_kind_does_not_carry() {
    assert!(
        kept("issue", "has(record.vendorSeverity)").is_empty(),
        "vendorSeverity belongs to vulnerability_finding, not issue"
    );
}

#[test]
fn a_nested_entity_snapshot_is_reachable_behind_a_null_guard() {
    // `entitySnapshot` is null on a resolved issue. Guarding first keeps
    // the predicate total; without the guard the field access on null is
    // a runtime error, which is the canonical-adapter behavior.
    assert_eq!(
        kept(
            "issue",
            r#"record.entitySnapshot != null && entitySnapshot.cloudPlatform == "AWS""#
        ),
        ["issue_01", "issue_02"]
    );
}

#[test]
fn reaching_into_a_null_snapshot_without_a_guard_is_a_runtime_error() {
    let out = filter("issue", r#"entitySnapshot.cloudPlatform == "Azure""#);
    assert!(
        !out.status.success(),
        "a missing field is an error, never a silent null"
    );
    let err = stderr_of(&out);
    assert!(err.contains("CEL runtime error"), "{err}");
}

// ---------------------------------------------------------------------------
// timestamp promotion
// ---------------------------------------------------------------------------

#[test]
fn camel_case_at_fields_promote_for_comparison_with_now() {
    // Wiz uses camelCase (`createdAt`), unlike the snake_case streams the
    // adapter was first written for. Every fixture issue predates any
    // plausible run, so `< now` keeps all four.
    assert_eq!(kept("issue", "createdAt < now").len(), 4);
}

#[test]
fn first_detected_at_promotes_on_vulnerability_findings() {
    assert_eq!(
        kept("vulnerability_finding", "firstDetectedAt < now").len(),
        5
    );
}

#[test]
fn a_duration_window_wide_enough_to_be_date_stable_keeps_everything() {
    // Ten years back. Deliberately not a tight window: a fixed fixture
    // date plus a tight window makes the test expire.
    assert_eq!(
        kept("issue", r#"createdAt > now - duration("87600h")"#).len(),
        4
    );
}

#[test]
fn a_future_window_keeps_nothing() {
    assert!(kept("issue", "createdAt > now").is_empty());
}

#[test]
fn two_timestamp_fields_compare_against_each_other() {
    // `updatedAt >= createdAt` holds for every fixture record, and both
    // sides had to promote for the comparison to typecheck.
    assert_eq!(kept("issue", "updatedAt >= createdAt").len(), 4);
}

// ---------------------------------------------------------------------------
// errors and diagnostics
// ---------------------------------------------------------------------------

#[test]
fn rejects_a_predicate_that_does_not_return_a_boolean() {
    let out = filter("issue", "severity");
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("must return bool"), "{err}");
    assert!(
        err.contains("severity"),
        "the message must quote the predicate: {err}"
    );
}

#[test]
fn rejects_a_malformed_stream_line() {
    let sandbox = Sandbox::new();
    let out = run_with_stdin(
        sandbox
            .cmd()
            .args(["filter", "--where", "_kind == \"issue\""])
            .env("STAVE_AUDIT", "off"),
        "{not json}\n",
    );
    assert!(!out.status.success(), "{out:?}");
    assert!(!stderr_of(&out).is_empty(), "a parse failure must say so");
}

#[test]
fn explain_prints_the_predicate_the_now_binding_and_the_kind_table() {
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args([
            "filter",
            "--where",
            r#"severity == "CRITICAL""#,
            "--explain",
        ])
        .env("STAVE_AUDIT", "off"));
    assert!(out.status.success(), "{}", stderr_of(&out));
    let stdout = stdout_of(&out);
    assert!(
        stdout.contains(r#"predicate: severity == "CRITICAL""#),
        "{stdout}"
    );
    assert!(stdout.contains("now:"), "{stdout}");
    assert!(stdout.contains("ast:"), "{stdout}");
    assert!(stdout.contains("v0.1 kind schemas"), "{stdout}");
    // The table is the authoring aid: it must name the per-kind fields a
    // predicate author cannot otherwise guess.
    assert!(stdout.contains("issue"), "{stdout}");
    assert!(stdout.contains("vulnerability_finding"), "{stdout}");
    assert!(stdout.contains("createdAt"), "{stdout}");
    assert!(stdout.contains("vendorSeverity"), "{stdout}");
    assert!(stdout.contains("firstDetectedAt"), "{stdout}");
}

#[test]
fn explain_does_not_read_stdin() {
    // A stdin-reading explain would hang in a pipeline. The fixture is
    // written but must be ignored, so no records may appear.
    let sandbox = Sandbox::new();
    let out = run_with_stdin(
        sandbox
            .cmd()
            .args(["filter", "--where", "_kind == \"issue\"", "--explain"])
            .env("STAVE_AUDIT", "off"),
        &fixture("issue"),
    );
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(
        !stdout_of(&out).contains("issue_01"),
        "explain must not stream records: {}",
        stdout_of(&out)
    );
}

// ---------------------------------------------------------------------------
// composition
// ---------------------------------------------------------------------------

#[test]
fn filter_output_is_a_valid_stream_for_the_next_primitive() {
    let out = filter("issue", r#"severity in ["CRITICAL", "HIGH"]"#);
    assert!(out.status.success(), "{}", stderr_of(&out));

    let sandbox = Sandbox::new();
    let emitted = run_with_stdin(
        sandbox
            .cmd()
            .args(["emit", "--format", "md"])
            .env("STAVE_AUDIT", "off"),
        &stdout_of(&out),
    );
    assert!(emitted.status.success(), "{}", stderr_of(&emitted));
    let table = stdout_of(&emitted);
    assert!(
        table.contains("| _kind | id | severity | timestamp |"),
        "{table}"
    );
    assert!(table.contains("issue_01"), "{table}");
    assert!(table.contains("CRITICAL"), "{table}");
    assert!(
        !table.contains("issue_03"),
        "the dropped record must not reappear: {table}"
    );
}

#[test]
fn filter_preserves_the_source_reference_of_every_kept_record() {
    let out = filter("issue", r#"severity == "CRITICAL""#);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let records = jsonl(&stdout_of(&out));
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["_source"]["operation_id"], "list_issues");
    assert_eq!(records[0]["_source"]["response_index"], 0);
}
