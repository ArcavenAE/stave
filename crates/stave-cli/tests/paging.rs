//! Coverage for how `stream_kind` walks a GraphQL connection.
//!
//! Two defects lived here, both invisible from a single-page response,
//! which is why every other test in this suite missed them:
//!
//!   * a zero-node page ended the read even when the connection said it
//!     had more, so a short read looked like a complete one; and
//!   * page size was derived from the remaining `--limit`, which is
//!     correct only when every fetched record is emitted. With a
//!     client-side predicate (`search`, `list --since`) the limit never
//!     advances on a non-match, so the page stayed pinned at `--limit`
//!     for the length of the whole connection.
//!
//! Both are properties of the SECOND page onward, so these tests all
//! serve a sequence.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{Sandbox, connection_page, ids, request_variables, run, stderr_of, stdout_of};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Serves a fixed list of response bodies in order, then repeats the
/// last one forever. Repeating rather than 404ing is deliberate: a test
/// for a non-terminating loop has to let the loop try to keep going.
struct Sequence {
    pages: Vec<Value>,
    served: Arc<AtomicUsize>,
}

impl Sequence {
    fn new(pages: Vec<Value>) -> Self {
        Self {
            pages,
            served: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Respond for Sequence {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let n = self.served.fetch_add(1, Ordering::SeqCst);
        let body = self
            .pages
            .get(n)
            .or_else(|| self.pages.last())
            .expect("Sequence needs at least one page");
        ResponseTemplate::new(200).set_body_json(body)
    }
}

async fn serve(server: &MockServer, pages: Vec<Value>) {
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(Sequence::new(pages))
        .mount(server)
        .await;
}

/// The `first` variable of every request the server received, in order.
async fn page_sizes(server: &MockServer) -> Vec<u64> {
    server
        .received_requests()
        .await
        .expect("wiremock records requests")
        .iter()
        .map(|r| {
            request_variables(&r.body)
                .get("first")
                .and_then(Value::as_u64)
                .expect("every list request carries `first`")
        })
        .collect()
}

fn issue(id: &str, ty: &str) -> Value {
    json!({"id": id, "type": ty, "severity": "HIGH"})
}

// ---------------------------------------------------------------------------
// Defect A: a zero-node page is not the end of the connection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_empty_page_with_more_pages_is_followed_not_treated_as_the_end() {
    // The server has two records to give, with an empty page between
    // them. Before the fix this returned only the first, exit 0, with
    // nothing on stderr to say the read had been cut short.
    let server = MockServer::start().await;
    serve(
        &server,
        vec![
            connection_page("issuesV2", vec![issue("issue_01", "A")], Some("cursor_1")),
            connection_page("issuesV2", vec![], Some("cursor_2")),
            connection_page("issuesV2", vec![issue("issue_02", "B")], None),
        ],
    )
    .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "50"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", format!("{}/graphql", server.uri())));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(ids(&stdout_of(&out)), ["issue_01", "issue_02"]);
}

#[tokio::test]
async fn following_an_empty_page_says_so_on_stderr() {
    // A silent short read is the part that hurt. Even now that the read
    // completes, the caller should be able to see that the connection
    // behaved oddly, and stdout stays the data contract.
    let server = MockServer::start().await;
    serve(
        &server,
        vec![
            connection_page("issuesV2", vec![], Some("cursor_1")),
            connection_page("issuesV2", vec![issue("issue_01", "A")], None),
        ],
    )
    .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "50"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", format!("{}/graphql", server.uri())));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(ids(&stdout_of(&out)), ["issue_01"]);
    let err = stderr_of(&out);
    assert!(err.contains("empty page"), "{err}");
    assert!(
        err.contains("issuesV2"),
        "the warning names the field: {err}"
    );
}

#[tokio::test]
async fn a_cursor_that_does_not_advance_stops_the_read() {
    // This is the actual non-termination hazard the old code was
    // reaching for, and it has nothing to do with the page being empty.
    // The server hands back the same cursor forever; stave must stop,
    // and must say the read is incomplete rather than exiting quietly.
    let server = MockServer::start().await;
    serve(
        &server,
        vec![
            connection_page("issuesV2", vec![issue("issue_01", "A")], Some("stuck")),
            connection_page("issuesV2", vec![issue("issue_02", "B")], Some("stuck")),
        ],
    )
    .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "50"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", format!("{}/graphql", server.uri())));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(ids(&stdout_of(&out)), ["issue_01", "issue_02"]);
    let err = stderr_of(&out);
    assert!(err.contains("same cursor twice"), "{err}");
    assert!(err.contains("incomplete"), "{err}");
}

#[tokio::test]
async fn endless_empty_pages_with_fresh_cursors_are_bounded() {
    // Advancing cursors defeat the equality check, so the empty-page run
    // needs its own ceiling. Without it this pages against a live tenant
    // until someone notices.
    let server = MockServer::start().await;
    let mut pages = vec![connection_page(
        "issuesV2",
        vec![issue("issue_01", "A")],
        Some("cursor_0"),
    )];
    for i in 1..40 {
        pages.push(connection_page(
            "issuesV2",
            vec![],
            Some(&format!("cursor_{i}")),
        ));
    }
    serve(&server, pages).await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "50"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", format!("{}/graphql", server.uri())));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(ids(&stdout_of(&out)), ["issue_01"]);
    assert!(
        stderr_of(&out).contains("consecutive empty pages"),
        "{}",
        stderr_of(&out)
    );
    // One page of data plus the bounded run of empty ones. The exact
    // ceiling is MAX_EMPTY_PAGES; the assertion is that a ceiling exists
    // and is nowhere near the 40 pages on offer.
    let requests = page_sizes(&server).await.len();
    assert!((2..=12).contains(&requests), "{requests} requests");
}

// ---------------------------------------------------------------------------
// Defect B: page size is a fetch concern, not an output concern
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_asks_for_whole_pages_regardless_of_limit() {
    // `--limit 5` describes the answer, not the request. `emitted` only
    // moves when the predicate matches, so deriving the page from it
    // pinned every request to 5 records for the length of a connection
    // that can hold twenty thousand.
    let server = MockServer::start().await;
    serve(
        &server,
        vec![
            connection_page(
                "issuesV2",
                vec![issue("issue_01", "CLOUD_CONFIGURATION")],
                Some("cursor_1"),
            ),
            connection_page(
                "issuesV2",
                vec![issue("issue_02", "TOXIC_COMBINATION")],
                None,
            ),
        ],
    )
    .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["search", "issue", "toxic", "--limit", "5"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", format!("{}/graphql", server.uri())));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(ids(&stdout_of(&out)), ["issue_02"]);
    assert_eq!(
        page_sizes(&server).await,
        vec![500, 500],
        "a filtering read pages at MAX_PAGE_SIZE, not at --limit"
    );
}

#[tokio::test]
async fn list_since_asks_for_whole_pages_regardless_of_limit() {
    // `--since` is the same client-side predicate wearing a different
    // flag, and it had the same defect.
    let recent = chrono::Utc::now() - chrono::TimeDelta::hours(2);
    let ancient = chrono::Utc::now() - chrono::TimeDelta::days(400);

    let server = MockServer::start().await;
    serve(
        &server,
        vec![
            connection_page(
                "issuesV2",
                vec![json!({"id": "issue_old", "createdAt": ancient.to_rfc3339()})],
                Some("cursor_1"),
            ),
            connection_page(
                "issuesV2",
                vec![json!({"id": "issue_new", "createdAt": recent.to_rfc3339()})],
                None,
            ),
        ],
    )
    .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--since", "24h", "--limit", "3"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", format!("{}/graphql", server.uri())));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(ids(&stdout_of(&out)), ["issue_new"]);
    assert_eq!(page_sizes(&server).await, vec![500, 500]);
}

#[tokio::test]
async fn an_unfiltered_list_still_asks_for_only_what_the_limit_needs() {
    // The other half of the contract. With no predicate, every fetched
    // record is emitted, so asking for more than the limit would pull
    // tenant data nobody requested. Fixing the filtered case must not
    // turn `--limit 3` into a 500-record read.
    let server = MockServer::start().await;
    serve(
        &server,
        vec![connection_page(
            "issuesV2",
            vec![issue("issue_01", "A"), issue("issue_02", "B")],
            Some("cursor_1"),
        )],
    )
    .await;

    let sandbox = Sandbox::new();
    let out = run(sandbox
        .cmd()
        .args(["list", "issue", "--limit", "3"])
        .env("STAVE_ACCESS_TOKEN", "example-access-token")
        .env("STAVE_BASE_URL", format!("{}/graphql", server.uri())));
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(ids(&stdout_of(&out)), ["issue_01", "issue_02", "issue_01"]);
    assert_eq!(
        page_sizes(&server).await,
        vec![3, 1],
        "the second request asks only for the shortfall"
    );
}
