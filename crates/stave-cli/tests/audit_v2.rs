//! End-to-end coverage for the v2 audit schema on the stream-transform
//! verbs.
//!
//! The audit trail is a feature, not a log: it is the dataset the v0.2
//! composite verbs will be mined from (charter F4). So the fields that
//! carry mining signal are pinned here rather than left to inspection:
//!
//!   * `schema_version` is 2 on every emission
//!   * `verb_phase` names which primitive ran
//!   * `filter` records the predicate text plus a value-independent
//!     `predicate_ast_shape`, so predicates cluster across runs without
//!     the trail storing the literals a tenant's data would leak
//!   * `enrich` records the recipe and how much auxiliary data it had
//!   * a verb-shape line carries no `operation` or `response` block
//!
//! API-shape emissions are covered in `wiremock_endpoint.rs`, which has
//! a server to answer them.

mod common;

use common::{Sandbox, fixture, fixture_path, run_with_stdin, stderr_of};
use serde_json::Value;

fn run_filter(sandbox: &Sandbox, predicate: &str, kind: &str) -> std::process::Output {
    run_with_stdin(
        sandbox.cmd().args(["filter", "--where", predicate]),
        &fixture(kind),
    )
}

fn only_line(sandbox: &Sandbox) -> Value {
    let lines = sandbox.audit_lines();
    assert_eq!(lines.len(), 1, "expected exactly one audit line: {lines:?}");
    lines.into_iter().next().expect("one line")
}

fn ast_shape(sandbox: &Sandbox) -> String {
    only_line(sandbox)["predicate_ast_shape"]
        .as_str()
        .expect("predicate_ast_shape is a string")
        .to_string()
}

// ---------------------------------------------------------------------------
// filter
// ---------------------------------------------------------------------------

#[test]
fn filter_emits_one_verb_line_with_the_predicate_and_its_outcome() {
    let sandbox = Sandbox::new();
    let out = run_filter(&sandbox, r#"severity == "CRITICAL""#, "issue");
    assert!(out.status.success(), "{}", stderr_of(&out));

    let line = only_line(&sandbox);
    assert_eq!(line["schema_version"], 2);
    assert_eq!(line["verb_phase"], "filter");
    assert_eq!(line["predicate_text"], r#"severity == "CRITICAL""#);
    assert!(
        line["predicate_ast_shape"]
            .as_str()
            .is_some_and(|s| s.starts_with("sha256:")),
        "{line}"
    );
    assert_eq!(line["predicate_outcome"]["kept_count"], 1);
    assert_eq!(line["predicate_outcome"]["dropped_count"], 3);
    assert_eq!(line["predicate_outcome"]["error_count"], 0);

    assert!(
        line.get("operation").is_none(),
        "a verb has no API operation: {line}"
    );
    assert!(
        line.get("response").is_none(),
        "a verb has no HTTP response: {line}"
    );
}

#[test]
fn filter_records_a_trace_and_span_id_and_a_duration() {
    let sandbox = Sandbox::new();
    let out = run_filter(&sandbox, r#"_kind == "issue""#, "issue");
    assert!(out.status.success(), "{}", stderr_of(&out));

    let line = only_line(&sandbox);
    assert!(line["trace_id"].as_str().is_some(), "{line}");
    assert!(line["span_id"].as_str().is_some(), "{line}");
    assert!(
        line["parent_span_id"].is_null(),
        "a top-level verb has no parent: {line}"
    );
    assert!(line["duration_ms"].as_u64().is_some(), "{line}");
    assert!(
        line["ts_start"].as_str().is_some_and(|t| t.ends_with('Z')),
        "start time is recorded in UTC: {line}"
    );
    assert_eq!(line["invocation"]["argv"][1], "filter");
}

#[test]
fn the_predicate_shape_is_independent_of_the_literals() {
    // Two predicates with the same operators and identifiers but
    // different values must cluster together. This is what lets the
    // mining surface count "how often do we filter on severity" without
    // recording which severities a tenant's issues carry.
    let critical = Sandbox::new();
    assert!(
        run_filter(&critical, r#"severity == "CRITICAL""#, "issue")
            .status
            .success()
    );
    let high = Sandbox::new();
    assert!(
        run_filter(&high, r#"severity == "HIGH""#, "issue")
            .status
            .success()
    );
    assert_eq!(ast_shape(&critical), ast_shape(&high));
}

#[test]
fn the_predicate_shape_changes_when_the_structure_changes() {
    let equality = Sandbox::new();
    assert!(
        run_filter(&equality, r#"severity == "CRITICAL""#, "issue")
            .status
            .success()
    );
    let membership = Sandbox::new();
    assert!(
        run_filter(&membership, r#"severity in ["CRITICAL", "HIGH"]"#, "issue")
            .status
            .success()
    );
    assert_ne!(ast_shape(&equality), ast_shape(&membership));
}

#[test]
fn the_predicate_shape_changes_when_the_field_changes() {
    // Identifiers survive the literal stripping, so filtering a different
    // field is a different shape even at identical structure.
    let severity = Sandbox::new();
    assert!(
        run_filter(&severity, r#"severity == "CRITICAL""#, "issue")
            .status
            .success()
    );
    let status = Sandbox::new();
    assert!(
        run_filter(&status, r#"status == "CRITICAL""#, "issue")
            .status
            .success()
    );
    assert_ne!(ast_shape(&severity), ast_shape(&status));
}

#[test]
fn the_predicate_text_is_recorded_verbatim_including_its_literals() {
    // The shape hash is the value-independent signal; the text is kept
    // whole because a repro needs the exact predicate. This is why
    // tenant-data hygiene treats raw audit lines as unshareable.
    let sandbox = Sandbox::new();
    let predicate = r#"severity == "CRITICAL" && status == "OPEN""#;
    assert!(run_filter(&sandbox, predicate, "issue").status.success());
    assert_eq!(only_line(&sandbox)["predicate_text"], predicate);
}

#[test]
fn a_failing_predicate_still_emits_its_line_with_the_error_counted() {
    // The span finishes on the error path too, so a broken pipeline stage
    // is visible in the trail rather than absent from it.
    let sandbox = Sandbox::new();
    let out = run_filter(
        &sandbox,
        r#"entitySnapshot.cloudPlatform == "AWS""#,
        "issue",
    );
    assert!(!out.status.success(), "expected a runtime error: {out:?}");

    let line = only_line(&sandbox);
    assert_eq!(line["verb_phase"], "filter");
    assert_eq!(line["predicate_outcome"]["error_count"], 1);
    assert_eq!(
        line["predicate_outcome"]["kept_count"], 2,
        "the records processed before the error are still counted"
    );
}

// ---------------------------------------------------------------------------
// enrich
// ---------------------------------------------------------------------------

#[test]
fn enrich_emits_one_verb_line_with_the_recipe_and_its_outcome() {
    let sandbox = Sandbox::new();
    let out = run_with_stdin(
        sandbox.cmd().args([
            "enrich",
            "--with",
            "account-context",
            "--accounts",
            &fixture_path("cloud_account"),
        ]),
        &fixture("cloud_resource"),
    );
    assert!(out.status.success(), "{}", stderr_of(&out));

    let line = only_line(&sandbox);
    assert_eq!(line["schema_version"], 2);
    assert_eq!(line["verb_phase"], "enrich");
    assert_eq!(line["recipe_id"], "account-context");
    assert_eq!(line["transform_outcome"]["transformed_count"], 4);
    assert_eq!(line["transform_outcome"]["error_count"], 0);
    assert_eq!(
        line["auxiliary"]["accounts_loaded"], 2,
        "the count is what the join could index, not the file's line count"
    );
    assert!(line.get("operation").is_none(), "{line}");
}

#[test]
fn enrich_records_the_indexed_auxiliary_count_not_the_line_count() {
    // An account with no externalId cannot participate in the join, so
    // counting the file would overstate what the recipe had to work with.
    let sandbox = Sandbox::new();
    let accounts = sandbox.root().join("accounts.jsonl");
    let mut body = fixture("cloud_account");
    body.push_str(
        r#"{"_kind":"cloud_account","_source":{"operation_id":"list_cloud_accounts","response_index":2,"fetched_at":"2026-08-05T10:00:00Z"},"id":"acct_03","name":"example-corp-unlinked","cloudProvider":"GCP","status":"DISCONNECTED"}"#,
    );
    body.push('\n');
    std::fs::write(&accounts, body).unwrap();

    let out = run_with_stdin(
        sandbox.cmd().args([
            "enrich",
            "--with",
            "account-context",
            "--accounts",
            accounts.to_str().unwrap(),
        ]),
        &fixture("cloud_resource"),
    );
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(
        only_line(&sandbox)["auxiliary"]["accounts_loaded"],
        2,
        "three accounts in the file, two of them joinable"
    );
}

#[test]
fn a_context_free_recipe_records_no_auxiliary_data() {
    let sandbox = Sandbox::new();
    let out = run_with_stdin(
        sandbox.cmd().args(["enrich", "--with", "severity-roll-up"]),
        &fixture("issue"),
    );
    assert!(out.status.success(), "{}", stderr_of(&out));

    let line = only_line(&sandbox);
    assert_eq!(line["recipe_id"], "severity-roll-up");
    assert_eq!(line["transform_outcome"]["transformed_count"], 4);
    assert_eq!(line["auxiliary"]["accounts_loaded"], 0);
}

#[test]
fn entity_hoist_records_its_own_recipe_id() {
    let sandbox = Sandbox::new();
    let out = run_with_stdin(
        sandbox.cmd().args(["enrich", "--with", "entity-hoist"]),
        &fixture("issue"),
    );
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(only_line(&sandbox)["recipe_id"], "entity-hoist");
}

// ---------------------------------------------------------------------------
// pipelines and the off switch
// ---------------------------------------------------------------------------

#[test]
fn a_two_stage_pipeline_leaves_one_line_per_stage() {
    let sandbox = Sandbox::new();

    let filtered = run_filter(&sandbox, r#"severity in ["CRITICAL", "HIGH"]"#, "issue");
    assert!(filtered.status.success(), "{}", stderr_of(&filtered));

    let enriched = run_with_stdin(
        sandbox.cmd().args(["enrich", "--with", "entity-hoist"]),
        &String::from_utf8_lossy(&filtered.stdout),
    );
    assert!(enriched.status.success(), "{}", stderr_of(&enriched));

    let lines = sandbox.audit_lines();
    assert_eq!(lines.len(), 2, "one line per stage: {lines:?}");
    let mut phases: Vec<&str> = lines
        .iter()
        .map(|l| l["verb_phase"].as_str().unwrap_or_default())
        .collect();
    phases.sort_unstable();
    assert_eq!(phases, ["enrich", "filter"]);
    assert_ne!(
        lines[0]["trace_id"], lines[1]["trace_id"],
        "separate invocations are separate traces; correlation is future work"
    );
}

#[test]
fn audit_off_silences_emission_entirely() {
    let sandbox = Sandbox::new();
    let out = run_with_stdin(
        sandbox
            .cmd()
            .args(["filter", "--where", r#"severity == "CRITICAL""#])
            .env("STAVE_AUDIT", "off"),
        &fixture("issue"),
    );
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(
        sandbox.audit_lines().is_empty(),
        "STAVE_AUDIT=off must write nothing: {:?}",
        sandbox.audit_lines()
    );
    assert!(
        !sandbox.audit_dir().exists(),
        "and must not create the directory either"
    );
}

#[test]
fn the_trail_lands_in_one_file_per_day() {
    let sandbox = Sandbox::new();
    for predicate in [r#"severity == "CRITICAL""#, r#"severity == "HIGH""#] {
        assert!(run_filter(&sandbox, predicate, "issue").status.success());
    }
    let files: Vec<_> = std::fs::read_dir(sandbox.audit_dir())
        .expect("audit dir exists")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files.len(), 1, "both runs append to one file: {files:?}");
    assert!(
        files[0].ends_with(".jsonl"),
        "the trail is JSONL: {}",
        files[0]
    );
    assert_eq!(sandbox.audit_lines().len(), 2);
}

#[test]
fn the_invocation_block_records_the_binary_version_and_argv() {
    let sandbox = Sandbox::new();
    assert!(
        run_filter(&sandbox, r#"severity == "CRITICAL""#, "issue")
            .status
            .success()
    );
    let line = only_line(&sandbox);
    let invocation = &line["invocation"];
    assert!(
        invocation["binary_version"].as_str().is_some(),
        "{invocation}"
    );
    assert_eq!(
        invocation["tty"], false,
        "stdout is a pipe under a test harness"
    );
    assert!(
        invocation["auth_source"].is_null(),
        "a stream verb resolves no credential: {invocation}"
    );
}
