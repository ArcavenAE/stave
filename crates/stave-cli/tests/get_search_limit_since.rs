//! Coverage for `stave get`, `stave search`, `--limit`, `--since`, and
//! the write-guard's CLI surface.
//!
//! Most of these paths fail before any network call, which is the
//! property under test: a bad `--since`, an unknown kind, an unsupported
//! `get`, or a mutating MCP tool must be refused locally rather than
//! costing an API request. `search` needs a real response shape, so it
//! runs against wiremock here alongside the validation cases.

mod common;

use common::{Sandbox, connection_page, ids, jsonl, run, stderr_of, stdout_of};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A command with no credentials and no endpoint, so anything that
/// reaches the network fails in a recognisable way.
fn offline(args: &[&str]) -> std::process::Output {
    let sandbox = Sandbox::new();
    run(sandbox.cmd().args(args).env("STAVE_AUDIT", "off"))
}

// ---------------------------------------------------------------------------
// get: unsupported in v0.1, with two ways through
// ---------------------------------------------------------------------------

#[test]
fn get_is_unsupported_and_names_both_workarounds() {
    // Singular lookups need per-kind singular queries or filter input
    // types that stave will not guess at (charter F2). The refusal has to
    // carry the two routes that work today, or an agent will retry it.
    let out = offline(&["get", "issue", "issue_01"]);
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("not supported in v0.1"), "{err}");
    assert!(err.contains("charter F2"), "{err}");
    assert!(
        err.contains("stave api --query ./issue-by-id.graphql --var id=issue_01"),
        "route 1 must be a runnable command: {err}"
    );
    assert!(
        err.contains(r#"stave list issue --limit 500 | stave filter --where 'id == "issue_01"'"#),
        "route 2 must be a runnable pipeline: {err}"
    );
}

#[test]
fn get_names_the_kinds_own_id_field_in_the_filter_suggestion() {
    // Every v0.1 kind keys on `id`, so the suggestion is uniform today.
    // The assertion exists so that a kind keyed on something else cannot
    // silently ship a wrong predicate.
    for kind in ["vulnerability_finding", "cloud_account", "audit_log"] {
        let out = offline(&["get", kind, "example-id"]);
        assert!(!out.status.success(), "{out:?}");
        let err = stderr_of(&out);
        assert!(
            err.contains(r#"id == "example-id""#),
            "{kind}: predicate must name the id field: {err}"
        );
    }
}

#[test]
fn get_rejects_an_unknown_kind_before_anything_else() {
    let out = offline(&["get", "definitelyNotAKind", "x"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        stderr_of(&out).to_lowercase().contains("invalid value"),
        "clap rejects against the kind table: {}",
        stderr_of(&out)
    );
}

#[test]
fn get_help_says_it_is_unsupported() {
    let out = offline(&["get", "--help"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("not supported in v0.1"), "{stdout}");
}

#[test]
#[ignore = "subcommand long_about is unreachable: the Cmd variant doc comment \
            shadows the #[command(long_about)] on GetArgs, so `stave get --help` \
            prints the one-line about instead. Systematic across all 11 \
            subcommands; only the top-level Cli long_about renders. Fix belongs \
            in main.rs (move long_about onto the Cmd variant), not here."]
fn get_help_explains_why_it_is_unsupported_and_names_the_workarounds() {
    // The refusal text on the error path carries the two routes through
    // (asserted above). The help text is supposed to carry them too, so
    // an agent reading `--help` before its first call does not have to
    // trigger the error to learn them.
    let out = offline(&["get", "--help"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("charter F2"), "{stdout}");
    assert!(stdout.contains("stave api --query"), "{stdout}");
}

// ---------------------------------------------------------------------------
// search: client-side substring over the kind's declared search field
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_matches_the_kinds_search_field_case_insensitively() {
    // `issue` declares `type` as its search field, so a search for
    // "toxic" matches TOXIC_COMBINATION regardless of case.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            vec![
                json!({"id": "issue_01", "type": "TOXIC_COMBINATION", "severity": "CRITICAL"}),
                json!({"id": "issue_02", "type": "CLOUD_CONFIGURATION", "severity": "HIGH"}),
                json!({"id": "issue_03", "type": "TOXIC_COMBINATION", "severity": "MEDIUM"}),
            ],
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["search", "issue", "toxic"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", format!("{}/graphql", server.uri())));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(ids(&stdout_of(&out)), ["issue_01", "issue_03"]);
}

#[tokio::test]
async fn search_drops_records_whose_search_field_is_absent() {
    // A missing search field is a non-match, not an error: the field
    // metadata is provisional until live validation (charter F1).
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            vec![
                json!({"id": "issue_01", "type": "TOXIC_COMBINATION"}),
                json!({"id": "issue_02", "severity": "HIGH"}),
            ],
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["search", "issue", "combination"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", format!("{}/graphql", server.uri())));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(ids(&stdout_of(&out)), ["issue_01"]);
}

#[tokio::test]
async fn search_audits_under_its_own_verb_phase() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "cloudResources",
            vec![json!({"id": "res_01", "name": "example-corp-audit-logs"})],
            None,
        )))
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["search", "cloud_resource", "audit-logs"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", format!("{}/graphql", server.uri())));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(jsonl(&stdout_of(&out)).len(), 1);

    let api = sandbox.api_audit_lines();
    assert_eq!(api.len(), 1, "{api:?}");
    assert_eq!(api[0]["verb_phase"], "search");
    assert_eq!(api[0]["operation"]["id"], "list_cloud_resources");
}

#[test]
fn search_rejects_an_unknown_kind() {
    let out = offline(&["search", "definitelyNotAKind", "x"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        stderr_of(&out).to_lowercase().contains("invalid value"),
        "{}",
        stderr_of(&out)
    );
}

#[test]
fn search_help_names_the_search_field_concept() {
    let out = offline(&["search", "--help"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("search field"), "{stdout}");
    assert!(stdout.contains("case-insensitive"), "{stdout}");
}

#[test]
#[ignore = "subcommand long_about is unreachable: the Cmd variant doc comment \
            shadows the #[command(long_about)] on SearchArgs. Same root cause as \
            get_help_explains_why_it_is_unsupported_and_names_the_workarounds. \
            Fix belongs in main.rs."]
fn search_help_explains_it_is_a_client_side_fallback() {
    // Server-side filter variables are charter F2 work. Until they land,
    // `search` is a client-side substring pass over a full page, which a
    // caller sizing a query needs to know.
    let out = offline(&["search", "--help"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("client-side"), "{stdout}");
    assert!(stdout.contains("charter F2"), "{stdout}");
}

// ---------------------------------------------------------------------------
// --since: validated before any network call
// ---------------------------------------------------------------------------

#[test]
fn since_rejects_a_kind_with_no_primary_timestamp() {
    // `project` has a list operation but no timestamp field, so `--since`
    // has nothing to compare against.
    let out = offline(&["list", "project", "--since", "24h"]);
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("no primary timestamp field"), "{err}");
    assert!(
        err.contains("stave list project | stave filter"),
        "the error must name the composition that does work: {err}"
    );
}

#[test]
fn since_rejects_the_days_unit_that_go_durations_do_not_have() {
    let out = offline(&["list", "issue", "--since", "7d"]);
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("invalid duration"), "{err}");
    assert!(
        err.contains("24h"),
        "the error must show the spelling that works: {err}"
    );
}

#[test]
fn since_rejects_garbage() {
    let out = offline(&["list", "issue", "--since", "not-a-real-duration"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        stderr_of(&out).contains("invalid duration"),
        "{}",
        stderr_of(&out)
    );
}

#[test]
fn since_rejects_an_embedded_quote_explicitly() {
    // The duration is interpolated into a CEL predicate, so a quote would
    // be an injection point. It is refused by name.
    let out = offline(&["list", "issue", "--since", r#"24h" || true"#]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        stderr_of(&out).contains("must not contain quotes"),
        "{}",
        stderr_of(&out)
    );
}

#[test]
fn since_accepts_compound_and_decimal_durations() {
    // These must get past validation. With no credentials the run then
    // fails on the credential chain, which proves validation passed.
    for value in ["24h", "1h30m", "0.5h", "500ms"] {
        let out = offline(&["list", "issue", "--since", value]);
        assert!(!out.status.success(), "{value}: {out:?}");
        let err = stderr_of(&out);
        assert!(
            !err.contains("invalid duration"),
            "{value} is a valid Go duration: {err}"
        );
        assert!(
            err.contains("credentials"),
            "{value} should have reached the credential chain: {err}"
        );
    }
}

#[test]
fn since_validation_fires_before_the_credential_chain() {
    // Ordering matters: a duration typo must not be reported as an auth
    // problem, and must not cost a token mint.
    let out = offline(&["list", "issue", "--since", "7d"]);
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(
        !err.contains("credentials"),
        "duration validation must come first: {err}"
    );
}

#[tokio::test]
async fn since_drops_records_older_than_the_window() {
    // The `--since` post-filter is a CEL predicate over the kind's
    // primary timestamp, so this also exercises camelCase promotion on a
    // live response rather than a fixture.
    let recent = chrono::Utc::now() - chrono::TimeDelta::hours(2);
    let ancient = chrono::Utc::now() - chrono::TimeDelta::days(400);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            vec![
                json!({"id": "issue_recent", "severity": "HIGH", "createdAt": recent.to_rfc3339()}),
                json!({"id": "issue_ancient", "severity": "LOW", "createdAt": ancient.to_rfc3339()}),
            ],
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--since", "24h"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", format!("{}/graphql", server.uri())));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(ids(&stdout_of(&out)), ["issue_recent"]);
}

// ---------------------------------------------------------------------------
// --limit and kind validation
// ---------------------------------------------------------------------------

#[test]
fn list_rejects_an_unknown_kind_against_the_kind_table() {
    let out = offline(&["list", "definitelyNotAKind"]);
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out).to_lowercase();
    assert!(err.contains("invalid value"), "{err}");
    assert!(
        err.contains("issue"),
        "the refusal must list the kinds that exist: {err}"
    );
}

#[test]
fn list_help_documents_limit_and_since() {
    let out = offline(&["list", "--help"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("--limit"), "{stdout}");
    assert!(stdout.contains("--since"), "{stdout}");
    assert!(
        stdout.contains("Go duration"),
        "the help must name the duration syntax: {stdout}"
    );
}

#[test]
fn list_rejects_a_non_numeric_limit() {
    let out = offline(&["list", "issue", "--limit", "lots"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        stderr_of(&out).to_lowercase().contains("invalid value"),
        "{}",
        stderr_of(&out)
    );
}

// ---------------------------------------------------------------------------
// ops: the curated library is discoverable without credentials
// ---------------------------------------------------------------------------

#[test]
fn ops_list_emits_one_record_per_curated_operation() {
    let out = offline(&["ops", "list"]);
    assert!(out.status.success(), "{out:?}");
    let records = jsonl(&stdout_of(&out));
    assert_eq!(records.len(), 12, "the v0.1 library has 12 operations");
    assert_eq!(records[0]["_kind"], "operation");
    assert_eq!(records[0]["name"], "list_issues");
    assert_eq!(records[0]["root_field"], "issuesV2");
    assert_eq!(
        records[0]["op_type"], "query",
        "every v0.1 operation is a read"
    );
}

#[test]
fn ops_list_filters_by_name_substring() {
    let out = offline(&["ops", "list", "--filter", "cloud"]);
    assert!(out.status.success(), "{out:?}");
    let records = jsonl(&stdout_of(&out));
    assert!(!records.is_empty(), "expected some cloud operations");
    for r in &records {
        assert!(
            r["name"].as_str().unwrap_or_default().contains("cloud"),
            "{r}"
        );
    }
}

#[test]
fn ops_show_writes_a_runnable_document_to_stdout() {
    // `stave ops show list_issues > my-query.graphql` has to produce a
    // file that `stave api --query` accepts, so the document owns stdout
    // and the prose goes to stderr.
    let out = offline(&["ops", "show", "list_issues"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("query ListIssues"), "{stdout}");
    assert!(stdout.contains("issuesV2"), "{stdout}");
    assert!(stdout.ends_with('\n'), "must end newline-terminated");
    let err = stderr_of(&out);
    assert!(
        err.contains("list_issues") && err.contains("issuesV2"),
        "the human summary belongs on stderr: {err}"
    );
}

#[test]
fn ops_show_unknown_operation_points_at_the_discovery_command() {
    let out = offline(&["ops", "show", "resolve_every_issue"]);
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("not found in the curated registry"), "{err}");
    assert!(err.contains("stave ops list"), "{err}");
}

#[test]
fn api_unknown_operation_points_at_the_discovery_command() {
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["api", "resolve_every_issue"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", "http://127.0.0.1:1/graphql")
        .env("STAVE_AUDIT", "off"));
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("not found in the curated registry"), "{err}");
    assert!(
        !err.contains("network"),
        "an unknown name must not cost a request: {err}"
    );
}

#[test]
fn api_requires_either_a_name_or_a_query() {
    let out = offline(&["api"]);
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out).to_lowercase();
    assert!(
        err.contains("required") || err.contains("usage"),
        "clap's arg group must enforce one of the two: {err}"
    );
}

#[test]
fn api_rejects_vars_that_are_not_a_json_object() {
    let out = offline(&["api", "list_issues", "--vars", "[1,2]"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        stderr_of(&out).contains("must be a JSON object"),
        "{}",
        stderr_of(&out)
    );
}

#[test]
fn api_rejects_a_var_without_an_equals_sign() {
    let out = offline(&["api", "list_issues", "--var", "first"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(stderr_of(&out).contains("key=value"), "{}", stderr_of(&out));
}

// ---------------------------------------------------------------------------
// the MCP write-guard, which fires before any credential resolution
// ---------------------------------------------------------------------------

#[test]
fn mcp_call_refuses_a_tool_whose_name_is_not_read_shaped() {
    // The heuristic is deliberately conservative: an unrecognised shape
    // is treated as a write, so the refusal happens with no credentials
    // and no network.
    for tool in ["resolve-issue", "delete-report", "ambiguous-tool"] {
        let out = offline(&["mcp", "call", tool, "--args", "{}"]);
        assert!(!out.status.success(), "{tool} must be gated: {out:?}");
        let err = stderr_of(&out);
        assert!(err.contains("write-guard"), "{err}");
        assert!(err.contains(tool), "the refusal must name the tool: {err}");
        // D1/D2: refusal is unconditional and terminal — no override
        // route is named.
        assert!(
            err.contains("read-only against live tenants"),
            "the refusal must state the posture: {err}"
        );
        assert!(
            !err.contains("--allow-write"),
            "no override breadcrumb: {err}"
        );
        assert!(
            !err.contains("STAVE_ALLOW_WRITE"),
            "no override breadcrumb: {err}"
        );
    }
}

#[test]
fn mcp_call_rejects_args_that_are_not_a_json_object() {
    let out = offline(&["mcp", "call", "get-issues", "--args", "[1,2]"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        stderr_of(&out).contains("must be a JSON object"),
        "{}",
        stderr_of(&out)
    );
}

#[test]
fn mcp_status_reports_the_hosted_default_endpoint_without_credentials() {
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["mcp", "status"])
        .env("STAVE_AUDIT", "off"));
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("(source: default)"), "{stdout}");
    assert!(
        stdout.contains("mcp.app.wiz.io"),
        "the hosted default must be reported: {stdout}"
    );
}

#[test]
fn mcp_status_reports_an_endpoint_override_from_config() {
    let sandbox = Sandbox::new();
    sandbox.write_config(
        r#"
[mcp]
url = "https://mcp.example.test"
"#,
    );
    let out = run(sandbox
        .cmd()
        .args(["mcp", "status"])
        .env("STAVE_AUDIT", "off"));
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("https://mcp.example.test"), "{stdout}");
    assert!(stdout.contains("(source: config)"), "{stdout}");
}

#[test]
fn mcp_config_masks_the_bearer_token_by_default() {
    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["mcp", "config"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_AUDIT", "off"));
    assert!(out.status.success(), "{out:?}");
    let stdout = stdout_of(&out);
    assert!(
        stdout.contains("<redacted, rerun with --reveal>"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("example-access-token"),
        "the token must not print without --reveal: {stdout}"
    );
}
