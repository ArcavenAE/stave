//! End-to-end coverage through the binary against a wiremock server.
//!
//! Closes four loops:
//!
//!   * `STAVE_BASE_URL` reaches the network layer, and a GraphQL request
//!     is posted as `{"query", "variables"}` with a bearer header.
//!   * `list` pages a connection: the second request carries the first
//!     response's `endCursor` as `variables.after`.
//!   * the write-guard refuses a mutating document before anything is
//!     sent (asserted with `expect(0)`, which wiremock verifies on drop);
//!     mutations refuse unconditionally, with no override (D1).
//!
//! SAFETY: every request here targets a LOCAL wiremock server on
//! 127.0.0.1, never a real Wiz tenant. No test performs, or could
//! perform, any active/change/remediation/destructive action against
//! any real system. See
//! docs/design/read-only-permissions-implementation-plan.md.
//!   * the OAuth mint runs against the mocked token endpoint with
//!     `grant_type=client_credentials` and `audience=wiz-api`, caches the
//!     result, and the audit line records where the credential and the
//!     endpoint each came from.
//!
//! Pattern: `#[tokio::test]` starts a `MockServer`, mounts `Mock`s with
//! explicit method, path, header, and body expectations, then runs the
//! CLI synchronously through the sandbox harness. Every response body is
//! synthetic.

mod common;

use common::{
    Sandbox, connection_page, jsonl, request_variables, run, run_with_stdin, stderr_of, stdout_of,
};
use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Two synthetic issue nodes, shaped like `list_issues` selects them.
fn issue_nodes() -> Vec<Value> {
    vec![
        json!({
            "id": "issue_01",
            "type": "TOXIC_COMBINATION",
            "severity": "CRITICAL",
            "status": "OPEN",
            "createdAt": "2026-07-28T09:15:00Z",
            "entitySnapshot": {
                "id": "ent_01",
                "type": "BUCKET",
                "name": "example-corp-audit-logs",
                "cloudPlatform": "AWS",
                "subscriptionExternalId": "123456789012",
            },
        }),
        json!({
            "id": "issue_02",
            "type": "CLOUD_CONFIGURATION",
            "severity": "HIGH",
            "status": "IN_PROGRESS",
            "createdAt": "2026-07-30T14:40:00Z",
            "entitySnapshot": Value::Null,
        }),
    ]
}

fn third_issue_node() -> Value {
    json!({
        "id": "issue_03",
        "type": "CLOUD_CONFIGURATION",
        "severity": "MEDIUM",
        "status": "OPEN",
        "createdAt": "2026-08-02T06:20:00Z",
        "entitySnapshot": Value::Null,
    })
}

/// A GraphQL endpoint on the mock server. Using a path rather than the
/// bare root keeps the matchers legible and mirrors the real tenant
/// endpoint, which ends in `/graphql`.
fn graphql_url(server: &MockServer) -> String {
    format!("{}/graphql", server.uri())
}

// ---------------------------------------------------------------------------
// the request stave actually posts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn api_posts_a_graphql_document_and_prints_the_data_block() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "Bearer example-access-token"))
        .and(body_string_contains("issuesV2"))
        .and(body_string_contains(r#""first":2"#))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["api", "list_issues", "--var", "first=2"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));

    assert!(out.status.success(), "api call failed: {}", stderr_of(&out));
    let data: Value = serde_json::from_str(&stdout_of(&out)).expect("stdout is one JSON object");
    let nodes = data["issuesV2"]["nodes"]
        .as_array()
        .expect("data carries the connection");
    assert_eq!(nodes.len(), 2, "{data}");
    assert_eq!(nodes[0]["id"], "issue_01");
}

#[tokio::test]
async fn var_values_that_parse_as_json_are_sent_as_json_scalars() {
    // `--var first=2` must send the number 2, not the string "2", or the
    // GraphQL variable coercion fails on the server side.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            vec![],
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args([
            "api",
            "list_issues",
            "--var",
            "first=2",
            "--var",
            "status=OPEN",
        ])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(out.status.success(), "{}", stderr_of(&out));

    let requests = server.received_requests().await.expect("recording enabled");
    let vars = request_variables(&requests[0].body);
    assert_eq!(vars["first"], json!(2), "numbers stay numbers: {vars}");
    assert_eq!(
        vars["status"],
        json!("OPEN"),
        "unparseable values become strings: {vars}"
    );
}

// ---------------------------------------------------------------------------
// pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_pages_a_connection_and_sends_the_cursor_on_the_second_call() {
    let server = MockServer::start().await;

    // First page: asked for 3, hands back 2 and says there is more.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains(r#""first":3"#))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            Some("cursor-page-2"),
        )))
        .expect(1)
        .mount(&server)
        .await;

    // Second page: the remaining 1, and the connection ends.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains(r#""after":"cursor-page-2""#))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            vec![third_issue_node()],
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "3"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(out.status.success(), "{}", stderr_of(&out));

    let records = jsonl(&stdout_of(&out));
    assert_eq!(records.len(), 3, "both pages must stream: {records:?}");
    assert_eq!(records[0]["_kind"], "issue");
    assert_eq!(records[2]["id"], "issue_03");
    // `_source.response_index` counts across pages, not within one.
    assert_eq!(records[2]["_source"]["response_index"], 2);
    assert_eq!(records[2]["_source"]["operation_id"], "list_issues");

    let requests = server.received_requests().await.expect("recording enabled");
    assert_eq!(requests.len(), 2, "one request per page");
    let first = request_variables(&requests[0].body);
    assert!(
        first.get("after").is_none(),
        "the first call has no cursor: {first}"
    );
    let second = request_variables(&requests[1].body);
    assert_eq!(second["after"], json!("cursor-page-2"));
    assert_eq!(
        second["first"],
        json!(1),
        "the second page asks only for the shortfall: {second}"
    );
}

#[tokio::test]
async fn list_stops_at_the_limit_without_asking_for_another_page() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            Some("cursor-page-2"),
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "1"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(jsonl(&stdout_of(&out)).len(), 1);
    // expect(1), verified on drop: the cursor was available but unused.
}

#[tokio::test]
async fn list_stops_on_a_connection_that_never_advances_its_cursor() {
    // This test used to assert that an empty page ends the connection,
    // which conflated two different things and cost real records: a
    // zero-node page with `hasNextPage: true` is legitimate and must be
    // followed (see `tests/paging.rs`). What actually cannot terminate
    // is a cursor that never moves, and that is what is asserted here.
    //
    // The server repeats one cursor. stave follows it once, sees the
    // same value come back, and stops: two requests, no records, and a
    // warning that the read is incomplete rather than a silent exit 0.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            vec![],
            Some("cursor-that-never-ends"),
        )))
        .expect(2)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "50"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).trim().is_empty(), "no records to emit");
    assert!(
        stderr_of(&out).contains("same cursor twice"),
        "the caller must be told the read stopped short: {}",
        stderr_of(&out)
    );
}

// ---------------------------------------------------------------------------
// error surfaces
// ---------------------------------------------------------------------------

#[tokio::test]
async fn graphql_errors_array_fails_the_call_with_the_servers_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": Value::Null,
            "errors": [
                {"message": "Cannot query field 'dueAt' on type 'Issue'"},
                {"message": "Variable '$first' is never used"},
            ],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["api", "list_issues"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));

    assert!(
        !out.status.success(),
        "a GraphQL errors array is a failed call: {out:?}"
    );
    let err = stderr_of(&out);
    assert!(err.contains("GraphQL:"), "{err}");
    assert!(err.contains("Cannot query field 'dueAt'"), "{err}");
    assert!(
        err.contains("Variable '$first' is never used"),
        "every message must survive: {err}"
    );
}

#[tokio::test]
async fn graphql_error_is_recorded_as_its_own_audit_outcome() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": Value::Null,
            "errors": [{"message": "Field 'issuesV2' is deprecated"}],
        })))
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["api", "list_issues"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(!out.status.success());

    let api = sandbox.api_audit_lines();
    assert_eq!(api.len(), 1, "a failed call still audits: {api:?}");
    assert_eq!(api[0]["result"], "graphql_error");
    assert_eq!(
        api[0]["response"]["status"], 200,
        "GraphQL reports failure inside a 200"
    );
    assert_eq!(api[0]["response"]["items_returned"], 1, "one error message");
}

#[tokio::test]
async fn a_non_success_status_surfaces_as_an_http_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["api", "list_issues"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));

    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("HTTP 401"), "{err}");

    let api = sandbox.api_audit_lines();
    assert_eq!(api.len(), 1);
    assert_eq!(api[0]["result"], "http_error");
    assert_eq!(api[0]["response"]["status"], 401);
}

// ---------------------------------------------------------------------------
// write-guard on ad-hoc documents
// ---------------------------------------------------------------------------

const MUTATION_DOCUMENT: &str = r#"mutation ResolveOneIssue($id: ID!) {
  updateIssue(input: {id: $id, patch: {status: RESOLVED}}) {
    issue { id status }
  }
}"#;

#[tokio::test]
async fn adhoc_mutation_is_refused_before_any_request_is_sent() {
    // Exploratory posture is set so the document reaches the write-guard
    // classifier (curated posture would refuse the ad-hoc document one
    // step earlier). A mutation must STILL refuse. expect(0): the guard
    // refuses locally, so wiremock — a local mock, never the tenant —
    // sees nothing.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {}})))
        .expect(0)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    sandbox.write_exploratory_config();
    let out = run_with_stdin(
        sandbox
            .cmd()
            .args(["api", "--query", "-", "--var", "id=issue_01"])
            .env("STAVE_ACCESS_TOKEN", "example-access-token")
            .env("STAVE_BASE_URL", graphql_url(&server)),
        MUTATION_DOCUMENT,
    );

    assert!(!out.status.success(), "the guard must refuse: {out:?}");
    let err = stderr_of(&out);
    // D2: terminal, byte-stable wall. It names the posture and stops.
    assert!(err.contains("write-guard"), "{err}");
    assert!(
        err.contains("read-only against live tenants"),
        "the refusal must state the posture: {err}"
    );
    // D2: the refusal must NOT name any override route — there is none.
    assert!(
        !err.contains("--allow-write"),
        "no override breadcrumb: {err}"
    );
    assert!(
        !err.contains("STAVE_ALLOW_WRITE"),
        "no override breadcrumb: {err}"
    );
    assert!(
        !err.contains("allow_writes"),
        "no override breadcrumb: {err}"
    );

    // D6: the refusal emits a first-class audit line (schema v3), never
    // a response block, carrying the refusal detail for correlation.
    let refused: Vec<_> = sandbox
        .audit_lines()
        .into_iter()
        .filter(|l| l["result"] == "refused")
        .collect();
    assert_eq!(refused.len(), 1, "one refused line: {refused:?}");
    assert_eq!(refused[0]["schema_version"], 3);
    assert_eq!(refused[0]["refusal"]["op_type"], "mutation");
    assert_eq!(refused[0]["refusal"]["operation"], "ResolveOneIssue");
    assert!(
        refused[0]["response"].is_null(),
        "a refused call never has a response block: {:?}",
        refused[0]
    );
}

#[tokio::test]
async fn a_refused_line_records_the_session_id_when_the_env_supplies_one() {
    // D6: STAVE_SESSION_ID threads into the refused audit line so a
    // per-session refusal detector can group reformulated attempts.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {}})))
        .expect(0)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    sandbox.write_exploratory_config();
    let out = run_with_stdin(
        sandbox
            .cmd()
            .args(["api", "--query", "-", "--var", "id=issue_01"])
            .env("STAVE_ACCESS_TOKEN", "example-access-token")
            .env("STAVE_BASE_URL", graphql_url(&server))
            .env("STAVE_SESSION_ID", "agent-session-77"),
        MUTATION_DOCUMENT,
    );
    assert!(!out.status.success());
    let refused: Vec<_> = sandbox
        .audit_lines()
        .into_iter()
        .filter(|l| l["result"] == "refused")
        .collect();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0]["invocation"]["session_id"], "agent-session-77");
}

#[tokio::test]
async fn adhoc_read_is_refused_under_the_curated_posture() {
    // D11: the default curated posture refuses ad-hoc --query documents
    // (even reads) before anything reaches the wire.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {}})))
        .expect(0)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run_with_stdin(
        sandbox
            .cmd()
            .args(["api", "--query", "-"])
            .env("STAVE_ACCESS_TOKEN", "example-access-token")
            .env("STAVE_BASE_URL", graphql_url(&server)),
        "query ReadIssues { issuesV2 { nodes { id } } }",
    );
    assert!(
        !out.status.success(),
        "curated posture must refuse: {out:?}"
    );
    let err = stderr_of(&out);
    assert!(err.contains("curated"), "{err}");
    assert!(
        err.contains("stave config set posture exploratory"),
        "{err}"
    );
}

#[tokio::test]
async fn adhoc_read_is_allowed_under_the_exploratory_posture() {
    // D11: under exploratory posture, an ad-hoc READ document runs
    // (against a LOCAL mock — never the tenant), and the audit line
    // records the posture and the document hash.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"issuesV2": {"nodes": []}}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    sandbox.write_exploratory_config();
    let out = run_with_stdin(
        sandbox
            .cmd()
            .args(["api", "--query", "-"])
            .env("STAVE_ACCESS_TOKEN", "example-access-token")
            .env("STAVE_BASE_URL", graphql_url(&server)),
        "query ReadIssues { issuesV2 { nodes { id } } }",
    );
    assert!(out.status.success(), "{}", stderr_of(&out));

    let api = sandbox.api_audit_lines();
    assert_eq!(api.len(), 1, "an ad-hoc read audits: {api:?}");
    assert_eq!(api[0]["posture"], "exploratory");
    assert!(
        api[0]["document_sha256"]
            .as_str()
            .is_some_and(|s| s.starts_with("sha256:")),
        "the ad-hoc document hash must be recorded: {api:?}"
    );
}

#[tokio::test]
async fn a_query_hiding_a_mutation_in_the_same_document_is_still_refused() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {}})))
        .expect(0)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    sandbox.write_exploratory_config();
    let document = "query ReadIssues { issuesV2 { nodes { id } } }\n\
                    mutation Sneaky { deleteReport(id: \"rep_01\") { id } }";
    let out = run_with_stdin(
        sandbox
            .cmd()
            .args(["api", "--query", "-"])
            .env("STAVE_ACCESS_TOKEN", "example-access-token")
            .env("STAVE_BASE_URL", graphql_url(&server)),
        document,
    );
    assert!(!out.status.success(), "{out:?}");
    assert!(
        stderr_of(&out).contains("write-guard"),
        "{}",
        stderr_of(&out)
    );
}

#[tokio::test]
async fn an_unparseable_document_never_reaches_the_wire() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {}})))
        .expect(0)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    sandbox.write_exploratory_config();
    let out = run_with_stdin(
        sandbox
            .cmd()
            .args(["api", "--query", "-"])
            .env("STAVE_ACCESS_TOKEN", "example-access-token")
            .env("STAVE_BASE_URL", graphql_url(&server)),
        "query { unbalanced",
    );
    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("GraphQL document"), "{err}");
    assert!(err.contains("parse error"), "{err}");
}

// ---------------------------------------------------------------------------
// audit provenance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn audit_line_records_the_v2_shape_for_a_curated_query() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            None,
        )))
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "2"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(out.status.success(), "{}", stderr_of(&out));

    let api = sandbox.api_audit_lines();
    assert_eq!(api.len(), 1, "{api:?}");
    let line = &api[0];
    assert_eq!(line["schema_version"], 3);
    assert_eq!(line["verb_phase"], "list");
    assert_eq!(line["synthesis_keys"][0], "id");
    assert_eq!(line["operation"]["id"], "list_issues");
    assert_eq!(
        line["operation"]["method"], "query",
        "GraphQL operation type stands in for the HTTP method"
    );
    assert_eq!(
        line["operation"]["url_template"], "issuesV2",
        "the connection root field is the closest thing to a route"
    );
    assert_eq!(line["operation"]["path_params"]["first"], 2);
    assert_eq!(line["result"], "ok");
    assert_eq!(line["response"]["status"], 200);
    assert_eq!(line["response"]["items_returned"], 2);
    assert!(
        line["response"]["shape_hash"]
            .as_str()
            .is_some_and(|h| h.starts_with("sha256:")),
        "{line}"
    );
    assert_eq!(
        line["redacted_fields"][0], "authorization",
        "the bearer header is never recorded"
    );
    assert!(
        !line.to_string().contains("example-access-token"),
        "the token must not appear in the trail: {line}"
    );
}

#[tokio::test]
async fn audit_records_the_endpoint_source_when_the_endpoint_came_from_config() {
    // `STAVE_BASE_URL` is a test and dev override, so it deliberately
    // records no source. The config layer of the chain does, and that is
    // the mining signal the audit format promises.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            None,
        )))
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    sandbox.write_config(&format!(
        "[default]\napi_url = \"{}\"\n",
        graphql_url(&server)
    ));
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "2"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token"));
    assert!(out.status.success(), "{}", stderr_of(&out));

    let api = sandbox.api_audit_lines();
    assert_eq!(api.len(), 1, "{api:?}");
    assert_eq!(api[0]["invocation"]["api_url_source"], "config");
    assert_eq!(
        api[0]["path_params_source"]["_api_url"], "config",
        "the endpoint's provenance rides alongside the other params"
    );
}

#[tokio::test]
async fn audit_records_the_endpoint_source_as_flag_for_a_per_call_override() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            None,
        )))
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args([
            "list",
            "issue",
            "--limit",
            "2",
            "--api-url",
            &graphql_url(&server),
        ])
        .env("STAVE_ACCESS_TOKEN", "example-access-token"));
    assert!(out.status.success(), "{}", stderr_of(&out));

    let api = sandbox.api_audit_lines();
    assert_eq!(api.len(), 1, "{api:?}");
    assert_eq!(
        api[0]["invocation"]["api_url_source"], "flag",
        "per-call intent is the signal that distinguishes an override"
    );
}

#[tokio::test]
async fn base_url_override_records_no_endpoint_source() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            None,
        )))
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "2"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(out.status.success(), "{}", stderr_of(&out));

    let api = sandbox.api_audit_lines();
    assert_eq!(api.len(), 1, "{api:?}");
    assert!(
        api[0]["invocation"]["api_url_source"].is_null(),
        "the test override is not a chain layer: {:?}",
        api[0]
    );
    assert!(
        api[0].get("path_params_source").is_none(),
        "no chain-resolved params means no key at all: {:?}",
        api[0]
    );
}

#[tokio::test]
async fn every_page_of_one_call_shares_a_trace_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains(r#""first":3"#))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            Some("cursor-page-2"),
        )))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("cursor-page-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            vec![third_issue_node()],
            None,
        )))
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "3"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(out.status.success(), "{}", stderr_of(&out));

    let api = sandbox.api_audit_lines();
    assert_eq!(api.len(), 2, "one line per page: {api:?}");
    assert_eq!(
        api[0]["trace_id"], api[1]["trace_id"],
        "a miner must see the paged read as one logical operation"
    );
    assert_ne!(
        api[0]["span_id"], api[1]["span_id"],
        "each page is still its own span"
    );
}

#[tokio::test]
async fn no_audit_records_a_stub_line_without_operation_or_response_detail() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "2", "--no-audit"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(out.status.success(), "{}", stderr_of(&out));

    let lines = sandbox.audit_lines();
    assert_eq!(
        lines.len(),
        1,
        "--no-audit still leaves a record that the call happened: {lines:?}"
    );
    let line = &lines[0];
    assert_eq!(line["result"], "redacted_block");
    assert!(
        line.get("operation").is_none(),
        "operation detail is withheld: {line}"
    );
    assert_eq!(line["response"]["status"], 200);
    assert_eq!(line["redacted_fields"][0], "operation");
    assert_eq!(line["redacted_fields"][1], "response");
}

// ---------------------------------------------------------------------------
// the OAuth mint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_mint_posts_a_client_credentials_grant_for_the_wiz_api_audience() {
    let server = MockServer::start().await;
    let token = common::jwt_with_dc_claim();

    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("grant_type=client_credentials"))
        .and(body_string_contains("audience=wiz-api"))
        .and(body_string_contains("client_id=svc-example"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": token,
            "expires_in": 3600,
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", format!("Bearer {token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "2"])
        .env("STAVE_CLIENT_ID", "svc-example")
        .env("STAVE_CLIENT_SECRET", "example-secret")
        .env("STAVE_TOKEN_URL", format!("{}/oauth/token", server.uri()))
        .env("STAVE_BASE_URL", graphql_url(&server)));

    assert!(
        out.status.success(),
        "mint then call failed: {}",
        stderr_of(&out)
    );
    assert_eq!(jsonl(&stdout_of(&out)).len(), 2);

    assert!(
        sandbox.token_cache_file().exists(),
        "a minted token is cached so the next pipeline stage does not re-mint"
    );
    let cached: Value =
        serde_json::from_str(&std::fs::read_to_string(sandbox.token_cache_file()).unwrap())
            .expect("cache file is JSON");
    assert_eq!(cached["client_id"], "svc-example");
    assert!(
        cached["expires_at"].as_str().is_some(),
        "the cache records its own expiry: {cached}"
    );

    let api = sandbox.api_audit_lines();
    assert_eq!(api.len(), 1);
    assert_eq!(
        api[0]["invocation"]["auth_source"], "env",
        "the secret came from STAVE_CLIENT_SECRET"
    );
    assert!(
        !api[0].to_string().contains("example-secret"),
        "the secret must never reach the trail: {:?}",
        api[0]
    );
}

#[tokio::test]
async fn a_fresh_cached_token_is_reused_instead_of_minting_again() {
    let server = MockServer::start().await;
    let token = common::jwt_with_dc_claim();

    // expect(0): the cache is fresh, so the mint must not be called.
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "should-never-be-minted",
            "expires_in": 3600,
        })))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", format!("Bearer {token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let token_url = format!("{}/oauth/token", server.uri());
    std::fs::create_dir_all(sandbox.token_cache_dir()).unwrap();
    std::fs::write(
        sandbox.token_cache_file(),
        json!({
            "access_token": token,
            "expires_at": (chrono::Utc::now() + chrono::TimeDelta::seconds(3600)).to_rfc3339(),
            "token_url": token_url,
            "client_id": "svc-example",
        })
        .to_string(),
    )
    .unwrap();

    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "2"])
        .env("STAVE_CLIENT_ID", "svc-example")
        .env("STAVE_CLIENT_SECRET", "example-secret")
        .env("STAVE_TOKEN_URL", &token_url)
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(out.status.success(), "{}", stderr_of(&out));
}

#[tokio::test]
async fn a_cached_token_for_a_different_client_is_not_reused() {
    let server = MockServer::start().await;
    let minted = common::jwt_with_dc_claim();

    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("client_id=svc-second"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": minted,
            "expires_in": 3600,
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", format!("Bearer {minted}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let token_url = format!("{}/oauth/token", server.uri());
    std::fs::create_dir_all(sandbox.token_cache_dir()).unwrap();
    std::fs::write(
        sandbox.token_cache_file(),
        json!({
            "access_token": "token-belonging-to-the-first-service-account",
            "expires_at": (chrono::Utc::now() + chrono::TimeDelta::seconds(3600)).to_rfc3339(),
            "token_url": token_url,
            "client_id": "svc-first",
        })
        .to_string(),
    )
    .unwrap();

    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "2"])
        .env("STAVE_CLIENT_ID", "svc-second")
        .env("STAVE_CLIENT_SECRET", "example-secret")
        .env("STAVE_TOKEN_URL", &token_url)
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(out.status.success(), "{}", stderr_of(&out));
}

#[tokio::test]
async fn a_refused_mint_reports_the_endpoint_and_the_credential_to_check() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "invalid_client"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {}})))
        .expect(0)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue"])
        .env("STAVE_CLIENT_ID", "svc-example")
        .env("STAVE_CLIENT_SECRET", "wrong-secret")
        .env("STAVE_TOKEN_URL", format!("{}/oauth/token", server.uri()))
        .env("STAVE_BASE_URL", graphql_url(&server)));

    assert!(!out.status.success(), "{out:?}");
    let err = stderr_of(&out);
    assert!(err.contains("token mint failed"), "{err}");
    assert!(
        err.contains("client ID/secret"),
        "the error must say what to check: {err}"
    );
    assert!(
        err.contains("/oauth/token"),
        "and which endpoint refused: {err}"
    );
    assert!(
        !sandbox.token_cache_file().exists(),
        "a failed mint must not write a cache entry"
    );
}

#[tokio::test]
async fn the_access_token_env_short_circuits_the_mint_entirely() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "x"})))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "Bearer example-access-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "issuesV2",
            issue_nodes(),
            None,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "2"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_CLIENT_ID", "svc-example")
        .env("STAVE_CLIENT_SECRET", "example-secret")
        .env("STAVE_TOKEN_URL", format!("{}/oauth/token", server.uri()))
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(out.status.success(), "{}", stderr_of(&out));

    let api = sandbox.api_audit_lines();
    assert_eq!(api.len(), 1);
    assert!(
        api[0]["invocation"]["auth_source"].is_null(),
        "a caller-supplied token has no chain source: {:?}",
        api[0]
    );
}
