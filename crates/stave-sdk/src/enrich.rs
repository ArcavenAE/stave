//! Enrichment recipes for `stave enrich --with <recipe>`.
//!
//! v0.1 ships three recipes — the same three *shapes* sidestep proved
//! (join, roll-up, hoist), re-instantiated on iru nouns:
//!
//! * `blueprint-context` — for each device record, attach its
//!   blueprint as a `blueprint` field (joined on `blueprint_id`).
//!   Orphan devices (no matching blueprint in the auxiliary set) get
//!   `blueprint: null`. Non-device records pass through unchanged.
//!   Requires an auxiliary stream of blueprint records
//!   (`--blueprints <FILE>`).
//!
//! * `severity-roll-up` — for every record, populate
//!   `severity_rollup` from the record's own `severity` (copy-rename
//!   so downstream rank predicates don't have to special-case
//!   missing-vs-present). iru's severity carrier is the
//!   `vulnerability` kind; records without severity get
//!   `severity_rollup: null`.
//!
//! * `device-platform` — for any record with a top-level `platform`
//!   field, hoist a normalized `_platform` copy. Useful for mixed
//!   streams where downstream predicates group by platform.
//!
//! Recipe machinery: each recipe is a function `Record -> Record`
//! parameterised by an [`EnrichmentContext`] that carries the
//! pre-built auxiliary lookups. Building the context is a one-time
//! cost per `enrich` invocation; transformation is per-record.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::error::{StaveError, Result};
use crate::stream::Record;

/// Recipe selector. Stable string names match the CLI `--with` flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipe {
    BlueprintContext,
    SeverityRollUp,
    DevicePlatform,
}

impl Recipe {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "blueprint-context" => Some(Self::BlueprintContext),
            "severity-roll-up" => Some(Self::SeverityRollUp),
            "device-platform" => Some(Self::DevicePlatform),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BlueprintContext => "blueprint-context",
            Self::SeverityRollUp => "severity-roll-up",
            Self::DevicePlatform => "device-platform",
        }
    }
}

/// Auxiliary lookups used by recipes. Built once per enrichment
/// invocation, then reused per record.
#[derive(Default, Debug)]
pub struct EnrichmentContext {
    /// Blueprint records indexed by `id`. Populated when the user
    /// passes `--blueprints <FILE>` (or, in a future revision, when
    /// enrich auto-fetches blueprints).
    pub blueprints_by_id: HashMap<String, Record>,
}

impl EnrichmentContext {
    /// Build a context from a list of blueprint records. Records with
    /// no `id` field, or whose id is not a string, are skipped.
    pub fn with_blueprints<I>(blueprints: I) -> Self
    where
        I: IntoIterator<Item = Record>,
    {
        let mut by_id = HashMap::new();
        for b in blueprints {
            if let Some(id) = b.get("id").and_then(Value::as_str) {
                by_id.insert(id.to_string(), b);
            }
        }
        Self {
            blueprints_by_id: by_id,
        }
    }

    pub fn validate_for(&self, recipe: Recipe) -> Result<()> {
        match recipe {
            Recipe::BlueprintContext => {
                if self.blueprints_by_id.is_empty() {
                    return Err(StaveError::InvalidParam(
                        "--with blueprint-context".into(),
                        "requires --blueprints <FILE> with at least one blueprint record".into(),
                    ));
                }
            }
            Recipe::SeverityRollUp | Recipe::DevicePlatform => {}
        }
        Ok(())
    }
}

/// Apply one recipe to one record. Pure: same input ↔ same output.
pub fn apply(recipe: Recipe, record: Record, ctx: &EnrichmentContext) -> Record {
    match recipe {
        Recipe::BlueprintContext => apply_blueprint_context(record, ctx),
        Recipe::SeverityRollUp => apply_severity_rollup(record),
        Recipe::DevicePlatform => apply_device_platform(record),
    }
}

fn apply_blueprint_context(mut record: Record, ctx: &EnrichmentContext) -> Record {
    if record.kind != "device" {
        return record;
    }
    let parent = record
        .get("blueprint_id")
        .and_then(Value::as_str)
        .and_then(|bid| ctx.blueprints_by_id.get(bid));
    let attached = match parent {
        Some(b) => blueprint_summary(b),
        None => Value::Null,
    };
    record.fields.insert("blueprint".to_string(), attached);
    record
}

fn apply_severity_rollup(mut record: Record) -> Record {
    let value = match record.get("severity").and_then(Value::as_str) {
        Some(s) => Value::String(s.to_string()),
        None => Value::Null,
    };
    record.fields.insert("severity_rollup".to_string(), value);
    record
}

fn apply_device_platform(mut record: Record) -> Record {
    if let Some(platform) = record.get("platform").and_then(Value::as_str) {
        record
            .fields
            .insert("_platform".to_string(), Value::String(platform.to_string()));
    }
    record
}

/// Reduce a blueprint record to the summary attached by
/// `blueprint-context`. Trims to the small set of fields downstream
/// filters and emit templates actually use; keeps the enriched stream
/// compact.
fn blueprint_summary(b: &Record) -> Value {
    let mut out = serde_json::Map::new();
    for field in ["id", "name", "enrollment_code", "computers_count"] {
        if let Some(v) = b.get(field) {
            out.insert(field.into(), v.clone());
        }
    }
    Value::Object(out)
}

/// Severity ordering for downstream rank predicates:
/// critical > high > medium > low > info. Unknown values rank lower
/// than any known value.
pub fn severity_rank(s: &str) -> Option<u8> {
    match s {
        "critical" => Some(4),
        "high" => Some(3),
        "medium" => Some(2),
        "low" => Some(1),
        "info" => Some(0),
        _ => None,
    }
}

/// Helper for tests and CLI: ergonomic constructor for a synthetic
/// blueprint record (used by tests + recipe demos).
#[doc(hidden)]
pub fn synthetic_blueprint(id: &str, name: &str) -> Record {
    Record::wrap(
        "blueprint",
        crate::stream::SourceRef {
            operation_id: "synthetic".into(),
            response_index: 0,
            fetched_at: chrono::Utc::now(),
            trace_ref: None,
        },
        json!({
            "id": id,
            "name": name,
            "enrollment_code": {"is_active": true},
            "computers_count": 0,
        }),
    )
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;

    use super::*;
    use crate::stream::SourceRef;

    fn device(id: &str, blueprint_id: Option<&str>, platform: &str) -> Record {
        let mut body = json!({"device_id": id, "platform": platform});
        if let Some(b) = blueprint_id {
            body["blueprint_id"] = json!(b);
        }
        Record::wrap(
            "device",
            SourceRef {
                operation_id: "op".into(),
                response_index: 0,
                fetched_at: Utc::now(),
                trace_ref: None,
            },
            body,
        )
    }

    fn vulnerability(cve: &str, severity: Option<&str>) -> Record {
        let mut body = json!({"cve_id": cve});
        if let Some(s) = severity {
            body["severity"] = json!(s);
        }
        Record::wrap(
            "vulnerability",
            SourceRef {
                operation_id: "op".into(),
                response_index: 0,
                fetched_at: Utc::now(),
                trace_ref: None,
            },
            body,
        )
    }

    #[test]
    fn recipe_parse_round_trip() {
        for r in [
            Recipe::BlueprintContext,
            Recipe::SeverityRollUp,
            Recipe::DevicePlatform,
        ] {
            assert_eq!(Recipe::parse(r.as_str()), Some(r));
        }
        assert_eq!(Recipe::parse("nope"), None);
    }

    #[test]
    fn blueprint_context_attaches_parent_to_device() {
        let ctx = EnrichmentContext::with_blueprints([synthetic_blueprint("bp_1", "Mac Fleet")]);
        let d = device("dev_1", Some("bp_1"), "Mac");
        let enriched = apply(Recipe::BlueprintContext, d, &ctx);
        let bp = enriched.get("blueprint").expect("blueprint attached");
        assert_eq!(bp.get("id").and_then(Value::as_str), Some("bp_1"));
        assert_eq!(bp.get("name").and_then(Value::as_str), Some("Mac Fleet"));
    }

    #[test]
    fn blueprint_context_marks_orphan_device_with_null() {
        let ctx = EnrichmentContext::with_blueprints([synthetic_blueprint("bp_1", "Mac Fleet")]);
        let d = device("dev_orphan", Some("bp_999"), "Mac");
        let enriched = apply(Recipe::BlueprintContext, d, &ctx);
        assert_eq!(enriched.get("blueprint"), Some(&Value::Null));
    }

    #[test]
    fn blueprint_context_passes_through_non_devices() {
        let ctx = EnrichmentContext::with_blueprints([synthetic_blueprint("bp_1", "Mac Fleet")]);
        let v = vulnerability("CVE-2026-0001", Some("high"));
        let enriched = apply(Recipe::BlueprintContext, v, &ctx);
        assert!(enriched.get("blueprint").is_none());
    }

    #[test]
    fn severity_rollup_copies_own_severity() {
        let v = vulnerability("CVE-2026-0001", Some("high"));
        let enriched = apply(Recipe::SeverityRollUp, v, &EnrichmentContext::default());
        assert_eq!(
            enriched.get("severity_rollup").and_then(Value::as_str),
            Some("high")
        );
    }

    #[test]
    fn severity_rollup_handles_missing_severity() {
        let d = device("dev_1", None, "Mac");
        let enriched = apply(Recipe::SeverityRollUp, d, &EnrichmentContext::default());
        assert_eq!(enriched.get("severity_rollup"), Some(&Value::Null));
    }

    #[test]
    fn device_platform_hoists_top_level_field() {
        let d = device("dev_1", None, "Mac");
        let enriched = apply(Recipe::DevicePlatform, d, &EnrichmentContext::default());
        assert_eq!(
            enriched.get("_platform").and_then(Value::as_str),
            Some("Mac")
        );
    }

    #[test]
    fn device_platform_passes_through_records_without_platform() {
        let v = vulnerability("CVE-2026-0001", None);
        let enriched = apply(Recipe::DevicePlatform, v, &EnrichmentContext::default());
        assert!(enriched.get("_platform").is_none());
    }

    #[test]
    fn validate_blueprint_context_requires_blueprints() {
        let empty = EnrichmentContext::default();
        assert!(empty.validate_for(Recipe::BlueprintContext).is_err());
        let nonempty =
            EnrichmentContext::with_blueprints([synthetic_blueprint("bp_1", "Mac Fleet")]);
        assert!(nonempty.validate_for(Recipe::BlueprintContext).is_ok());
    }

    #[test]
    fn severity_rank_orders_known_values() {
        assert!(severity_rank("critical") > severity_rank("high"));
        assert!(severity_rank("high") > severity_rank("medium"));
        assert!(severity_rank("medium") > severity_rank("low"));
        assert!(severity_rank("low") > severity_rank("info"));
        assert_eq!(severity_rank("bogus"), None);
    }
}
