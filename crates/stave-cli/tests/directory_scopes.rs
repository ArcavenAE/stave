//! `--from-directory`: the second route to the caller's own granted
//! scopes, when the token's own claim is an opaque bitmask.
//!
//! Covers the four outcomes that matter, plus the two properties this
//! route must not lose:
//!
//!   * the caller's own record is matched by client ID and its scopes
//!     are reported, tagged `source: "directory"`
//!   * a neighbour's scopes never appear in the output
//!   * an empty scope list and an absent account are DIFFERENT answers
//!   * the default (no flag) still makes no API call at all
//!
//! SAFETY: every request here targets a LOCAL wiremock server on
//! 127.0.0.1, never a real Wiz tenant. Every response body is
//! synthetic, and every client ID, scope, and account name below is
//! invented. No test performs, or could perform, any active,
//! change, remediation, or destructive action against any real system.

mod common;

use common::{Sandbox, connection_page, run, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn graphql_url(server: &MockServer) -> String {
    format!("{}/graphql", server.uri())
}

/// A token whose payload carries `encodedScopes` and nothing readable —
/// the F1 shape, which is what makes the directory route necessary.
/// Payload is base64url-no-pad of `{"encodedScopes":"AQID"}`.
const OPAQUE_TOKEN: &str = "header.eyJlbmNvZGVkU2NvcGVzIjoiQVFJRCJ9.sig";

fn account(client_id: &str, scopes: &[&str]) -> Value {
    json!({
        "id": format!("sa_{client_id}"),
        "name": format!("account-{client_id}"),
        "clientId": client_id,
        "scopes": scopes,
        "enabled": true,
    })
}

async fn mount_directory(server: &MockServer, nodes: Vec<Value>) {
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(connection_page(
            "serviceAccounts",
            nodes,
            None,
        )))
        .mount(server)
        .await;
}

#[tokio::test]
async fn own_scopes_are_read_from_the_directory_and_tagged_as_such() {
    let server = MockServer::start().await;
    mount_directory(
        &server,
        vec![
            account("someone-else", &["admin:everything"]),
            account("our-client-id", &["read:issues", "read:projects"]),
        ],
    )
    .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["auth", "scopes", "--from-directory"])
        .env("STAVE_ACCESS_TOKEN", OPAQUE_TOKEN)
        .env("STAVE_CLIENT_ID", "our-client-id")
        .env("STAVE_BASE_URL", graphql_url(&server)));

    assert!(
        out.status.success(),
        "directory lookup failed: {}",
        stderr_of(&out)
    );
    let rec: Value = serde_json::from_str(&stdout_of(&out)).expect("stdout is one JSON object");
    assert_eq!(rec["source"], "directory", "{rec}");
    assert_eq!(
        rec["scopes"],
        json!(["read:issues", "read:projects"]),
        "{rec}"
    );
}

/// The hygiene property, asserted at the boundary that actually prints.
/// The unit test in the SDK proves the matcher; this proves nothing
/// leaks through the CLI's own formatting on the way out.
#[tokio::test]
async fn another_accounts_scopes_never_reach_stdout() {
    let server = MockServer::start().await;
    mount_directory(
        &server,
        vec![
            account("someone-else", &["admin:everything", "write:issues"]),
            account("our-client-id", &["read:issues"]),
        ],
    )
    .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["auth", "scopes", "--from-directory"])
        .env("STAVE_ACCESS_TOKEN", OPAQUE_TOKEN)
        .env("STAVE_CLIENT_ID", "our-client-id")
        .env("STAVE_BASE_URL", graphql_url(&server)));

    let printed = stdout_of(&out);
    assert!(
        !printed.contains("admin:everything") && !printed.contains("write:issues"),
        "a neighbour's scopes reached stdout: {printed}"
    );
    assert!(
        !printed.contains("someone-else"),
        "a neighbour's client id reached stdout: {printed}"
    );
}

/// "Found with no scopes" and "not found at all" are different facts
/// about the world and must not collapse into one message: the first
/// says the account holds nothing, the second says we could not see
/// ourselves. Conflating them would send someone to the wrong fix.
#[tokio::test]
async fn an_empty_scope_list_and_an_absent_account_report_differently() {
    let empty_server = MockServer::start().await;
    mount_directory(&empty_server, vec![account("our-client-id", &[])]).await;
    let sandbox = Sandbox::new();
    let empty = run(sandbox
        .cmd()
        .args(["auth", "scopes", "--from-directory"])
        .env("STAVE_ACCESS_TOKEN", OPAQUE_TOKEN)
        .env("STAVE_CLIENT_ID", "our-client-id")
        .env("STAVE_BASE_URL", graphql_url(&empty_server)));
    assert!(!empty.status.success(), "empty scopes should not exit 0");
    let empty_err = stderr_of(&empty);
    assert!(
        empty_err.contains("empty scope list"),
        "empty case did not say so: {empty_err}"
    );

    let absent_server = MockServer::start().await;
    mount_directory(&absent_server, vec![account("someone-else", &["read:all"])]).await;
    let sandbox2 = Sandbox::new();
    let absent = run(sandbox2
        .cmd()
        .args(["auth", "scopes", "--from-directory"])
        .env("STAVE_ACCESS_TOKEN", OPAQUE_TOKEN)
        .env("STAVE_CLIENT_ID", "our-client-id")
        .env("STAVE_BASE_URL", graphql_url(&absent_server)));
    assert!(!absent.status.success(), "absent account should not exit 0");
    let absent_err = stderr_of(&absent);
    assert!(
        absent_err.contains("not in the tenant's service-account directory"),
        "absent case did not say so: {absent_err}"
    );
    assert!(
        !absent_err.contains("empty scope list"),
        "absent case reported as empty: {absent_err}"
    );
}

/// `auth can-i` answers instead of refusing, once the directory route
/// supplies the grant set. This is the whole point of the ticket.
#[tokio::test]
async fn can_i_answers_through_the_directory_instead_of_refusing() {
    let server = MockServer::start().await;
    mount_directory(
        &server,
        vec![account("our-client-id", &["read:issues", "read:projects"])],
    )
    .await;

    let sandbox = Sandbox::new();
    let refused = run(sandbox
        .cmd()
        .args(["auth", "can-i", "list_issues"])
        .env("STAVE_ACCESS_TOKEN", OPAQUE_TOKEN)
        .env("STAVE_CLIENT_ID", "our-client-id")
        .env("STAVE_BASE_URL", graphql_url(&server)));
    assert!(
        !refused.status.success(),
        "without the flag the opaque token must still refuse"
    );
    assert!(
        stderr_of(&refused).contains("--from-directory"),
        "the refusal should name the route that would work: {}",
        stderr_of(&refused)
    );

    let sandbox2 = Sandbox::new();
    let answered = run(sandbox2
        .cmd()
        .args(["auth", "can-i", "list_issues", "--from-directory"])
        .env("STAVE_ACCESS_TOKEN", OPAQUE_TOKEN)
        .env("STAVE_CLIENT_ID", "our-client-id")
        .env("STAVE_BASE_URL", graphql_url(&server)));
    // Whether the verdict is yes or no depends on list_issues' declared
    // required_scopes, which this test deliberately does not pin. What
    // it asserts is that a VERDICT was reached rather than a refusal.
    let err = stderr_of(&answered);
    assert!(
        !err.contains("opaque bitmask"),
        "still refusing with the flag set: {err}"
    );
}

/// The default must stay offline. `auth scopes` has always been a pure
/// read of the token at hand; if this ever starts calling the API
/// without the flag, `expect(0)` fails when the mock server drops.
#[tokio::test]
async fn without_the_flag_no_api_call_is_made() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["auth", "scopes"])
        .env("STAVE_ACCESS_TOKEN", OPAQUE_TOKEN)
        .env("STAVE_CLIENT_ID", "our-client-id")
        .env("STAVE_BASE_URL", graphql_url(&server)));

    // The opaque token still reports opaque, offline, exit 0.
    assert!(out.status.success(), "{}", stderr_of(&out));
    let rec: Value = serde_json::from_str(&stdout_of(&out)).expect("stdout is one JSON object");
    assert_eq!(rec["claim_field"], "encodedScopes", "{rec}");
    assert_eq!(rec["enumerable"], json!(false), "{rec}");
}
