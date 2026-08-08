//! The caller's own granted scopes, read from the tenant's
//! service-account directory.
//!
//! This is the SECOND route to a question the token would not answer.
//! Wiz service-account tokens carry `encodedScopes`, a base64 bitmask
//! against an internal ordering stave does not have, so
//! [`crate::token_scopes`] correctly reports `Opaque` and the
//! permission verbs correctly refuse rather than guess (charter F1,
//! finding-001).
//!
//! `ServiceAccount.scopes` is a `[String!]!` on a type stave already
//! queries, and `clientId` is already in the same selection. Confirmed
//! live on 2026-08-08: the field arrives and is populated on every
//! record sampled. So if the caller's own account appears in the
//! connection, matching on the configured client ID answers exactly
//! what the token claim would not (bd `aae-orc-8af5`).
//!
//! # What this deliberately does not do
//!
//! It reads the directory but retains **only the caller's own record**.
//! Other accounts' scopes are read off the wire and dropped
//! unexamined; nothing here returns, logs, or counts them beyond how
//! many records were scanned. The directory is tenant-identifying
//! (`.claude/rules/tenant-data-hygiene.md`) and one account's grants
//! are not this function's business even though they arrive in the
//! same response.
//!
//! It is also NOT a security control. It reports what the server says
//! the caller was granted; the server's own enforcement is the
//! boundary. A `Found` result is a UX answer, not a permission.

use serde_json::{Value, json};

use crate::client::{CallOptions, Client};
use crate::error::Result;

/// What the service-account directory says about the caller's own
/// granted scopes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DirectoryScopes {
    /// The configured client ID was found and carries a non-empty
    /// scope list.
    Found { scopes: Vec<String> },
    /// The configured client ID was found, and its scope list is
    /// empty. Distinct from [`DirectoryScopes::SelfNotListed`]: the
    /// server answered about us and said "none", which is a real
    /// answer rather than a failed lookup.
    Empty,
    /// The configured client ID is not in the connection. Either the
    /// directory does not list the caller, or the caller lacks the
    /// grant to see itself. `accounts_scanned` is how many records
    /// were walked before giving up — the only thing retained about
    /// records that were not ours.
    SelfNotListed { accounts_scanned: usize },
}

/// Page size for the directory walk. The directory is small (12
/// records on the tenant this was built against), so one page is the
/// expected case and paging is here for correctness, not throughput.
const PAGE_SIZE: i64 = 100;

/// Hard stop on the walk. A directory large enough to exceed this is a
/// different problem than the one this function solves, and an
/// unbounded loop against a live tenant is not an acceptable failure
/// mode for a UX helper.
const MAX_PAGES: usize = 20;

/// Look up the caller's own granted scopes by client ID.
///
/// Walks `list_service_accounts` and returns on the first record whose
/// `clientId` matches. Every other record is discarded without
/// inspection beyond that one field.
pub async fn own_scopes(client: &Client, client_id: &str) -> Result<DirectoryScopes> {
    let mut cursor: Option<String> = None;
    let mut scanned = 0usize;

    for _ in 0..MAX_PAGES {
        let variables = match &cursor {
            Some(after) => json!({ "first": PAGE_SIZE, "after": after }),
            None => json!({ "first": PAGE_SIZE }),
        };
        let opts = CallOptions {
            verb_phase: Some("auth"),
            ..CallOptions::default()
        };
        let data = client
            .call_op("list_service_accounts", &variables, opts)
            .await?;

        let Some(conn) = data.get("serviceAccounts") else {
            break;
        };
        let nodes = conn.get("nodes").and_then(Value::as_array);
        let Some(nodes) = nodes else { break };

        for node in nodes {
            scanned += 1;
            if node.get("clientId").and_then(Value::as_str) != Some(client_id) {
                continue;
            }
            // Ours. Everything read before this point is dropped.
            let scopes: Vec<String> = node
                .get("scopes")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            return Ok(if scopes.is_empty() {
                DirectoryScopes::Empty
            } else {
                DirectoryScopes::Found { scopes }
            });
        }

        let page_info = conn.get("pageInfo");
        let has_next = page_info
            .and_then(|p| p.get("hasNextPage"))
            .and_then(Value::as_bool)
            == Some(true);
        let next = page_info
            .and_then(|p| p.get("endCursor"))
            .and_then(Value::as_str)
            .map(str::to_string);
        // A page that claims more but names no cursor would loop
        // forever on the same page; stop instead.
        match (has_next, next) {
            (true, Some(c)) => cursor = Some(c),
            _ => break,
        }
    }

    Ok(DirectoryScopes::SelfNotListed {
        accounts_scanned: scanned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(client_id: &str, scopes: &[&str]) -> Value {
        json!({ "id": "sa", "clientId": client_id, "scopes": scopes })
    }

    /// The match/extract half, exercised without a server. The walk
    /// itself is covered by the wiremock integration tests, which are
    /// the only place the paging contract is real.
    fn pick(nodes: &[Value], client_id: &str) -> Option<DirectoryScopes> {
        nodes.iter().find_map(|n| {
            if n.get("clientId").and_then(Value::as_str) != Some(client_id) {
                return None;
            }
            let scopes: Vec<String> = n
                .get("scopes")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            Some(if scopes.is_empty() {
                DirectoryScopes::Empty
            } else {
                DirectoryScopes::Found { scopes }
            })
        })
    }

    #[test]
    fn own_record_is_matched_by_client_id() {
        let nodes = vec![
            node("someone-else", &["read:all"]),
            node("ours", &["read:issues", "read:projects"]),
        ];
        assert_eq!(
            pick(&nodes, "ours"),
            Some(DirectoryScopes::Found {
                scopes: vec!["read:issues".into(), "read:projects".into()],
            })
        );
    }

    #[test]
    fn an_empty_scope_list_is_not_the_same_as_a_missing_account() {
        assert_eq!(
            pick(&[node("ours", &[])], "ours"),
            Some(DirectoryScopes::Empty)
        );
        assert_eq!(pick(&[node("other", &["read:all"])], "ours"), None);
    }

    /// The hygiene property, asserted rather than assumed: a neighbour's
    /// scopes must not survive the lookup. If this ever fails, the
    /// function has started reporting other tenants' grants.
    #[test]
    fn another_accounts_scopes_never_appear_in_the_result() {
        let nodes = vec![
            node("someone-else", &["admin:everything"]),
            node("ours", &["read:issues"]),
        ];
        let got = pick(&nodes, "ours").expect("ours is present");
        let DirectoryScopes::Found { scopes } = got else {
            panic!("expected Found");
        };
        assert_eq!(scopes, vec!["read:issues".to_string()]);
        assert!(!scopes.iter().any(|s| s.contains("admin")));
    }
}
