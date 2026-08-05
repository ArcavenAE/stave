//! Enrichment recipes for `stave enrich --with <recipe>`.
//!
//! v0.1 ships three recipes: the same three *shapes* sidestep proved
//! (join, roll-up, hoist), re-instantiated on Wiz security-graph
//! nouns:
//!
//! * `account-context` (join). For each `cloud_resource` record,
//!   attach its owning cloud account as an `account` field, joined on
//!   `subscriptionExternalId` == the account's `externalId`. Resources
//!   whose subscription has no matching account in the auxiliary set
//!   get `account: null`, so an orphan reference is data rather than a
//!   silent drop. Records of other kinds pass through unchanged.
//!   Requires an auxiliary stream of `cloud_account` records
//!   (`--accounts <FILE>`).
//!
//! * `severity-roll-up` (roll-up). For every record, populate
//!   `severity_rollup` from whichever severity field the kind carries:
//!   `vendorSeverity` (the `vulnerability_finding` carrier) falling
//!   back to `severity` (`issue`, `control`, `cloud_config_rule`).
//!   Records with neither get `severity_rollup: null`, so downstream
//!   rank predicates never have to special-case missing-vs-present
//!   across a mixed stream.
//!
//! * `entity-hoist` (hoist). Wiz nests the affected entity inside
//!   `issue.entitySnapshot`. This lifts `name`, `type`, and
//!   `cloudPlatform` to top-level `entity_name`, `entity_type`, and
//!   `entity_cloud_platform` so CEL predicates and `emit --format md`
//!   columns can reach them without a nested path.
//!
//! Recipe machinery: each recipe is a function `Record -> Record`
//! parameterised by an [`EnrichmentContext`] that carries the
//! pre-built auxiliary lookups. Building the context is a one-time
//! cost per `enrich` invocation; transformation is per-record.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::error::{Result, StaveError};
use crate::stream::Record;

/// Recipe selector. Stable string names match the CLI `--with` flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipe {
    AccountContext,
    SeverityRollUp,
    EntityHoist,
}

impl Recipe {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "account-context" => Some(Self::AccountContext),
            "severity-roll-up" => Some(Self::SeverityRollUp),
            "entity-hoist" => Some(Self::EntityHoist),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AccountContext => "account-context",
            Self::SeverityRollUp => "severity-roll-up",
            Self::EntityHoist => "entity-hoist",
        }
    }
}

/// Auxiliary lookups used by recipes. Built once per enrichment
/// invocation, then reused per record.
#[derive(Default, Debug)]
pub struct EnrichmentContext {
    /// `cloud_account` records indexed by `externalId`, the value a
    /// `cloud_resource` carries as `subscriptionExternalId`. Populated
    /// when the user passes `--accounts <FILE>` (or, in a future
    /// revision, when enrich auto-fetches accounts).
    pub accounts_by_external_id: HashMap<String, Record>,
}

impl EnrichmentContext {
    /// Build a context from a list of `cloud_account` records. Records
    /// with no `externalId` field, or whose id is not a string, are
    /// skipped: they cannot participate in the join.
    pub fn with_accounts<I>(accounts: I) -> Self
    where
        I: IntoIterator<Item = Record>,
    {
        let mut by_external_id = HashMap::new();
        for a in accounts {
            if let Some(external_id) = a.get("externalId").and_then(Value::as_str) {
                by_external_id.insert(external_id.to_string(), a);
            }
        }
        Self {
            accounts_by_external_id: by_external_id,
        }
    }

    pub fn validate_for(&self, recipe: Recipe) -> Result<()> {
        match recipe {
            Recipe::AccountContext => {
                if self.accounts_by_external_id.is_empty() {
                    return Err(StaveError::InvalidParam(
                        "--with account-context".into(),
                        "requires --accounts <FILE> with at least one cloud_account record".into(),
                    ));
                }
            }
            Recipe::SeverityRollUp | Recipe::EntityHoist => {}
        }
        Ok(())
    }
}

/// Apply one recipe to one record. Pure: same input ↔ same output.
pub fn apply(recipe: Recipe, record: Record, ctx: &EnrichmentContext) -> Record {
    match recipe {
        Recipe::AccountContext => apply_account_context(record, ctx),
        Recipe::SeverityRollUp => apply_severity_rollup(record),
        Recipe::EntityHoist => apply_entity_hoist(record),
    }
}

fn apply_account_context(mut record: Record, ctx: &EnrichmentContext) -> Record {
    if record.kind != "cloud_resource" {
        return record;
    }
    let owner = record
        .get("subscriptionExternalId")
        .and_then(Value::as_str)
        .and_then(|ext| ctx.accounts_by_external_id.get(ext));
    let attached = match owner {
        Some(a) => account_summary(a),
        None => Value::Null,
    };
    record.fields.insert("account".to_string(), attached);
    record
}

fn apply_severity_rollup(mut record: Record) -> Record {
    let value = SEVERITY_FIELDS
        .iter()
        .find_map(|f| record.get(f).and_then(Value::as_str))
        .map(|s| Value::String(s.to_string()))
        .unwrap_or(Value::Null);
    record.fields.insert("severity_rollup".to_string(), value);
    record
}

/// Severity carriers in Wiz payloads, most specific first.
/// `vulnerability_finding` reports `vendorSeverity`; `issue`,
/// `control`, and `cloud_config_rule` report `severity`.
const SEVERITY_FIELDS: &[&str] = &["vendorSeverity", "severity"];

fn apply_entity_hoist(mut record: Record) -> Record {
    let Some(snapshot) = record.get("entitySnapshot").cloned() else {
        return record;
    };
    for (nested, hoisted) in ENTITY_HOIST_FIELDS {
        if let Some(v) = snapshot.get(nested) {
            record.fields.insert((*hoisted).to_string(), v.clone());
        }
    }
    record
}

/// `entitySnapshot` sub-field → hoisted top-level name.
const ENTITY_HOIST_FIELDS: &[(&str, &str)] = &[
    ("name", "entity_name"),
    ("type", "entity_type"),
    ("cloudPlatform", "entity_cloud_platform"),
];

/// Reduce a `cloud_account` record to the summary attached by
/// `account-context`. Trims to the small set of fields downstream
/// filters and emit templates actually use; keeps the enriched stream
/// compact.
fn account_summary(a: &Record) -> Value {
    let mut out = serde_json::Map::new();
    for field in ["id", "name", "externalId", "cloudProvider", "status"] {
        if let Some(v) = a.get(field) {
            out.insert(field.into(), v.clone());
        }
    }
    Value::Object(out)
}

/// Severity ordering for downstream rank predicates:
/// CRITICAL > HIGH > MEDIUM > LOW > INFORMATIONAL. Wiz reports these
/// as upper-case enum members; the match is case-insensitive so a
/// hand-written predicate or a lower-cased upstream still ranks.
/// Unknown values rank lower than any known value.
pub fn severity_rank(s: &str) -> Option<u8> {
    match s.to_ascii_uppercase().as_str() {
        "CRITICAL" => Some(4),
        "HIGH" => Some(3),
        "MEDIUM" => Some(2),
        "LOW" => Some(1),
        "INFORMATIONAL" => Some(0),
        _ => None,
    }
}

/// Helper for tests and CLI: ergonomic constructor for a synthetic
/// `cloud_account` record (used by tests + recipe demos).
#[doc(hidden)]
pub fn synthetic_cloud_account(external_id: &str, name: &str) -> Record {
    Record::wrap(
        "cloud_account",
        crate::stream::SourceRef {
            operation_id: "synthetic".into(),
            response_index: 0,
            fetched_at: chrono::Utc::now(),
            trace_ref: None,
        },
        json!({
            "id": format!("acct_{external_id}"),
            "name": name,
            "externalId": external_id,
            "cloudProvider": "AWS",
            "status": "CONNECTED",
        }),
    )
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;

    use super::*;
    use crate::stream::SourceRef;

    fn source() -> SourceRef {
        SourceRef {
            operation_id: "op".into(),
            response_index: 0,
            fetched_at: Utc::now(),
            trace_ref: None,
        }
    }

    fn cloud_resource(id: &str, subscription: Option<&str>) -> Record {
        let mut body = json!({"id": id, "name": format!("example-{id}"), "type": "BUCKET"});
        if let Some(s) = subscription {
            body["subscriptionExternalId"] = json!(s);
        }
        Record::wrap("cloud_resource", source(), body)
    }

    fn vulnerability_finding(name: &str, vendor_severity: Option<&str>) -> Record {
        let mut body = json!({"id": format!("vf_{name}"), "name": name, "status": "OPEN"});
        if let Some(s) = vendor_severity {
            body["vendorSeverity"] = json!(s);
        }
        Record::wrap("vulnerability_finding", source(), body)
    }

    fn issue(id: &str, severity: &str, snapshot: Option<Value>) -> Record {
        let mut body = json!({"id": id, "type": "TOXIC_COMBINATION", "severity": severity});
        if let Some(s) = snapshot {
            body["entitySnapshot"] = s;
        }
        Record::wrap("issue", source(), body)
    }

    #[test]
    fn recipe_parse_round_trip() {
        for r in [
            Recipe::AccountContext,
            Recipe::SeverityRollUp,
            Recipe::EntityHoist,
        ] {
            assert_eq!(Recipe::parse(r.as_str()), Some(r));
        }
        assert_eq!(Recipe::parse("nope"), None);
    }

    #[test]
    fn account_context_attaches_owner_to_cloud_resource() {
        let ctx = EnrichmentContext::with_accounts([synthetic_cloud_account(
            "123456789012",
            "example-corp-prod",
        )]);
        let r = cloud_resource("res_1", Some("123456789012"));
        let enriched = apply(Recipe::AccountContext, r, &ctx);
        let account = enriched.get("account").expect("account attached");
        assert_eq!(
            account.get("externalId").and_then(Value::as_str),
            Some("123456789012")
        );
        assert_eq!(
            account.get("name").and_then(Value::as_str),
            Some("example-corp-prod")
        );
        assert_eq!(
            account.get("cloudProvider").and_then(Value::as_str),
            Some("AWS")
        );
    }

    #[test]
    fn account_context_marks_orphan_subscription_with_null() {
        let ctx = EnrichmentContext::with_accounts([synthetic_cloud_account(
            "123456789012",
            "example-corp-prod",
        )]);
        let r = cloud_resource("res_orphan", Some("999999999999"));
        let enriched = apply(Recipe::AccountContext, r, &ctx);
        assert_eq!(enriched.get("account"), Some(&Value::Null));
    }

    #[test]
    fn account_context_marks_missing_subscription_with_null() {
        let ctx = EnrichmentContext::with_accounts([synthetic_cloud_account(
            "123456789012",
            "example-corp-prod",
        )]);
        let r = cloud_resource("res_no_sub", None);
        let enriched = apply(Recipe::AccountContext, r, &ctx);
        assert_eq!(enriched.get("account"), Some(&Value::Null));
    }

    #[test]
    fn account_context_passes_through_other_kinds() {
        let ctx = EnrichmentContext::with_accounts([synthetic_cloud_account(
            "123456789012",
            "example-corp-prod",
        )]);
        let v = vulnerability_finding("CVE-2026-0001", Some("HIGH"));
        let enriched = apply(Recipe::AccountContext, v, &ctx);
        assert!(enriched.get("account").is_none());
    }

    #[test]
    fn with_accounts_skips_records_without_external_id() {
        let no_external_id = Record::wrap(
            "cloud_account",
            source(),
            json!({"id": "acct_x", "name": "example-corp-sandbox"}),
        );
        let ctx = EnrichmentContext::with_accounts([no_external_id]);
        assert!(ctx.accounts_by_external_id.is_empty());
    }

    #[test]
    fn severity_rollup_copies_vendor_severity() {
        let v = vulnerability_finding("CVE-2026-0001", Some("HIGH"));
        let enriched = apply(Recipe::SeverityRollUp, v, &EnrichmentContext::default());
        assert_eq!(
            enriched.get("severity_rollup").and_then(Value::as_str),
            Some("HIGH")
        );
    }

    #[test]
    fn severity_rollup_falls_back_to_plain_severity() {
        let i = issue("issue_1", "CRITICAL", None);
        let enriched = apply(Recipe::SeverityRollUp, i, &EnrichmentContext::default());
        assert_eq!(
            enriched.get("severity_rollup").and_then(Value::as_str),
            Some("CRITICAL")
        );
    }

    #[test]
    fn severity_rollup_prefers_vendor_severity_when_both_present() {
        let mut v = vulnerability_finding("CVE-2026-0002", Some("MEDIUM"));
        v.fields.insert("severity".into(), json!("LOW"));
        let enriched = apply(Recipe::SeverityRollUp, v, &EnrichmentContext::default());
        assert_eq!(
            enriched.get("severity_rollup").and_then(Value::as_str),
            Some("MEDIUM")
        );
    }

    #[test]
    fn severity_rollup_handles_missing_severity() {
        let r = cloud_resource("res_1", Some("123456789012"));
        let enriched = apply(Recipe::SeverityRollUp, r, &EnrichmentContext::default());
        assert_eq!(enriched.get("severity_rollup"), Some(&Value::Null));
    }

    #[test]
    fn entity_hoist_lifts_snapshot_fields_to_top_level() {
        let i = issue(
            "issue_1",
            "HIGH",
            Some(json!({
                "id": "ent_1",
                "name": "example-corp-public-bucket",
                "type": "BUCKET",
                "cloudPlatform": "AWS",
                "subscriptionExternalId": "123456789012",
            })),
        );
        let enriched = apply(Recipe::EntityHoist, i, &EnrichmentContext::default());
        assert_eq!(
            enriched.get("entity_name").and_then(Value::as_str),
            Some("example-corp-public-bucket")
        );
        assert_eq!(
            enriched.get("entity_type").and_then(Value::as_str),
            Some("BUCKET")
        );
        assert_eq!(
            enriched
                .get("entity_cloud_platform")
                .and_then(Value::as_str),
            Some("AWS")
        );
    }

    #[test]
    fn entity_hoist_leaves_the_snapshot_in_place() {
        let i = issue(
            "issue_1",
            "HIGH",
            Some(json!({"name": "example-corp-vm", "type": "VIRTUAL_MACHINE"})),
        );
        let enriched = apply(Recipe::EntityHoist, i, &EnrichmentContext::default());
        assert_eq!(
            enriched
                .get("entitySnapshot")
                .and_then(|s| s.get("name"))
                .and_then(Value::as_str),
            Some("example-corp-vm")
        );
    }

    #[test]
    fn entity_hoist_hoists_only_the_fields_present() {
        let i = issue("issue_1", "LOW", Some(json!({"name": "example-corp-db"})));
        let enriched = apply(Recipe::EntityHoist, i, &EnrichmentContext::default());
        assert_eq!(
            enriched.get("entity_name").and_then(Value::as_str),
            Some("example-corp-db")
        );
        assert!(enriched.get("entity_type").is_none());
        assert!(enriched.get("entity_cloud_platform").is_none());
    }

    #[test]
    fn entity_hoist_passes_through_records_without_a_snapshot() {
        let v = vulnerability_finding("CVE-2026-0001", None);
        let enriched = apply(Recipe::EntityHoist, v, &EnrichmentContext::default());
        assert!(enriched.get("entity_name").is_none());
        assert!(enriched.get("entity_type").is_none());
    }

    #[test]
    fn validate_account_context_requires_accounts() {
        let empty = EnrichmentContext::default();
        assert!(empty.validate_for(Recipe::AccountContext).is_err());
        let nonempty = EnrichmentContext::with_accounts([synthetic_cloud_account(
            "123456789012",
            "example-corp-prod",
        )]);
        assert!(nonempty.validate_for(Recipe::AccountContext).is_ok());
    }

    #[test]
    fn validate_passes_for_context_free_recipes() {
        let empty = EnrichmentContext::default();
        assert!(empty.validate_for(Recipe::SeverityRollUp).is_ok());
        assert!(empty.validate_for(Recipe::EntityHoist).is_ok());
    }

    #[test]
    fn severity_rank_orders_known_values() {
        assert!(severity_rank("CRITICAL") > severity_rank("HIGH"));
        assert!(severity_rank("HIGH") > severity_rank("MEDIUM"));
        assert!(severity_rank("MEDIUM") > severity_rank("LOW"));
        assert!(severity_rank("LOW") > severity_rank("INFORMATIONAL"));
        assert_eq!(severity_rank("bogus"), None);
    }

    #[test]
    fn severity_rank_is_case_insensitive() {
        assert_eq!(severity_rank("critical"), severity_rank("CRITICAL"));
        assert_eq!(
            severity_rank("Informational"),
            severity_rank("INFORMATIONAL")
        );
    }
}
