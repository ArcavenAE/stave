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

/// Scope names below follow the vendor's `verb:resource` grammar as
/// assembled from integration-vendor documentation. They are
/// PROVISIONAL until F1 live validation confirms them against the
/// tenant (the official scope list sits behind tenant-authenticated
/// docs). Scope-dependent surfaces (`stave auth plan`, `can-i`) must
/// mark their output provisional while this is true.
pub const SCOPE_METADATA_PROVISIONAL: bool = true;

/// What class of data a READ returns (D4a). Descriptive, never a
/// gate: the credential's scopes decide access; this exists so the
/// tool can say what an operation exposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sensitivity {
    Normal,
    Identity,
    Posture,
    Unknown,
}

impl Sensitivity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Sensitivity::Normal => "normal",
            Sensitivity::Identity => "identity",
            Sensitivity::Posture => "posture",
            Sensitivity::Unknown => "unknown",
        }
    }
}

/// Advisory query-cost hint (D4a). Informs future depth/size limits;
/// never a gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostHint {
    Light,
    Heavy,
    Unknown,
}

impl CostHint {
    pub fn as_str(&self) -> &'static str {
        match self {
            CostHint::Light => "light",
            CostHint::Heavy => "heavy",
            CostHint::Unknown => "unknown",
        }
    }
}

/// Authored mutation-consequence judgment (D4). The tier is the
/// conservative join of the scope-prefix tier and these axes; any
/// `Unknown` resolves to the strictest tier. Describes MUTATION
/// consequences only — read exposure is `Sensitivity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effects {
    pub reversibility: Reversibility,
    pub side_effects: SideEffects,
    pub egress: Egress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reversibility {
    Reversible,
    Irreversible,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideEffects {
    None,
    Notifies,
    TriggersIntegrations,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Egress {
    None,
    ProducesEgressArtifact,
    Unknown,
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
    /// Wiz API scopes this operation needs (D3, provisional until F1).
    /// `cargo xtask check-ops` fails when this is empty: no verb enters
    /// the registry without declaring its permission cost.
    ///
    /// The 2026-08-07 widening added joined scopes under one rule: a
    /// joined type earns a scope only when another registry operation
    /// lists exactly that type as its kind (`Issue.projects` reads
    /// `Project`, and `list_projects` declares `read:projects`).
    /// Joins to types with no registry analogue (`Identity`,
    /// `VulnerableAsset`, `Deployment`, `ServiceTicket`,
    /// `SecuritySubCategory`, `UserRole`, `IgnoreRule`) add nothing,
    /// because inventing a scope name would be a guess `auth plan`
    /// would then hand to an operator as a provisioning instruction.
    /// Whether Wiz enforces scopes at nested-field granularity at all
    /// is itself unverified. See docs/design/widening-notes.md.
    pub required_scopes: &'static [&'static str],
    /// What class of data this READ returns (D4a).
    pub sensitivity: Sensitivity,
    /// Advisory query-cost hint (D4a).
    pub cost_hint: CostHint,
    /// Mutation-consequence axes (D4). `None` for queries: the effects
    /// block describes mutation consequences only.
    pub effects: Option<Effects>,
}

/// All curated operations, in stable order.
pub const OPERATIONS: &[OperationDoc] = &[
    OperationDoc {
        name: "list_issues",
        document: include_str!("../ops/list_issues.graphql"),
        op_type: OpType::Query,
        root_field: "issuesV2",
        description: "Issues with severity, status, lifecycle timestamps, assignee, \
                      projects, service tickets, and the affected entity snapshot",
        required_scopes: &[
            "read:issues",
            "read:projects",
            "read:users",
            "read:service_accounts",
        ],
        sensitivity: Sensitivity::Posture,
        cost_hint: CostHint::Heavy,
        effects: None,
    },
    OperationDoc {
        name: "list_vulnerability_findings",
        document: include_str!("../ops/list_vulnerability_findings.graphql"),
        op_type: OpType::Query,
        root_field: "vulnerabilityFindings",
        description: "Vulnerability findings with canonical severity, fix and exploit \
                      status, the vulnerable asset, and the detection window",
        required_scopes: &["read:vulnerabilities", "read:projects"],
        sensitivity: Sensitivity::Posture,
        cost_hint: CostHint::Heavy,
        effects: None,
    },
    OperationDoc {
        name: "list_cloud_resources",
        document: include_str!("../ops/list_cloud_resources.graphql"),
        op_type: OpType::Query,
        root_field: "cloudResources",
        description: "Cloud resources with type, platform, and subscription",
        required_scopes: &["read:resources"],
        sensitivity: Sensitivity::Normal,
        cost_hint: CostHint::Heavy,
        effects: None,
    },
    OperationDoc {
        name: "list_cloud_resources_v2",
        document: include_str!("../ops/list_cloud_resources_v2.graphql"),
        op_type: OpType::Query,
        root_field: "cloudResourcesV2",
        description: "Cloud resources with exposure, sensitive-data, ownership, IaC, and rollups",
        // Same declaration as `list_cloud_resources`. The two bind
        // different root fields on the same noun, and nothing offline
        // can tell us whether Wiz gates the richer one differently.
        // Provisional under SCOPE_METADATA_PROVISIONAL like every other
        // entry, with the extra caveat that this one was matched to its
        // v1 sibling rather than derived.
        required_scopes: &["read:resources"],
        // Stronger than `list_cloud_resources` (Normal): this selection
        // carries internet-exposure and sensitive-data flags plus issue
        // and vulnerability counts per resource. It ALSO carries owner
        // identity, which a single-valued Sensitivity cannot express;
        // Posture is the closer of the two available answers.
        sensitivity: Sensitivity::Posture,
        cost_hint: CostHint::Heavy,
        effects: None,
    },
    OperationDoc {
        name: "list_projects",
        document: include_str!("../ops/list_projects.graphql"),
        op_type: OpType::Query,
        root_field: "projects",
        description: "Wiz projects with owners, security champions, business unit, and tags",
        required_scopes: &["read:projects", "read:users"],
        // Normal until 2026-08-07. `projectOwners` and
        // `securityChampions` name real employees, so the old value
        // understated what the read returns.
        sensitivity: Sensitivity::Identity,
        cost_hint: CostHint::Heavy,
        effects: None,
    },
    OperationDoc {
        name: "list_reports",
        document: include_str!("../ops/list_reports.graphql"),
        op_type: OpType::Query,
        root_field: "reports",
        description: "Configured reports",
        required_scopes: &["read:reports"],
        sensitivity: Sensitivity::Normal,
        cost_hint: CostHint::Light,
        effects: None,
    },
    OperationDoc {
        name: "list_controls",
        document: include_str!("../ops/list_controls.graphql"),
        op_type: OpType::Query,
        root_field: "controls",
        description: "Controls with severity, enablement, last-run health, service tickets, \
                      and compliance sub-categories",
        required_scopes: &["read:controls"],
        sensitivity: Sensitivity::Normal,
        cost_hint: CostHint::Heavy,
        effects: None,
    },
    OperationDoc {
        name: "list_security_frameworks",
        document: include_str!("../ops/list_security_frameworks.graphql"),
        op_type: OpType::Query,
        root_field: "securityFrameworks",
        // Heavy and multiplicative, not merely heavy: two nested
        // connections of 100 ride on every framework in the outer page,
        // and stave's pager walks the outer connection only.
        description: "Security/compliance frameworks with their control and \
                      cloud-configuration-rule rosters (nested pages capped at 100)",
        required_scopes: &[
            "read:security_frameworks",
            "read:controls",
            "read:cloud_configuration",
        ],
        sensitivity: Sensitivity::Normal,
        cost_hint: CostHint::Heavy,
        effects: None,
    },
    OperationDoc {
        name: "list_cloud_accounts",
        document: include_str!("../ops/list_cloud_accounts.graphql"),
        op_type: OpType::Query,
        root_field: "cloudAccounts",
        description: "Connected cloud accounts with scan window, system-health counts, \
                      linked projects, and source deployments",
        required_scopes: &["read:cloud_accounts", "read:projects"],
        sensitivity: Sensitivity::Normal,
        cost_hint: CostHint::Heavy,
        effects: None,
    },
    OperationDoc {
        name: "list_users",
        document: include_str!("../ops/list_users.graphql"),
        op_type: OpType::Query,
        root_field: "users",
        description: "Portal users with last login, enablement, suspension, and effective role",
        required_scopes: &["read:users"],
        sensitivity: Sensitivity::Identity,
        cost_hint: CostHint::Light,
        effects: None,
    },
    OperationDoc {
        name: "list_service_accounts",
        document: include_str!("../ops/list_service_accounts.graphql"),
        op_type: OpType::Query,
        root_field: "serviceAccounts",
        description: "API service accounts with login, rotation, expiry, enablement, and scopes",
        required_scopes: &["read:service_accounts"],
        sensitivity: Sensitivity::Identity,
        cost_hint: CostHint::Light,
        effects: None,
    },
    OperationDoc {
        name: "list_permission_scopes",
        document: include_str!("../ops/list_permission_scopes.graphql"),
        op_type: OpType::Query,
        root_field: "permissionScopes",
        description: "The tenant's scope vocabulary: every permission scope with its resource, permission class, and whether it can be narrowed to a project",
        // VALIDATED BY THE SERVER, 2026-08-08, and the only entry in this
        // registry that can say so. It began as a guess from the tenant's
        // observed convention, read:<resource_plural>. Run under the
        // twelve-scope measurement credential, which deliberately does not
        // hold it, the server refused and named it:
        //
        //   access denied, at least one of the following is required:
        //   [read:all read:permission_scopes]
        //
        // Two things fall out and both are larger than this entry. The
        // server publishes an operation's required scopes in its denial,
        // so an under-privileged credential is a scope-discovery
        // instrument and per-operation assignment is readable rather than
        // inferable. And read:all appears as an alternative in the
        // server's own list, which is the first direct evidence for the
        // D3 implication rule, unvalidated since scaffold.
        required_scopes: &["read:permission_scopes"],
        // Vendor vocabulary, not tenant data. No account, resource, person
        // or posture appears in this connection.
        sensitivity: Sensitivity::Normal,
        cost_hint: CostHint::Light,
        effects: None,
    },
    OperationDoc {
        name: "list_audit_log_entries",
        document: include_str!("../ops/list_audit_log_entries.graphql"),
        op_type: OpType::Query,
        root_field: "auditLogEntries",
        description: "Tenant audit log entries with the acting principal, action type, \
                      parameters, and source IP",
        required_scopes: &["admin:audit"],
        // Left at Posture despite now naming principals: `Sensitivity`
        // is one value, not a set, and swapping to Identity would drop
        // the posture signal without adding one. The single-value
        // limitation is recorded in docs/design/widening-notes.md.
        sensitivity: Sensitivity::Posture,
        cost_hint: CostHint::Heavy,
        effects: None,
    },
    OperationDoc {
        name: "list_cloud_configuration_rules",
        document: include_str!("../ops/list_cloud_configuration_rules.graphql"),
        op_type: OpType::Query,
        root_field: "cloudConfigurationRules",
        description: "Cloud configuration rules with severity and enablement",
        required_scopes: &["read:cloud_configuration"],
        sensitivity: Sensitivity::Normal,
        cost_hint: CostHint::Light,
        effects: None,
    },
];

/// Look up a curated operation by registry name.
pub fn find(name: &str) -> Option<&'static OperationDoc> {
    OPERATIONS.iter().find(|op| op.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tripwire, not a fact: registering an operation should be a
    /// deliberate act, so adding one must break a test and make someone
    /// say so in a commit. The name no longer spells the number, because
    /// the number changes and the intent does not.
    #[test]
    fn the_operation_count_is_pinned() {
        assert_eq!(
            OPERATIONS.len(),
            14,
            "an operation was added or removed; update this pin and say why in the commit"
        );
    }

    #[test]
    fn both_cloud_resource_bindings_are_registered() {
        // The v2 binding ships BESIDE v1, not instead of it: replacing
        // v1 would change the shape of an existing `_kind` stream.
        let v1 = find("list_cloud_resources").expect("v1 registered");
        let v2 = find("list_cloud_resources_v2").expect("v2 registered");
        assert_eq!(v1.root_field, "cloudResources");
        assert_eq!(v2.root_field, "cloudResourcesV2");
    }

    #[test]
    fn every_operation_declares_required_scopes() {
        // D3: no verb may enter the registry without declaring its
        // permission cost. This is the always-on companion to the
        // check-ops registry gate (which additionally needs the
        // vendored schema); this one runs with `cargo test`, no
        // service account and no schema required.
        for op in OPERATIONS {
            assert!(
                !op.required_scopes.is_empty(),
                "operation {} declares no required_scopes (D3)",
                op.name
            );
            for scope in op.required_scopes {
                assert!(
                    scope.contains(':'),
                    "operation {} scope {:?} is not verb:resource shaped",
                    op.name,
                    scope
                );
            }
        }
    }

    #[test]
    fn queries_carry_no_mutation_effects_block() {
        // D4: the effects block describes mutation consequences only.
        // v0.1 curates only reads, so every effects field is None.
        for op in OPERATIONS {
            if op.op_type == OpType::Query {
                assert!(
                    op.effects.is_none(),
                    "query {} carries a mutation effects block",
                    op.name
                );
            }
        }
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
