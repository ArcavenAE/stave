//! Coverage for the read-only permission surface (D5):
//! `stave ops permissions`, `auth scopes`, `auth can-i`, and
//! `auth plan [--check]`.
//!
//! SAFETY (MANDATORY): every test here is hermetic — sandboxed env,
//! keyring disabled, synthetic JWTs, no network. Nothing performs, or
//! could perform, any active, change, remediation, or destructive
//! action against any real system; the production Wiz tenant is never
//! contacted. Scope names are provisional until live validation, so
//! these tests pin the CLI's *shape and logic*, not the vendor's
//! canonical scope list. See
//! docs/design/read-only-permissions-implementation-plan.md.

mod common;

use common::{Sandbox, fake_jwt, run, stderr_of, stdout_of};
use serde_json::Value;

/// A token carrying an explicit set of scopes, injected via
/// `STAVE_ACCESS_TOKEN` so nothing mints and no network is touched.
fn token_with_scopes(scopes: &[&str]) -> String {
    fake_jwt(serde_json::json!({ "scope": scopes.join(" "), "sub": "svc-example" }))
}

fn jsonl(out: &str) -> Vec<Value> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("each line is JSON"))
        .collect()
}

// ---------------------------------------------------------------------------
// ops permissions — offline, pure registry metadata
// ---------------------------------------------------------------------------

#[test]
fn ops_permissions_reports_scopes_and_metadata_offline() {
    // No token, no server: pure registry read.
    let sandbox = Sandbox::new();
    let out = run(sandbox.cmd().args(["ops", "permissions"]));
    assert!(out.status.success(), "{}", stderr_of(&out));
    let rows = jsonl(&stdout_of(&out));
    assert_eq!(rows.len(), 12, "one row per curated operation");
    for row in &rows {
        assert_eq!(row["_kind"], "operation_permissions");
        assert!(
            row["required_scopes"]
                .as_array()
                .is_some_and(|a| !a.is_empty()),
            "every operation declares scopes: {row}"
        );
        assert_eq!(
            row["scopes_provisional"], true,
            "scope metadata is provisional until F1: {row}"
        );
    }
}

#[test]
fn ops_permissions_filters_by_name() {
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["ops", "permissions", "--filter", "issue"]));
    assert!(out.status.success(), "{}", stderr_of(&out));
    let rows = jsonl(&stdout_of(&out));
    assert!(
        !rows.is_empty(),
        "issue filter matches at least list_issues"
    );
    for row in &rows {
        assert!(
            row["name"].as_str().unwrap().contains("issue"),
            "filter leaked a non-match: {row}"
        );
    }
}

// ---------------------------------------------------------------------------
// auth scopes — decoded from the token at hand, no mint
// ---------------------------------------------------------------------------

#[test]
fn auth_scopes_decodes_the_token_at_hand() {
    let sandbox = Sandbox::new();
    let out = run(sandbox.cmd().args(["auth", "scopes"]).env(
        "STAVE_ACCESS_TOKEN",
        token_with_scopes(&["read:issues", "read:projects"]),
    ));
    assert!(out.status.success(), "{}", stderr_of(&out));
    let record: Value = serde_json::from_str(stdout_of(&out).trim()).expect("one JSON object");
    let scopes = record["scopes"].as_array().expect("scopes array");
    assert_eq!(scopes.len(), 2);
    assert_eq!(record["claim_field"], "scope");
    assert_eq!(record["provisional"], true);
}

#[test]
fn auth_scopes_without_a_token_errors_cleanly() {
    let sandbox = Sandbox::new();
    let out = run(sandbox.cmd().args(["auth", "scopes"]));
    assert!(!out.status.success(), "no token: must fail");
    assert!(
        stderr_of(&out).contains("no token scopes available"),
        "{}",
        stderr_of(&out)
    );
}

// ---------------------------------------------------------------------------
// auth can-i — required subset-of granted; exit 0 yes, 1 no
// ---------------------------------------------------------------------------

#[test]
fn can_i_yes_when_the_scope_is_granted() {
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["auth", "can-i", "list_issues"])
        .env("STAVE_ACCESS_TOKEN", token_with_scopes(&["read:issues"])));
    assert!(
        out.status.success(),
        "exit 0 on allowed: {}",
        stderr_of(&out)
    );
    let record: Value = serde_json::from_str(stdout_of(&out).trim()).expect("one JSON object");
    assert_eq!(record["allowed"], true);
    assert!(record["missing_scopes"].as_array().unwrap().is_empty());
}

#[test]
fn can_i_no_and_exits_nonzero_when_the_scope_is_missing() {
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["auth", "can-i", "list_issues"])
        .env("STAVE_ACCESS_TOKEN", token_with_scopes(&["read:projects"])));
    assert_eq!(out.status.code(), Some(1), "exit 1 on not-allowed");
    let record: Value = serde_json::from_str(stdout_of(&out).trim()).expect("one JSON object");
    assert_eq!(record["allowed"], false);
    assert_eq!(record["missing_scopes"][0], "read:issues");
}

#[test]
fn can_i_treats_read_all_as_granting_any_read_scope() {
    // Provisional rule (D3): read:all subsumes read:* until F1 confirms
    // Wiz's implication semantics.
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["auth", "can-i", "list_issues"])
        .env("STAVE_ACCESS_TOKEN", token_with_scopes(&["read:all"])));
    assert!(out.status.success(), "read:all should satisfy read:issues");
}

// ---------------------------------------------------------------------------
// auth plan — GRANT / DO NOT GRANT, and --check missing vs excess
// ---------------------------------------------------------------------------

#[test]
fn plan_lists_grant_and_do_not_grant_for_a_subset() {
    let sandbox = Sandbox::new();
    // Planning for one operation: its scope is GRANT, and at least one
    // other registry scope lands in DO NOT GRANT.
    let out = run(sandbox.cmd().args(["auth", "plan", "--op", "list_issues"]));
    assert!(out.status.success(), "{}", stderr_of(&out));
    let record: Value = serde_json::from_str(stdout_of(&out).trim()).expect("one JSON object");
    let grant = record["grant"].as_array().unwrap();
    assert!(
        grant.iter().any(|s| s == "read:issues"),
        "GRANT must include the selected op's scope: {record}"
    );
    assert!(
        !record["do_not_grant"].as_array().unwrap().is_empty(),
        "other registry scopes must appear in DO NOT GRANT: {record}"
    );
    assert_eq!(record["provisional"], true);
}

#[test]
fn plan_check_reports_missing_and_excess_separately() {
    // Grant read:projects only, plan for list_issues only:
    //   missing = read:issues (unusable), excess = read:projects.
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["auth", "plan", "--op", "list_issues", "--check"])
        .env("STAVE_ACCESS_TOKEN", token_with_scopes(&["read:projects"])));
    assert_eq!(out.status.code(), Some(1), "drift must exit nonzero");
    let record: Value = serde_json::from_str(stdout_of(&out).trim()).expect("one JSON object");
    assert_eq!(record["missing"][0], "read:issues");
    assert_eq!(record["excess"][0], "read:projects");
}

#[test]
fn plan_check_passes_when_scopes_match_exactly() {
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["auth", "plan", "--op", "list_issues", "--check"])
        .env("STAVE_ACCESS_TOKEN", token_with_scopes(&["read:issues"])));
    assert!(
        out.status.success(),
        "exact match must exit 0: {}",
        stderr_of(&out)
    );
    let record: Value = serde_json::from_str(stdout_of(&out).trim()).expect("one JSON object");
    assert!(record["missing"].as_array().unwrap().is_empty());
    assert!(record["excess"].as_array().unwrap().is_empty());
}
