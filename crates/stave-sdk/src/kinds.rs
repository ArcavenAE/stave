//! The `_kind` types for stave v0.1 — iru Endpoint Management nouns.
//!
//! Each kind binds:
//!   * a stable name (`device`, `blueprint`, …) — appears in `_kind`
//!   * a list operation (`operationId`) for `stave list <kind>`
//!   * an optional get-by-id operation for `stave get <kind> <id>`
//!   * the field that holds the primary key
//!   * the response-extraction strategy — iru endpoints return either
//!     bare arrays (`GET /devices`) or DRF-style
//!     `{count, next, previous, results}` wrappers
//!
//! Field metadata (id/severity/timestamp/search fields) is drawn from
//! the iru API documentation and is **provisional until live-validated**
//! — the scaffold-time token had no API permissions (bd aae-orc-b6dg.11),
//! so per-kind field names could not be confirmed against real payloads.
//! Wrong guesses degrade gracefully: `emit --format md` shows blanks and
//! `--since` errors name the missing field.

use serde_json::Value;

/// Stable v0.1 kind names.
pub const KIND_DEVICE: &str = "device";
pub const KIND_BLUEPRINT: &str = "blueprint";
pub const KIND_USER: &str = "user";
pub const KIND_TAG: &str = "tag";
pub const KIND_AUDIT_EVENT: &str = "audit_event";
pub const KIND_THREAT: &str = "threat";
pub const KIND_BEHAVIORAL_DETECTION: &str = "behavioral_detection";
pub const KIND_VULNERABILITY: &str = "vulnerability";
pub const KIND_CUSTOM_APP: &str = "custom_app";
pub const KIND_CUSTOM_PROFILE: &str = "custom_profile";
pub const KIND_CUSTOM_SCRIPT: &str = "custom_script";
pub const KIND_ADE_DEVICE: &str = "ade_device";

/// All v0.1 kind names, in stable order.
pub const ALL_KINDS: &[&str] = &[
    KIND_DEVICE,
    KIND_BLUEPRINT,
    KIND_USER,
    KIND_TAG,
    KIND_AUDIT_EVENT,
    KIND_THREAT,
    KIND_BEHAVIORAL_DETECTION,
    KIND_VULNERABILITY,
    KIND_CUSTOM_APP,
    KIND_CUSTOM_PROFILE,
    KIND_CUSTOM_SCRIPT,
    KIND_ADE_DEVICE,
];

/// Static metadata for one `_kind`.
#[derive(Debug, Clone)]
pub struct KindSpec {
    /// Stable name in the stream contract.
    pub name: &'static str,

    /// `operationId` to call for `stave list <kind>`.
    pub list_operation_id: Option<&'static str>,

    /// `operationId` for `stave get <kind> <id>`, when the spec
    /// exposes one.
    pub get_operation_id: Option<&'static str>,

    /// Field name in each item that carries the stable primary key.
    pub id_field: &'static str,

    /// Field name in each item that carries severity, when present.
    pub severity_field: Option<&'static str>,

    /// Field name in each item that carries the canonical timestamp
    /// (used by `--since` and the canonical adapter's `now` binding
    /// comparisons).
    pub primary_timestamp_field: Option<&'static str>,

    /// Spec path-parameter name that the kind's `id` binds to in
    /// `stave get <kind> <id>`. `None` when the kind has no
    /// get-by-id endpoint.
    pub id_path_param: Option<&'static str>,

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

/// The static kind → operation table. Operation IDs are the
/// synthesized ids from the vendored spec (`{method}_{path}` with the
/// `/api/v1` prefix stripped — see xtask's `synthesize_operation_id`).
const KIND_TABLE: &[KindSpec] = &[
    KindSpec {
        name: KIND_DEVICE,
        list_operation_id: Some("get_devices"),
        get_operation_id: Some("get_devices_device_id"),
        id_field: "device_id",
        severity_field: None,
        primary_timestamp_field: Some("last_check_in"),
        id_path_param: Some("device_id"),
        search_field: Some("device_name"),
    },
    KindSpec {
        name: KIND_BLUEPRINT,
        list_operation_id: Some("get_blueprints"),
        get_operation_id: Some("get_blueprints_blueprint_id"),
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        id_path_param: Some("blueprint_id"),
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_USER,
        list_operation_id: Some("get_users"),
        get_operation_id: Some("get_users_user_id"),
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        id_path_param: Some("user_id"),
        search_field: Some("email"),
    },
    KindSpec {
        name: KIND_TAG,
        list_operation_id: Some("get_tags"),
        get_operation_id: None,
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        id_path_param: None,
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_AUDIT_EVENT,
        list_operation_id: Some("get_audit_events"),
        get_operation_id: None,
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: Some("occurred_at"),
        id_path_param: None,
        search_field: Some("action"),
    },
    KindSpec {
        name: KIND_THREAT,
        list_operation_id: Some("get_threat_details"),
        get_operation_id: None,
        id_field: "threat_id",
        severity_field: None,
        primary_timestamp_field: Some("detection_date"),
        id_path_param: None,
        search_field: Some("threat_name"),
    },
    KindSpec {
        name: KIND_BEHAVIORAL_DETECTION,
        list_operation_id: Some("get_behavioral_detections"),
        get_operation_id: None,
        id_field: "detection_id",
        severity_field: None,
        primary_timestamp_field: Some("detection_date"),
        id_path_param: None,
        search_field: Some("malware_family"),
    },
    KindSpec {
        name: KIND_VULNERABILITY,
        list_operation_id: Some("get_vulnerability_management_vulnerabilities"),
        get_operation_id: Some("get_vulnerability_management_vulnerabilities_cve_id"),
        id_field: "cve_id",
        severity_field: Some("severity"),
        primary_timestamp_field: Some("first_detection_date"),
        id_path_param: Some("cve_id"),
        search_field: Some("cve_id"),
    },
    KindSpec {
        name: KIND_CUSTOM_APP,
        list_operation_id: Some("get_library_custom_apps"),
        get_operation_id: Some("get_library_custom_apps_library_item_id"),
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        id_path_param: Some("library_item_id"),
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_CUSTOM_PROFILE,
        list_operation_id: Some("get_library_custom_profiles"),
        get_operation_id: Some("get_library_custom_profiles_library_item_id"),
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        id_path_param: Some("library_item_id"),
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_CUSTOM_SCRIPT,
        list_operation_id: Some("get_library_custom_scripts"),
        get_operation_id: Some("get_library_custom_scripts_library_item_id"),
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        id_path_param: Some("library_item_id"),
        search_field: Some("name"),
    },
    KindSpec {
        name: KIND_ADE_DEVICE,
        list_operation_id: Some("get_integrations_apple_ade_devices"),
        get_operation_id: Some("get_integrations_apple_ade_devices_device_id"),
        id_field: "id",
        severity_field: None,
        primary_timestamp_field: None,
        id_path_param: Some("device_id"),
        search_field: Some("serial_number"),
    },
];

/// Extract the array of items from an API response body. Mirrors the
/// detection logic in `client::count_items` so the audit-emitted
/// `items_returned` matches what the primitive actually streams.
pub fn extract_items(response: &Value) -> Option<&[Value]> {
    if let Some(arr) = response.as_array() {
        return Some(arr);
    }
    for key in ["results", "items", "data", "devices", "detections"] {
        if let Some(arr) = response.get(key).and_then(|v| v.as_array()) {
            return Some(arr);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn twelve_kinds_in_table() {
        assert_eq!(KIND_TABLE.len(), 12);
        assert_eq!(ALL_KINDS.len(), 12);
        for (a, b) in KIND_TABLE.iter().zip(ALL_KINDS.iter()) {
            assert_eq!(&a.name, b);
        }
    }

    #[test]
    fn lookup_known_kind() {
        let k = kind_spec("device").expect("device in table");
        assert_eq!(k.list_operation_id, Some("get_devices"));
        assert_eq!(k.id_field, "device_id");
        assert_eq!(k.search_field, Some("device_name"));
    }

    #[test]
    fn device_kind_has_device_id_path_param() {
        let k = kind_spec("device").expect("device in table");
        assert_eq!(k.id_path_param, Some("device_id"));
    }

    #[test]
    fn vulnerability_kind_carries_severity() {
        let k = kind_spec("vulnerability").expect("vulnerability");
        assert_eq!(k.severity_field, Some("severity"));
        assert_eq!(k.id_path_param, Some("cve_id"));
    }

    #[test]
    fn every_list_operation_exists_in_registry() {
        // The kind table references synthesized operationIds; keep it
        // honest against the vendored spec.
        let r = crate::spec::registry();
        for k in KIND_TABLE {
            if let Some(op) = k.list_operation_id {
                assert!(r.find(op).is_ok(), "kind {} list op {op} missing", k.name);
            }
            if let Some(op) = k.get_operation_id {
                assert!(r.find(op).is_ok(), "kind {} get op {op} missing", k.name);
            }
        }
    }

    #[test]
    fn lookup_unknown_kind() {
        assert!(kind_spec("nope").is_none());
    }

    #[test]
    fn extract_items_handles_bare_array() {
        let body = json!([{"device_id": "a"}, {"device_id": "b"}]);
        assert_eq!(extract_items(&body).unwrap().len(), 2);
    }

    #[test]
    fn extract_items_handles_drf_wrapper() {
        let body = json!({"count": 1, "next": null, "previous": null,
            "results": [{"id": "a"}]});
        assert_eq!(extract_items(&body).unwrap().len(), 1);
    }

    #[test]
    fn extract_items_returns_none_when_no_array() {
        let body = json!({"device_id": "single", "platform": "Mac"});
        assert!(extract_items(&body).is_none());
    }
}
