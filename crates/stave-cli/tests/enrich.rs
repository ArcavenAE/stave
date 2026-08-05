//! End-to-end tests for `stave enrich`.
//!
//! Covers all three v0.1 recipes (blueprint-context, severity-roll-up,
//! device-platform) against the iru fixtures so the cross-kind-join
//! semantic is enforced via the Rust binary.

use std::io::Write;
use std::process::{Command, Stdio};

use assert_cmd::cargo::CommandCargoExt;

fn fixture(name: &str) -> String {
    let path = format!("../../examples/fixtures/{name}.jsonl");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {path}: {e}"))
}

fn fixture_path(name: &str) -> String {
    format!("../../examples/fixtures/{name}.jsonl")
}

fn run_enrich(input: &str, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::cargo_bin("stave").expect("stave binary");
    cmd.arg("enrich").args(args);
    cmd.env_remove("STAVE_API_TOKEN");
    cmd.env("STAVE_AUDIT", "off");
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

fn parse_jsonl(s: &str) -> Vec<serde_json::Value> {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("valid json"))
        .collect()
}

#[test]
fn blueprint_context_attaches_parent_to_each_device() {
    let out = run_enrich(
        &fixture("device"),
        &[
            "--with",
            "blueprint-context",
            "--blueprints",
            &fixture_path("blueprint"),
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let records = parse_jsonl(&String::from_utf8(out.stdout).unwrap());

    let by_name: std::collections::HashMap<String, &serde_json::Value> = records
        .iter()
        .map(|r| (r["device_name"].as_str().unwrap().to_string(), r))
        .collect();

    assert_eq!(
        by_name["kestrel"]["blueprint"]["id"].as_str(),
        Some("bp_001"),
        "kestrel → bp_001 (Mac Fleet)"
    );
    assert_eq!(
        by_name["kestrel"]["blueprint"]["name"].as_str(),
        Some("Mac Fleet")
    );
    assert_eq!(
        by_name["osprey"]["blueprint"]["id"].as_str(),
        Some("bp_002"),
        "osprey → bp_002 (Kiosk iPads)"
    );
    assert_eq!(
        by_name["rocinante"]["blueprint"]["id"].as_str(),
        Some("bp_001")
    );
    assert!(
        by_name["skiff-ipad"]["blueprint"].is_null(),
        "skiff-ipad (blueprint bp_999 absent) → null"
    );
}

#[test]
fn blueprint_context_passes_through_non_devices() {
    let out = run_enrich(
        &fixture("vulnerability"),
        &[
            "--with",
            "blueprint-context",
            "--blueprints",
            &fixture_path("blueprint"),
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let records = parse_jsonl(&String::from_utf8(out.stdout).unwrap());
    assert!(!records.is_empty());
    for r in &records {
        assert_eq!(r["_kind"].as_str(), Some("vulnerability"));
        assert!(
            r.get("blueprint").is_none(),
            "vulnerability records should not get a blueprint attached: {}",
            r["cve_id"]
        );
    }
}

#[test]
fn blueprint_context_requires_blueprints_flag() {
    let out = run_enrich(&fixture("device"), &["--with", "blueprint-context"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("requires --blueprints"), "stderr: {stderr}");
}

#[test]
fn severity_rollup_copies_own_severity_for_vulnerabilities() {
    let out = run_enrich(&fixture("vulnerability"), &["--with", "severity-roll-up"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let records = parse_jsonl(&String::from_utf8(out.stdout).unwrap());
    assert!(!records.is_empty());
    for r in &records {
        let sev = r["severity"].as_str();
        let rollup = r["severity_rollup"].as_str();
        assert_eq!(sev, rollup, "rollup must equal own severity: {r}");
    }
}

#[test]
fn severity_rollup_yields_null_for_kinds_without_severity() {
    let out = run_enrich(&fixture("device"), &["--with", "severity-roll-up"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let records = parse_jsonl(&String::from_utf8(out.stdout).unwrap());
    for r in &records {
        assert!(
            r["severity_rollup"].is_null(),
            "devices carry no severity — rollup must be null: {r}"
        );
    }
}

#[test]
fn device_platform_hoists_top_level_field_for_devices() {
    let out = run_enrich(&fixture("device"), &["--with", "device-platform"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let records = parse_jsonl(&String::from_utf8(out.stdout).unwrap());
    for r in &records {
        let nested = r["platform"].as_str();
        let hoisted = r["_platform"].as_str();
        assert_eq!(nested, hoisted, "hoist must mirror platform");
        assert!(nested.is_some(), "device fixtures all have platform");
    }
}

#[test]
fn device_platform_passes_through_records_without_platform() {
    // vulnerability fixtures don't have a platform field.
    let out = run_enrich(&fixture("vulnerability"), &["--with", "device-platform"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let records = parse_jsonl(&String::from_utf8(out.stdout).unwrap());
    for r in &records {
        assert!(
            r.get("_platform").is_none(),
            "vulnerability should pass through"
        );
    }
}

#[test]
fn rejects_unknown_recipe() {
    let out = run_enrich(&fixture("device"), &["--with", "totally-fake"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown recipe"), "stderr: {stderr}");
}

#[test]
fn rejects_blueprints_file_with_wrong_kind() {
    // vulnerability.jsonl is not blueprint records.
    let out = run_enrich(
        &fixture("device"),
        &[
            "--with",
            "blueprint-context",
            "--blueprints",
            &fixture_path("vulnerability"),
        ],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("kinds other than `blueprint`"),
        "stderr: {stderr}"
    );
}

#[test]
fn enrich_then_filter_then_emit_pipeline() {
    // Fleet-triage-shaped pipeline: attach blueprint context to
    // devices, then keep only devices in the Mac Fleet blueprint.
    // Validates that enriched fields are visible to the filter
    // primitive.
    let mut enrich = Command::cargo_bin("stave").expect("stave");
    enrich.args([
        "enrich",
        "--with",
        "blueprint-context",
        "--blueprints",
        &fixture_path("blueprint"),
    ]);
    enrich.env("STAVE_AUDIT", "off");
    enrich.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut enrich_child = enrich.spawn().expect("spawn enrich");
    enrich_child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(fixture("device").as_bytes())
        .unwrap();
    drop(enrich_child.stdin.take());
    let enrich_out = enrich_child.wait_with_output().expect("wait enrich");
    assert!(
        enrich_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&enrich_out.stderr)
    );

    let mut filter = Command::cargo_bin("stave").expect("stave");
    filter.args([
        "filter",
        "--where",
        r#"has(record.blueprint) && record.blueprint != null && blueprint.name == "Mac Fleet""#,
    ]);
    filter.env("STAVE_AUDIT", "off");
    filter.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut filter_child = filter.spawn().expect("spawn filter");
    filter_child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&enrich_out.stdout)
        .unwrap();
    drop(filter_child.stdin.take());
    let filter_out = filter_child.wait_with_output().expect("wait filter");
    assert!(
        filter_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&filter_out.stderr)
    );

    let kept = parse_jsonl(&String::from_utf8(filter_out.stdout).unwrap());
    let names: Vec<&str> = kept
        .iter()
        .map(|r| r["device_name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"kestrel"), "names: {names:?}");
    assert!(names.contains(&"rocinante"), "names: {names:?}");
    assert!(!names.contains(&"osprey"), "names: {names:?}");
    assert!(!names.contains(&"skiff-ipad"), "names: {names:?}");
}
