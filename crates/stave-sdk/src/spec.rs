//! Operation registry built from the vendored OpenAPI spec.
//!
//! The spec is embedded at compile time via `include_str!`. Parsing
//! happens once on first access, cached in `OnceLock`.
//!
//! The iru API's server URLs are *templated* —
//! `https://{subdomain}.api.kandji.io` (US) and
//! `https://{subdomain}.api.eu.kandji.io` (EU) — so the registry
//! exposes [`Registry::base_url_for`] rather than a single static
//! base URL. The subdomain resolves through the auth chain
//! (see `auth::resolve_subdomain`).

use std::collections::HashMap;
use std::sync::OnceLock;

use openapiv3::{OpenAPI, Operation, Parameter, PathItem, ReferenceOr, RequestBody};

use crate::error::{StaveError, Result};

const SPEC_JSON: &str = include_str!("../../../spec/iru-endpoint-openapi.json");

/// Fallback US server template if the vendored spec somehow carries no
/// servers block.
const DEFAULT_US_TEMPLATE: &str = "https://{subdomain}.api.kandji.io";

pub fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(|| {
        Registry::load().unwrap_or_else(|e| panic!("vendored spec failed to parse: {e}"))
    })
}

#[derive(Debug)]
pub struct Registry {
    /// Server URL templates keyed by region (`us`, `eu`). Each contains
    /// a `{subdomain}` placeholder.
    server_templates: HashMap<String, String>,
    ops: HashMap<String, OperationMeta>,
}

#[derive(Clone, Debug)]
pub struct OperationMeta {
    pub id: String,
    pub method: HttpMethod,
    pub path_template: String,
    /// Path parameter names (substituted into `{name}` placeholders).
    pub path_params: Vec<String>,
    /// Query parameter names.
    pub query_params: Vec<String>,
    /// Required path + query parameter names. Used to validate input.
    pub required_params: Vec<String>,
    /// True if the operation accepts a request body (POST/PUT/PATCH/DELETE
    /// with `requestBody` declared).
    pub has_body: bool,
    /// Brief one-line description from the spec, when available.
    pub summary: Option<String>,
}

impl OperationMeta {
    /// True when the operation mutates tenant state (anything that is
    /// not a plain GET). The write-guard in `Client::call_op` keys off
    /// this.
    pub fn is_mutating(&self) -> bool {
        self.method != HttpMethod::Get
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Put,
    Post,
    Delete,
    Options,
    Head,
    Patch,
    Trace,
}

impl HttpMethod {
    pub fn as_reqwest(&self) -> reqwest::Method {
        match self {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Delete => reqwest::Method::DELETE,
            HttpMethod::Options => reqwest::Method::OPTIONS,
            HttpMethod::Head => reqwest::Method::HEAD,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Trace => reqwest::Method::TRACE,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Put => "PUT",
            HttpMethod::Post => "POST",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Head => "HEAD",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Trace => "TRACE",
        }
    }
}

impl Registry {
    fn load() -> Result<Self> {
        let spec: OpenAPI = serde_json::from_str(SPEC_JSON)
            .map_err(|e| StaveError::Spec(format!("json parse: {e}")))?;

        let mut server_templates = HashMap::new();
        for server in &spec.servers {
            let url = server.url.trim_end_matches('/').to_string();
            // Classify by hostname: the EU server embeds `.eu.`; the
            // first non-EU entry is `us`.
            let region = if url.contains(".eu.") { "eu" } else { "us" };
            server_templates.entry(region.to_string()).or_insert(url);
        }
        server_templates
            .entry("us".to_string())
            .or_insert_with(|| DEFAULT_US_TEMPLATE.to_string());

        let mut ops = HashMap::new();
        for (path, item_ref) in spec.paths.paths.iter() {
            let ReferenceOr::Item(item) = item_ref else {
                continue;
            };
            for (method, op) in operations(item) {
                let Some(id) = op.operation_id.clone() else {
                    continue;
                };
                let meta = build_meta(id.clone(), method, path.as_str(), op);
                ops.insert(id, meta);
            }
        }
        Ok(Self {
            server_templates,
            ops,
        })
    }

    /// Substitute the subdomain into the server template for `region`
    /// (`us` default; `eu` selects the EU server). Unknown regions
    /// error rather than silently falling back.
    pub fn base_url_for(&self, subdomain: &str, region: Option<&str>) -> Result<String> {
        let region = region.unwrap_or("us");
        let template = self.server_templates.get(region).ok_or_else(|| {
            StaveError::Spec(format!("unknown region '{region}' — known regions: {}", {
                let mut regions: Vec<&str> =
                    self.server_templates.keys().map(String::as_str).collect();
                regions.sort_unstable();
                regions.join(", ")
            }))
        })?;
        Ok(template.replace("{subdomain}", subdomain))
    }

    /// The raw server URL template for a region, if known.
    pub fn server_template(&self, region: &str) -> Option<&str> {
        self.server_templates.get(region).map(String::as_str)
    }

    pub fn find(&self, id: &str) -> Result<&OperationMeta> {
        self.ops
            .get(id)
            .ok_or_else(|| StaveError::UnknownOperation(id.to_string()))
    }

    pub fn iter(&self) -> impl Iterator<Item = &OperationMeta> {
        self.ops.values()
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

fn operations(item: &PathItem) -> Vec<(HttpMethod, &Operation)> {
    let mut out: Vec<(HttpMethod, &Operation)> = Vec::new();
    if let Some(op) = &item.get {
        out.push((HttpMethod::Get, op));
    }
    if let Some(op) = &item.put {
        out.push((HttpMethod::Put, op));
    }
    if let Some(op) = &item.post {
        out.push((HttpMethod::Post, op));
    }
    if let Some(op) = &item.delete {
        out.push((HttpMethod::Delete, op));
    }
    if let Some(op) = &item.options {
        out.push((HttpMethod::Options, op));
    }
    if let Some(op) = &item.head {
        out.push((HttpMethod::Head, op));
    }
    if let Some(op) = &item.patch {
        out.push((HttpMethod::Patch, op));
    }
    if let Some(op) = &item.trace {
        out.push((HttpMethod::Trace, op));
    }
    out
}

fn build_meta(id: String, method: HttpMethod, path: &str, op: &Operation) -> OperationMeta {
    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut required_params = Vec::new();

    for p in &op.parameters {
        let ReferenceOr::Item(p) = p else { continue };
        match p {
            Parameter::Path { parameter_data, .. } => {
                path_params.push(parameter_data.name.clone());
                if parameter_data.required {
                    required_params.push(parameter_data.name.clone());
                }
            }
            Parameter::Query { parameter_data, .. } => {
                query_params.push(parameter_data.name.clone());
                if parameter_data.required {
                    required_params.push(parameter_data.name.clone());
                }
            }
            // Header / Cookie parameters intentionally not surfaced in v0.1.
            _ => {}
        }
    }

    let has_body = match &op.request_body {
        Some(ReferenceOr::Item(RequestBody { content, .. })) => !content.is_empty(),
        Some(ReferenceOr::Reference { .. }) => true,
        None => false,
    };

    OperationMeta {
        id,
        method,
        path_template: path.to_string(),
        path_params,
        query_params,
        required_params,
        has_body,
        summary: op.summary.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_with_expected_op_count() {
        let r = registry();
        // Vendored spec has 121 operations, all with synthesized
        // operationIds (`cargo xtask sync-spec` fills them). We assert
        // >= 110 so this test doesn't break on minor upstream removals.
        assert!(r.len() >= 110, "expected >= 110 ops, got {}", r.len());
    }

    #[test]
    fn registry_finds_a_well_known_op() {
        let r = registry();
        let op = r.find("get_devices").expect("get_devices exists");
        assert_eq!(op.method, HttpMethod::Get);
        assert!(op.path_template.contains("/devices"));
        assert!(op.query_params.contains(&"limit".to_string()));
    }

    #[test]
    fn registry_finds_get_device_by_id() {
        let r = registry();
        let op = r
            .find("get_devices_device_id")
            .expect("get_devices_device_id exists");
        assert!(op.path_params.contains(&"device_id".to_string()));
    }

    #[test]
    fn mutating_classification_follows_method() {
        let r = registry();
        let read = r.find("get_devices").expect("get_devices");
        assert!(!read.is_mutating());
        // Every non-GET op is mutating; find one from the spec.
        let mutating = r.iter().find(|op| op.method != HttpMethod::Get);
        if let Some(op) = mutating {
            assert!(op.is_mutating(), "{} should be mutating", op.id);
        }
    }

    #[test]
    fn base_url_substitutes_subdomain() {
        let r = registry();
        let url = r.base_url_for("accuhive", None).expect("us url");
        assert_eq!(url, "https://accuhive.api.kandji.io");
        let eu = r.base_url_for("accuhive", Some("eu")).expect("eu url");
        assert!(eu.contains(".eu."), "eu url should differ: {eu}");
    }

    #[test]
    fn base_url_unknown_region_errors() {
        let r = registry();
        assert!(r.base_url_for("accuhive", Some("mars")).is_err());
    }

    #[test]
    fn registry_unknown_op_errors() {
        let r = registry();
        assert!(r.find("definitelyNotARealOp").is_err());
    }
}
