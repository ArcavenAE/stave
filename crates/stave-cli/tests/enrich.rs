//! End-to-end coverage for `stave enrich --with <recipe>`.
//!
//! All three v0.1 recipes run through the binary against the synthetic
//! Wiz fixtures, so the cross-kind join semantics are enforced at the
//! CLI boundary and not only in the SDK unit tests:
//!
//!   * `account-context` joins a cloud_resource to its owning cloud
//!     account on `subscriptionExternalId`, and marks an orphan
//!     subscription with `account: null` rather than dropping it.
//!   * `severity-roll-up` normalises whichever severity field the kind
//!     happens to carry into one `severity_rollup`.
//!   * `entity-hoist` lifts `issue.entitySnapshot` fields to top level
//!     so a predicate can reach them without a nested path.

mod common;

use common::{Sandbox, fixture, fixture_path, jsonl, run_with_stdin, stderr_of, stdout_of};
use serde_json::Value;

fn enrich(kind: &str, args: &[&str]) -> std::process::Output {
    let sandbox = Sandbox::new();
    run_with_stdin(
        sandbox
            .cmd()
            .arg("enrich")
            .args(args)
            .env("STAVE_AUDIT", "off"),
        &fixture(kind),
    )
}

fn enriched(kind: &str, args: &[&str]) -> Vec<Value> {
    let out = enrich(kind, args);
    assert!(
        out.status.success(),
        "enrich {args:?} failed: {}",
        stderr_of(&out)
    );
    jsonl(&stdout_of(&out))
}

fn by_id(records: &[Value]) -> std::collections::HashMap<String, &Value> {
    records
        .iter()
        .map(|r| {
            (
                r["id"]
                    .as_str()
                    .expect("every record has an id")
                    .to_string(),
                r,
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// account-context (the join)
// ---------------------------------------------------------------------------

#[test]
fn account_context_attaches_the_owning_account_to_each_cloud_resource() {
    let records = enriched(
        "cloud_resource",
        &[
            "--with",
            "account-context",
            "--accounts",
            &fixture_path("cloud_account"),
        ],
    );
    let index = by_id(&records);

    assert_eq!(
        index["res_01"]["account"]["externalId"].as_str(),
        Some("123456789012"),
        "res_01 belongs to the AWS production account"
    );
    assert_eq!(
        index["res_01"]["account"]["name"].as_str(),
        Some("example-corp-prod")
    );
    assert_eq!(
        index["res_01"]["account"]["cloudProvider"].as_str(),
        Some("AWS")
    );
    assert_eq!(
        index["res_03"]["account"]["name"].as_str(),
        Some("example-corp-sandbox"),
        "res_03 belongs to the Azure sandbox subscription"
    );
    assert_eq!(
        index["res_03"]["account"]["status"].as_str(),
        Some("PARTIALLY_CONNECTED"),
        "the join carries account status, which triage needs"
    );
}

#[test]
fn account_context_marks_an_orphan_subscription_with_null_rather_than_dropping_it() {
    // res_04's subscription has no matching account in the auxiliary set.
    // A silent drop would hide a real gap in the connector inventory.
    let records = enriched(
        "cloud_resource",
        &[
            "--with",
            "account-context",
            "--accounts",
            &fixture_path("cloud_account"),
        ],
    );
    let index = by_id(&records);
    assert_eq!(records.len(), 4, "no record is dropped: {records:?}");
    assert!(
        index["res_04"]["account"].is_null(),
        "orphan reference must be data: {}",
        index["res_04"]
    );
}

#[test]
fn account_context_attaches_only_the_summary_fields() {
    // The join keeps the stream compact, so the attached account is a
    // summary and not the whole record.
    let records = enriched(
        "cloud_resource",
        &[
            "--with",
            "account-context",
            "--accounts",
            &fixture_path("cloud_account"),
        ],
    );
    let account = records[0]["account"].as_object().expect("account attached");
    let mut keys: Vec<&str> = account.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["cloudProvider", "externalId", "id", "name", "status"],
        "unexpected summary shape: {account:?}"
    );
    assert!(
        account.get("_source").is_none(),
        "the auxiliary record's provenance is not carried into the join"
    );
}

#[test]
fn account_context_passes_other_kinds_through_untouched() {
    let records = enriched(
        "issue",
        &[
            "--with",
            "account-context",
            "--accounts",
            &fixture_path("cloud_account"),
        ],
    );
    assert_eq!(records.len(), 4);
    for r in &records {
        assert_eq!(r["_kind"].as_str(), Some("issue"));
        assert!(
            r.get("account").is_none(),
            "an issue is not a cloud_resource: {r}"
        );
    }
}

#[test]
fn account_context_requires_the_accounts_flag() {
    let out = enrich("cloud_resource", &["--with", "account-context"]);
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("requires --accounts"), "{err}");
    assert!(
        err.contains("cloud_account"),
        "the error must name the kind it needs: {err}"
    );
}

#[test]
fn account_context_rejects_an_auxiliary_stream_of_the_wrong_kind() {
    // Indexing the wrong kind would produce an all-null join that looks
    // exactly like real orphan data, so this fails loudly instead.
    let out = enrich(
        "cloud_resource",
        &[
            "--with",
            "account-context",
            "--accounts",
            &fixture_path("issue"),
        ],
    );
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("carries a `issue` record"), "{err}");
    assert!(err.contains("cloud_account"), "{err}");
    assert!(
        err.contains("stave list cloud_account"),
        "the error must name how to capture the right stream: {err}"
    );
}

#[test]
fn account_context_reports_a_missing_auxiliary_file() {
    let out = enrich(
        "cloud_resource",
        &[
            "--with",
            "account-context",
            "--accounts",
            "/nonexistent/stave-test-accounts.jsonl",
        ],
    );
    assert!(!out.status.success(), "{out:?}");
    assert!(
        stderr_of(&out).contains("stave-test-accounts.jsonl"),
        "the error must name the path it could not open: {}",
        stderr_of(&out)
    );
}

// ---------------------------------------------------------------------------
// severity-roll-up (the normaliser)
// ---------------------------------------------------------------------------

#[test]
fn severity_rollup_reads_vendor_severity_on_vulnerability_findings() {
    let records = enriched("vulnerability_finding", &["--with", "severity-roll-up"]);
    assert_eq!(records.len(), 5);
    for r in &records {
        assert_eq!(
            r["severity_rollup"].as_str(),
            r["vendorSeverity"].as_str(),
            "vendorSeverity is the carrier for this kind: {r}"
        );
    }
}

#[test]
fn severity_rollup_falls_back_to_severity_on_issues() {
    let records = enriched("issue", &["--with", "severity-roll-up"]);
    assert_eq!(records.len(), 4);
    for r in &records {
        assert_eq!(r["severity_rollup"].as_str(), r["severity"].as_str());
    }
    let index = by_id(&records);
    assert_eq!(
        index["issue_01"]["severity_rollup"].as_str(),
        Some("CRITICAL")
    );
}

#[test]
fn severity_rollup_is_null_for_a_kind_that_carries_no_severity() {
    // A cloud_resource has no severity at all. Null keeps a mixed stream
    // uniform, so a downstream rank predicate need not special-case it.
    let records = enriched("cloud_resource", &["--with", "severity-roll-up"]);
    assert_eq!(records.len(), 4);
    for r in &records {
        assert!(r["severity_rollup"].is_null(), "{r}");
    }
}

#[test]
fn severity_rollup_normalises_a_mixed_stream_into_one_field() {
    // The reason the recipe exists: one predicate over two kinds whose
    // severity lives under different names.
    let sandbox = Sandbox::new();
    let mixed = format!("{}{}", fixture("issue"), fixture("vulnerability_finding"));
    let rolled = run_with_stdin(
        sandbox
            .cmd()
            .args(["enrich", "--with", "severity-roll-up"])
            .env("STAVE_AUDIT", "off"),
        &mixed,
    );
    assert!(rolled.status.success(), "{}", stderr_of(&rolled));

    let filtered = run_with_stdin(
        sandbox
            .cmd()
            .args(["filter", "--where", r#"severity_rollup == "CRITICAL""#])
            .env("STAVE_AUDIT", "off"),
        &stdout_of(&rolled),
    );
    assert!(filtered.status.success(), "{}", stderr_of(&filtered));
    let critical = jsonl(&stdout_of(&filtered));
    let ids: Vec<&str> = critical
        .iter()
        .map(|r| r["id"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        ids,
        ["issue_01", "vf_01"],
        "one predicate must reach both kinds: {ids:?}"
    );
}

// ---------------------------------------------------------------------------
// entity-hoist (the flattener)
// ---------------------------------------------------------------------------

#[test]
fn entity_hoist_lifts_the_snapshot_to_top_level_fields() {
    let records = enriched("issue", &["--with", "entity-hoist"]);
    let index = by_id(&records);
    assert_eq!(
        index["issue_01"]["entity_name"].as_str(),
        Some("example-corp-audit-logs")
    );
    assert_eq!(index["issue_01"]["entity_type"].as_str(), Some("BUCKET"));
    assert_eq!(
        index["issue_01"]["entity_cloud_platform"].as_str(),
        Some("AWS")
    );
}

#[test]
fn entity_hoist_leaves_the_original_snapshot_in_place() {
    let records = enriched("issue", &["--with", "entity-hoist"]);
    let index = by_id(&records);
    assert_eq!(
        index["issue_02"]["entitySnapshot"]["name"].as_str(),
        Some("example-corp-api-01"),
        "hoisting copies, it does not move"
    );
}

#[test]
fn entity_hoist_passes_through_an_issue_with_no_snapshot() {
    // issue_04 is resolved and carries `entitySnapshot: null`.
    let records = enriched("issue", &["--with", "entity-hoist"]);
    let index = by_id(&records);
    assert!(
        index["issue_04"].get("entity_name").is_none(),
        "nothing to hoist means no key: {}",
        index["issue_04"]
    );
}

#[test]
fn entity_hoist_passes_through_kinds_with_no_snapshot_field() {
    let records = enriched("vulnerability_finding", &["--with", "entity-hoist"]);
    assert_eq!(records.len(), 5);
    for r in &records {
        assert!(r.get("entity_name").is_none(), "{r}");
    }
}

#[test]
fn entity_hoist_then_filter_reaches_the_hoisted_field_without_a_null_guard() {
    // The payoff: the same question that needs `record.entitySnapshot !=
    // null && entitySnapshot.cloudPlatform == "AWS"` on the raw stream
    // becomes a flat comparison after hoisting.
    let sandbox = Sandbox::new();
    let hoisted = run_with_stdin(
        sandbox
            .cmd()
            .args(["enrich", "--with", "entity-hoist"])
            .env("STAVE_AUDIT", "off"),
        &fixture("issue"),
    );
    assert!(hoisted.status.success(), "{}", stderr_of(&hoisted));

    let filtered = run_with_stdin(
        sandbox
            .cmd()
            .args([
                "filter",
                "--where",
                r#"has(record.entity_cloud_platform) && entity_cloud_platform == "AWS""#,
            ])
            .env("STAVE_AUDIT", "off"),
        &stdout_of(&hoisted),
    );
    assert!(filtered.status.success(), "{}", stderr_of(&filtered));
    let ids = common::ids(&stdout_of(&filtered));
    assert_eq!(ids, ["issue_01", "issue_02"], "{ids:?}");
}

// ---------------------------------------------------------------------------
// recipe selection
// ---------------------------------------------------------------------------

#[test]
fn rejects_an_unknown_recipe_and_lists_the_real_ones() {
    let out = enrich("issue", &["--with", "totally-fake"]);
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("unknown recipe"), "{err}");
    for recipe in ["account-context", "severity-roll-up", "entity-hoist"] {
        assert!(
            err.contains(recipe),
            "suggestion list is missing {recipe}: {err}"
        );
    }
}

#[test]
fn enrich_passes_an_empty_stream_through() {
    let sandbox = Sandbox::new();
    let out = run_with_stdin(
        sandbox
            .cmd()
            .args(["enrich", "--with", "severity-roll-up"])
            .env("STAVE_AUDIT", "off"),
        "",
    );
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).is_empty());
}
