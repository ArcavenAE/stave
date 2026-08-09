//! The `_kind` types for stave v0.1 — Wiz security-graph nouns.
//!
//! Each kind binds:
//!   * a stable name (`issue`, `project`, …) — appears in `_kind`
//!   * a curated list operation (`stave list <kind>`)
//!   * the field that holds the primary key
//!   * severity / timestamp / search field metadata for the primitives
//!
//! Field metadata is drawn from Wiz's published API examples and is
//! **provisional until live-validated** (charter F1) — per-kind field
//! names could not be confirmed against real payloads at scaffold
//! time. Wrong guesses degrade gracefully: `emit --format md` shows
//! blanks and `--since` errors name the missing field.
//!
//! There is no `stave get <kind> <id>` in v0.1: singular lookups need
//! per-kind filter input types we will not guess at. Use `stave api`
//! with a custom document until introspection lands (charter F2).

use serde_json::Value;

/// Stable v0.1 kind names.
pub const KIND_ISSUE: &str = "issue";
pub const KIND_VULNERABILITY_FINDING: &str = "vulnerability_finding";
pub const KIND_CLOUD_RESOURCE: &str = "cloud_resource";
/// The `cloudResourcesV2` binding of the same noun. Kept beside
/// `cloud_resource` rather than replacing it: the two carry different
/// field sets, and re-pointing `cloud_resource` at the richer root
/// would change the shape of a stream consumers already read.
pub const KIND_CLOUD_RESOURCE_V2: &str = "cloud_resource_v2";
pub const KIND_PROJECT: &str = "project";
pub const KIND_REPORT: &str = "report";
pub const KIND_CONTROL: &str = "control";
pub const KIND_SECURITY_FRAMEWORK: &str = "security_framework";
pub const KIND_CLOUD_ACCOUNT: &str = "cloud_account";
pub const KIND_USER: &str = "user";
pub const KIND_SERVICE_ACCOUNT: &str = "service_account";
pub const KIND_AUDIT_LOG: &str = "audit_log";
pub const KIND_CLOUD_CONFIG_RULE: &str = "cloud_config_rule";
/// The tenant's scope vocabulary. Unlike every other kind here, this
/// carries no tenant data: no account, resource, person or posture, only
/// the vendor's own permission names and their descriptions. That is the
/// point of it, and it is why `scrub.sh --catalog` is the right mode.
pub const KIND_PERMISSION_SCOPE: &str = "permission_scope";

/// All v0.1 kind names, in stable order.
pub const ALL_KINDS: &[&str] = &[
    KIND_ISSUE,
    KIND_VULNERABILITY_FINDING,
    KIND_CLOUD_RESOURCE,
    KIND_CLOUD_RESOURCE_V2,
    KIND_PROJECT,
    KIND_REPORT,
    KIND_CONTROL,
    KIND_SECURITY_FRAMEWORK,
    KIND_CLOUD_ACCOUNT,
    KIND_USER,
    KIND_SERVICE_ACCOUNT,
    KIND_AUDIT_LOG,
    KIND_CLOUD_CONFIG_RULE,
    KIND_PERMISSION_SCOPE,
];

/// Static metadata for one `_kind`.
#[derive(Debug, Clone)]
pub struct KindSpec {
    /// Stable name in the stream contract.
    pub name: &'static str,

    /// Curated operation name for `stave list <kind>`.
    pub list_operation: &'static str,

    /// Field name in each item that carries the stable primary key.
    pub id_field: &'static str,

    /// Field name in each item that carries severity, when present.
    pub severity_field: Option<&'static str>,

    /// Field name in each item that carries the canonical timestamp
    /// (used by `--since` and the canonical adapter's `now` binding
    /// comparisons).
    pub primary_timestamp_field: Option<&'static str>,

    /// Field name on each record that `stave search <kind> <text>`
    /// matches against (case-insensitive substring). `None` means
    /// search isn't supported for this kind in v0.1 — operators
    /// compose `list | filter` instead.
    pub search_field: Option<&'static str>,
}

/// Look up a kind by its stream-contract name.
pub fn kind_spec(name: &str) -> Option<&'static KindSpec> {
    KIND_TABLE.iter().find(|k| k.name == name)
}

/// All v0.1 kind specs.
pub fn all_kinds() -> &'static [KindSpec] {
    KIND_TABLE
}

/// The static kind → operation table. Operation names are curated
/// registry names from `stave-api`.
const KIND_TABLE: &[KindSpec] = &[
    KindSpec {
        name: KIND_ISSUE,
        list_operation: "list_issues",
        id_field: "id",
        severity_field: Some("severity"),
        primary_timestamp_field: Some("createdAt"),
        search_field: Some("type"),
    },
    KindSpec {
        name: KIND_VULNERABILITY_FINDING,
        list_operation: "list_vulnerability_findings",
        id_field: "id",
        severity_field: Some("vendorSeverity"),
        primary_timestamp_field: Some("firstDetectedAt"),
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_CLOUD_RESOURCE,
        list_operation: "list_cloud_resources",
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_CLOUD_RESOURCE_V2,
        list_operation: "list_cloud_resources_v2",
        id_field: "id",
        // A resource has no severity of its own. `issueAnalytics` and
        // `vulnerabilityAnalytics` carry per-severity COUNTS, which is
        // a different shape: `emit` expects a single severity value,
        // and pointing it at a count object would print nonsense.
        severity_field: None,
        // `firstSeen` is non-null and is when the resource entered the
        // security graph, which is what `--since` means for an
        // inventory. `createdAt` is the cloud-side creation time and is
        // nullable; `lastSeen` answers a different question (is it
        // still there) and belongs in a predicate, not in `--since`.
        primary_timestamp_field: Some("firstSeen"),
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_PROJECT,
        list_operation: "list_projects",
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_REPORT,
        list_operation: "list_reports",
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_CONTROL,
        list_operation: "list_controls",
        id_field: "id",
        severity_field: Some("severity"),
        primary_timestamp_field: None,
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_SECURITY_FRAMEWORK,
        list_operation: "list_security_frameworks",
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_CLOUD_ACCOUNT,
        list_operation: "list_cloud_accounts",
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_USER,
        list_operation: "list_users",
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        search_field: Some("email"),
    },
    KindSpec {
        name: KIND_SERVICE_ACCOUNT,
        list_operation: "list_service_accounts",
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: Some("createdAt"),
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_AUDIT_LOG,
        list_operation: "list_audit_log_entries",
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: Some("timestamp"),
        search_field: Some("action"),
    },
    KindSpec {
        name: KIND_CLOUD_CONFIG_RULE,
        list_operation: "list_cloud_configuration_rules",
        id_field: "id",
        severity_field: Some("severity"),
        primary_timestamp_field: None,
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_PERMISSION_SCOPE,
        list_operation: "list_permission_scopes",
        id_field: "id",
        severity_field: None,
        // addedAt is when the vendor added the scope to the product, not
        // an event in this tenant, so --since over it would read as
        // "scopes introduced recently", which is a real question but not
        // the one --since is for anywhere else in this table.
        primary_timestamp_field: None,
        search_field: Some("scope"),
    },
];

/// Extract the array of items from a GraphQL `data` value. With a
/// known root field, read `data.<root>.nodes`; otherwise scan `data`'s
/// top-level objects for the first `nodes` array. Mirrors the
/// detection logic in the client's `count_items` so the audit-emitted
/// `items_returned` matches what the primitive actually streams.
pub fn extract_items<'a>(data: &'a Value, root_field: Option<&str>) -> Option<&'a [Value]> {
    let connection = match root_field {
        Some(field) => data.get(field)?,
        None => data
            .as_object()?
            .values()
            .find(|v| v.get("nodes").is_some_and(Value::is_array))?,
    };
    connection
        .get("nodes")
        .and_then(Value::as_array)
        .map(|a| a.as_slice())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The count is a tripwire so adding a kind is deliberate. The
    /// agreement between the two lists is the real invariant: they are
    /// written separately and a kind present in one and absent from the
    /// other is a silent gap.
    #[test]
    fn the_kind_table_and_name_list_agree() {
        assert_eq!(
            KIND_TABLE.len(),
            14,
            "a kind was added or removed; update this pin and say why in the commit"
        );
        assert_eq!(ALL_KINDS.len(), KIND_TABLE.len());
        for (a, b) in KIND_TABLE.iter().zip(ALL_KINDS.iter()) {
            assert_eq!(&a.name, b);
        }
    }

    #[test]
    fn both_cloud_resource_kinds_are_present_and_distinct() {
        let v1 = kind_spec("cloud_resource").expect("v1 in table");
        let v2 = kind_spec("cloud_resource_v2").expect("v2 in table");
        assert_eq!(v1.list_operation, "list_cloud_resources");
        assert_eq!(v2.list_operation, "list_cloud_resources_v2");
        // v1 has no usable timestamp; v2 does. That difference is the
        // reason the second kind exists rather than replacing the first.
        assert_eq!(v1.primary_timestamp_field, None);
        assert_eq!(v2.primary_timestamp_field, Some("firstSeen"));
    }

    #[test]
    fn lookup_known_kind() {
        let k = kind_spec("issue").expect("issue in table");
        assert_eq!(k.list_operation, "list_issues");
        assert_eq!(k.id_field, "id");
        assert_eq!(k.severity_field, Some("severity"));
    }

    #[test]
    fn vulnerability_finding_carries_vendor_severity() {
        let k = kind_spec("vulnerability_finding").expect("in table");
        assert_eq!(k.severity_field, Some("vendorSeverity"));
        assert_eq!(k.primary_timestamp_field, Some("firstDetectedAt"));
    }

    #[test]
    fn every_list_operation_exists_in_registry() {
        // The kind table references curated operation names; keep it
        // honest against the embedded operation library.
        for k in KIND_TABLE {
            let op = crate::ops::find(k.list_operation);
            assert!(
                op.is_ok(),
                "kind {} list op {} missing",
                k.name,
                k.list_operation
            );
        }
    }

    #[test]
    fn lookup_unknown_kind() {
        assert!(kind_spec("nope").is_none());
    }

    #[test]
    fn extract_items_reads_connection_with_root_field() {
        let data = json!({"issuesV2": {"nodes": [{"id": "a"}, {"id": "b"}],
            "pageInfo": {"hasNextPage": false}}});
        assert_eq!(extract_items(&data, Some("issuesV2")).unwrap().len(), 2);
    }

    #[test]
    fn extract_items_scans_without_root_field() {
        let data = json!({"projects": {"nodes": [{"id": "p"}]}});
        assert_eq!(extract_items(&data, None).unwrap().len(), 1);
    }

    #[test]
    fn extract_items_returns_none_when_no_connection() {
        let data = json!({"viewer": {"id": "x"}});
        assert!(extract_items(&data, None).is_none());
    }
}
