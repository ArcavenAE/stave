//! stave-api — the curated GraphQL operation library for the Wiz API.
//!
//! Each operation is a `.graphql` document under `ops/`, embedded at
//! compile time and validated against the vendored schema in `spec/`
//! by `cargo xtask check-ops`. Documents are the contract: the SDK
//! executes them verbatim; the CLI names them (`stave ops list`,
//! `stave api <name>`).
//!
//! Field selections are **provisional until live-validated** against a
//! real tenant (see charter.md F1) — chosen conservatively from Wiz's
//! published API examples so a wrong guess fails loudly as a GraphQL
//! validation error, never silently.

#![forbid(unsafe_code)]

/// Whether a document's top-level operation reads or writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpType {
    Query,
    Mutation,
}

impl OpType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OpType::Query => "query",
            OpType::Mutation => "mutation",
        }
    }
}

/// One curated operation: a named, embedded GraphQL document plus the
/// metadata the SDK needs to execute and audit it.
#[derive(Clone, Copy, Debug)]
pub struct OperationDoc {
    /// Stable registry name (`list_issues`). CLI-visible.
    pub name: &'static str,
    /// The GraphQL source, embedded verbatim.
    pub document: &'static str,
    /// Query or Mutation — drives the write-guard.
    pub op_type: OpType,
    /// The top-level response field (`issuesV2`) under `data` where
    /// the connection (nodes/pageInfo) lives.
    pub root_field: &'static str,
    /// One-line human description for `stave ops list`.
    pub description: &'static str,
}

/// All curated operations, in stable order.
pub const OPERATIONS: &[OperationDoc] = &[
    OperationDoc {
        name: "list_issues",
        document: include_str!("../ops/list_issues.graphql"),
        op_type: OpType::Query,
        root_field: "issuesV2",
        description: "Issues with severity, status, and the affected entity snapshot",
    },
    OperationDoc {
        name: "list_vulnerability_findings",
        document: include_str!("../ops/list_vulnerability_findings.graphql"),
        op_type: OpType::Query,
        root_field: "vulnerabilityFindings",
        description: "Vulnerability findings with vendor severity and detection window",
    },
    OperationDoc {
        name: "list_cloud_resources",
        document: include_str!("../ops/list_cloud_resources.graphql"),
        op_type: OpType::Query,
        root_field: "cloudResources",
        description: "Cloud resources with type, platform, and subscription",
    },
    OperationDoc {
        name: "list_projects",
        document: include_str!("../ops/list_projects.graphql"),
        op_type: OpType::Query,
        root_field: "projects",
        description: "Wiz projects (slug, description, archived)",
    },
    OperationDoc {
        name: "list_reports",
        document: include_str!("../ops/list_reports.graphql"),
        op_type: OpType::Query,
        root_field: "reports",
        description: "Configured reports",
    },
    OperationDoc {
        name: "list_controls",
        document: include_str!("../ops/list_controls.graphql"),
        op_type: OpType::Query,
        root_field: "controls",
        description: "Controls with severity and enablement",
    },
    OperationDoc {
        name: "list_security_frameworks",
        document: include_str!("../ops/list_security_frameworks.graphql"),
        op_type: OpType::Query,
        root_field: "securityFrameworks",
        description: "Security/compliance frameworks",
    },
    OperationDoc {
        name: "list_cloud_accounts",
        document: include_str!("../ops/list_cloud_accounts.graphql"),
        op_type: OpType::Query,
        root_field: "cloudAccounts",
        description: "Connected cloud accounts with provider and status",
    },
    OperationDoc {
        name: "list_users",
        document: include_str!("../ops/list_users.graphql"),
        op_type: OpType::Query,
        root_field: "users",
        description: "Portal users",
    },
    OperationDoc {
        name: "list_service_accounts",
        document: include_str!("../ops/list_service_accounts.graphql"),
        op_type: OpType::Query,
        root_field: "serviceAccounts",
        description: "API service accounts",
    },
    OperationDoc {
        name: "list_audit_log_entries",
        document: include_str!("../ops/list_audit_log_entries.graphql"),
        op_type: OpType::Query,
        root_field: "auditLogEntries",
        description: "Tenant audit log entries",
    },
    OperationDoc {
        name: "list_cloud_configuration_rules",
        document: include_str!("../ops/list_cloud_configuration_rules.graphql"),
        op_type: OpType::Query,
        root_field: "cloudConfigurationRules",
        description: "Cloud configuration rules with severity and enablement",
    },
];

/// Look up a curated operation by registry name.
pub fn find(name: &str) -> Option<&'static OperationDoc> {
    OPERATIONS.iter().find(|op| op.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_operations_registered() {
        assert_eq!(OPERATIONS.len(), 12);
    }

    #[test]
    fn names_are_unique() {
        let mut names: Vec<&str> = OPERATIONS.iter().map(|o| o.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
    }

    #[test]
    fn every_document_mentions_its_root_field() {
        for op in OPERATIONS {
            assert!(
                op.document.contains(op.root_field),
                "operation {} document does not contain root field {}",
                op.name,
                op.root_field
            );
        }
    }

    #[test]
    fn every_document_paginates() {
        for op in OPERATIONS {
            for needle in ["$first", "$after", "pageInfo", "endCursor", "hasNextPage"] {
                assert!(
                    op.document.contains(needle),
                    "operation {} missing {needle}",
                    op.name
                );
            }
        }
    }

    #[test]
    fn find_known_and_unknown() {
        assert!(find("list_issues").is_some());
        assert!(find("nope").is_none());
    }
}
