//! xtask: developer commands for the stave workspace.
//!
//! Run via `cargo xtask <command>`. The companion `[alias] xtask = "run -p xtask --"`
//! lives in `.cargo/config.toml`.
//!
//! Commands:
//!   * `sync-spec`: introspect the tenant GraphQL API, write `spec/wiz-schema.graphql` + sha256 pin
//!   * `check-ops`: validate `crates/stave-api/ops/*.graphql` against the vendored schema
//!   * `diff-spec`: stub, parity with the sibling repos
//!
//! Two design notes.
//!
//! **Introspection to SDL is printed here, not vendored as JSON.** The
//! contract the rest of the workspace reads is SDL: `check-ops` parses
//! it with `graphql-parser`, humans diff it in review, and the sha256
//! pin is over the SDL bytes. So `sync-spec` converts the introspection
//! response into SDL itself (see the `introspection` module) rather than
//! storing the raw JSON. The printer is deterministic: types, fields,
//! arguments, enum values, interfaces, and union members are all sorted
//! by name, so an unrelated server-side reordering cannot move the pin.
//! Descriptions are dropped (structure is the contract; vendor prose
//! belongs in vendor docs) and `@deprecated` markers are kept, because
//! selecting a deprecated field is drift worth a warning.
//!
//! **Credentials come from the SDK, not from a private copy.**
//! `sync-spec` builds a `stave_sdk::Client`, so it walks exactly the
//! chains the CLI walks (client ID: env then config; secret: env then
//! keyring then config; endpoint: env then config then derived from the
//! minted token's data-center claim) and the request lands in the same
//! audit trail. No env-reading logic is duplicated here.

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use regex::Regex;
use sha2::{Digest, Sha256};

const SCHEMA_REL_PATH: &str = "spec/wiz-schema.graphql";
const SCHEMA_SHA_REL_PATH: &str = "spec/wiz-schema.graphql.sha256";
const SCHEMA_FILE_NAME: &str = "wiz-schema.graphql";
const OPS_REL_DIR: &str = "crates/stave-api/ops";
const REGISTRY_REL_PATH: &str = "crates/stave-api/src/lib.rs";

/// Scalars every GraphQL service provides. Schema printers omit them
/// from SDL, so the checker seeds them as known leaf types.
const BUILT_IN_SCALARS: &[&str] = &["String", "Int", "Float", "Boolean", "ID"];

#[derive(Parser, Debug)]
#[command(name = "xtask", about = "Developer tasks for stave")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Introspect the tenant API and refresh the vendored GraphQL schema.
    SyncSpec,
    /// Validate the curated operation documents against the vendored schema.
    CheckOps,
    /// Diff the vendored schema against the live one. Not yet implemented.
    DiffSpec,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::SyncSpec => sync_spec(),
        Cmd::CheckOps => check_ops(),
        Cmd::DiffSpec => {
            eprintln!("xtask diff-spec: not yet implemented");
            Ok(())
        }
    }
}

// ─── sync-spec ─────────────────────────────────────────────────────

fn sync_spec() -> Result<()> {
    let workspace_root = workspace_root()?;
    let schema_path = workspace_root.join(SCHEMA_REL_PATH);
    let sha_path = workspace_root.join(SCHEMA_SHA_REL_PATH);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    let response = runtime.block_on(fetch_introspection())?;

    let schema_value = response
        .get("__schema")
        .ok_or_else(|| anyhow!("introspection response has no `__schema` key"))?;
    let schema: introspection::Schema =
        serde_json::from_value(schema_value.clone()).context("parse introspection response")?;
    let sdl = introspection::to_sdl(&schema)?;

    fs::create_dir_all(schema_path.parent().context("spec/ parent")?)?;
    fs::write(&schema_path, sdl.as_bytes())
        .with_context(|| format!("write {}", schema_path.display()))?;

    let mut hasher = Sha256::new();
    hasher.update(sdl.as_bytes());
    let digest = hex::encode(hasher.finalize());
    fs::write(&sha_path, format!("{digest}  {SCHEMA_FILE_NAME}\n"))
        .with_context(|| format!("write {}", sha_path.display()))?;

    eprintln!(
        "xtask: wrote {} ({} bytes, {} type(s))",
        schema_path.display(),
        sdl.len(),
        schema.types.len()
    );
    eprintln!("xtask: sha256 {digest}");
    eprintln!("xtask: next step is `cargo xtask check-ops`");
    Ok(())
}

/// The standard GraphQL introspection query, minus descriptions.
///
/// `TypeRef` nests seven deep, which covers every wrapper stack a real
/// schema uses (`[[Type!]!]!` and then some). Deeper nesting would be
/// reported as a named type without a name and fail loudly.
const INTROSPECTION_QUERY: &str = r#"
query StaveIntrospection {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types { ...FullType }
  }
}

fragment FullType on __Type {
  kind
  name
  fields(includeDeprecated: true) {
    name
    args { ...InputValue }
    type { ...TypeRef }
    isDeprecated
    deprecationReason
  }
  inputFields { ...InputValue }
  interfaces { ...TypeRef }
  enumValues(includeDeprecated: true) {
    name
    isDeprecated
    deprecationReason
  }
  possibleTypes { ...TypeRef }
}

fragment InputValue on __InputValue {
  name
  type { ...TypeRef }
  defaultValue
}

fragment TypeRef on __Type {
  kind
  name
  ofType {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
            ofType { kind name }
          }
        }
      }
    }
  }
}
"#;

/// Mint a token through the SDK's chains and run the introspection
/// query against the tenant endpoint. Credential and endpoint failures
/// surface with the SDK's own chain-naming messages, which already name
/// every layer and a concrete next step for each.
async fn fetch_introspection() -> Result<serde_json::Value> {
    let client = stave_sdk::Client::from_env().await.map_err(|e| {
        anyhow!(
            "{e}\n\nsync-spec needs a Wiz service account: it introspects the live \
             tenant schema. This is expected to fail until one exists."
        )
    })?;
    eprintln!("xtask: introspecting the tenant GraphQL API");
    let opts = stave_sdk::CallOptions {
        verb_phase: Some("xtask"),
        ..Default::default()
    };
    client
        .call_document(INTROSPECTION_QUERY, &serde_json::json!({}), opts)
        .await
        .map_err(|e| anyhow!("introspection failed: {e}"))
}

/// Introspection response model plus the SDL printer.
mod introspection {
    use anyhow::{Context, Result, anyhow};
    use serde::Deserialize;

    use crate::BUILT_IN_SCALARS;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Schema {
        #[serde(default)]
        pub query_type: Option<NamedRef>,
        #[serde(default)]
        pub mutation_type: Option<NamedRef>,
        #[serde(default)]
        pub subscription_type: Option<NamedRef>,
        #[serde(default)]
        pub types: Vec<FullType>,
    }

    #[derive(Debug, Deserialize)]
    pub struct NamedRef {
        #[serde(default)]
        pub name: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct FullType {
        pub kind: String,
        #[serde(default)]
        pub name: Option<String>,
        #[serde(default)]
        pub fields: Option<Vec<Field>>,
        #[serde(default)]
        pub input_fields: Option<Vec<InputValue>>,
        #[serde(default)]
        pub interfaces: Option<Vec<TypeRef>>,
        #[serde(default)]
        pub enum_values: Option<Vec<EnumValue>>,
        #[serde(default)]
        pub possible_types: Option<Vec<TypeRef>>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Field {
        pub name: String,
        #[serde(default)]
        pub args: Vec<InputValue>,
        #[serde(rename = "type")]
        pub field_type: TypeRef,
        #[serde(default)]
        pub is_deprecated: bool,
        #[serde(default)]
        pub deprecation_reason: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct InputValue {
        pub name: String,
        #[serde(rename = "type")]
        pub field_type: TypeRef,
        #[serde(default)]
        pub default_value: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct EnumValue {
        pub name: String,
        #[serde(default)]
        pub is_deprecated: bool,
        #[serde(default)]
        pub deprecation_reason: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct TypeRef {
        pub kind: String,
        #[serde(default)]
        pub name: Option<String>,
        #[serde(default)]
        pub of_type: Option<Box<TypeRef>>,
    }

    const HEADER: &str = "\
# Wiz GraphQL schema (SDL), vendored by `cargo xtask sync-spec`.
#
# Printed from a live introspection response, deterministically: types,
# fields, arguments, enum values, interfaces, and union members are all
# sorted by name, so a server-side reordering cannot move the sha256
# pin. Descriptions are dropped; `@deprecated` markers are kept.
#
# No tenant identity is recorded here. The schema is type structure
# only, and the endpoint it came from stays in local config.
#
# DO NOT EDIT BY HAND. Refresh with `cargo xtask sync-spec`, then
# validate the curated operations with `cargo xtask check-ops`.

";

    /// Print a whole introspection result as SDL.
    pub fn to_sdl(schema: &Schema) -> Result<String> {
        let mut out = String::with_capacity(HEADER.len() + 64 * schema.types.len());
        out.push_str(HEADER);

        out.push_str(&root_block(schema));

        let mut printable: Vec<&FullType> = schema
            .types
            .iter()
            .filter(|t| match t.name.as_deref() {
                Some(name) => !name.starts_with("__") && !BUILT_IN_SCALARS.contains(&name),
                None => false,
            })
            .collect();
        printable.sort_by(|a, b| a.name.cmp(&b.name));

        for ty in printable {
            out.push_str(&type_block(ty)?);
            out.push('\n');
        }
        Ok(out)
    }

    fn root_block(schema: &Schema) -> String {
        let mut lines = Vec::new();
        for (label, root) in [
            ("query", &schema.query_type),
            ("mutation", &schema.mutation_type),
            ("subscription", &schema.subscription_type),
        ] {
            if let Some(name) = root.as_ref().and_then(|r| r.name.as_deref()) {
                lines.push(format!("  {label}: {name}\n"));
            }
        }
        if lines.is_empty() {
            return String::new();
        }
        format!("schema {{\n{}}}\n\n", lines.concat())
    }

    fn type_block(ty: &FullType) -> Result<String> {
        let name = ty
            .name
            .as_deref()
            .ok_or_else(|| anyhow!("type without a name"))?;
        match ty.kind.as_str() {
            "SCALAR" => Ok(format!("scalar {name}\n")),
            "OBJECT" => Ok(sdl_block(
                "type",
                name,
                &implements_clause(ty),
                &field_lines(ty, name)?,
            )),
            "INTERFACE" => Ok(sdl_block(
                "interface",
                name,
                &implements_clause(ty),
                &field_lines(ty, name)?,
            )),
            "UNION" => {
                let mut members: Vec<&str> = ty
                    .possible_types
                    .iter()
                    .flatten()
                    .filter_map(|t| t.name.as_deref())
                    .collect();
                members.sort_unstable();
                if members.is_empty() {
                    // A union with no members cannot be expressed in SDL.
                    return Err(anyhow!("union {name} has no possible types"));
                }
                Ok(format!("union {name} = {}\n", members.join(" | ")))
            }
            "ENUM" => {
                let mut values: Vec<&EnumValue> = ty.enum_values.iter().flatten().collect();
                values.sort_by(|a, b| a.name.cmp(&b.name));
                let body: String = values
                    .iter()
                    .map(|v| {
                        format!(
                            "  {}{}\n",
                            v.name,
                            deprecated_suffix(v.is_deprecated, v.deprecation_reason.as_deref())
                        )
                    })
                    .collect();
                Ok(sdl_block("enum", name, "", &body))
            }
            "INPUT_OBJECT" => {
                let mut fields: Vec<&InputValue> = ty.input_fields.iter().flatten().collect();
                fields.sort_by(|a, b| a.name.cmp(&b.name));
                let mut body = String::new();
                for f in fields {
                    body.push_str(&format!(
                        "  {}\n",
                        input_value(f).with_context(|| format!("input {name}.{}", f.name))?
                    ));
                }
                Ok(sdl_block("input", name, "", &body))
            }
            other => Err(anyhow!("type {name} has unknown kind {other}")),
        }
    }

    /// Render a type definition, omitting the `{ }` block when the body
    /// is empty. GraphQL's grammar makes the fields/values block
    /// optional, and an empty `{}` is a parse error (a fields block
    /// requires at least one member), so a bodyless definition is the
    /// only valid SDL for a type introspection reports with no members.
    /// Wiz's schema has a couple (e.g. empty input objects).
    fn sdl_block(keyword: &str, name: &str, suffix: &str, body: &str) -> String {
        if body.is_empty() {
            format!("{keyword} {name}{suffix}\n")
        } else {
            format!("{keyword} {name}{suffix} {{\n{body}}}\n")
        }
    }

    fn implements_clause(ty: &FullType) -> String {
        let mut names: Vec<&str> = ty
            .interfaces
            .iter()
            .flatten()
            .filter_map(|t| t.name.as_deref())
            .collect();
        names.sort_unstable();
        if names.is_empty() {
            String::new()
        } else {
            format!(" implements {}", names.join(" & "))
        }
    }

    fn field_lines(ty: &FullType, owner: &str) -> Result<String> {
        let mut fields: Vec<&Field> = ty.fields.iter().flatten().collect();
        fields.sort_by(|a, b| a.name.cmp(&b.name));
        let mut out = String::new();
        for f in fields {
            let mut args: Vec<&InputValue> = f.args.iter().collect();
            args.sort_by(|a, b| a.name.cmp(&b.name));
            let rendered: Result<Vec<String>> = args
                .iter()
                .map(|a| {
                    input_value(a)
                        .with_context(|| format!("argument {owner}.{}({})", f.name, a.name))
                })
                .collect();
            let rendered = rendered?;
            let arg_list = if rendered.is_empty() {
                String::new()
            } else {
                format!("({})", rendered.join(", "))
            };
            out.push_str(&format!(
                "  {}{}: {}{}\n",
                f.name,
                arg_list,
                render_type_ref(&f.field_type)
                    .with_context(|| format!("field {owner}.{}", f.name))?,
                deprecated_suffix(f.is_deprecated, f.deprecation_reason.as_deref())
            ));
        }
        Ok(out)
    }

    fn input_value(v: &InputValue) -> Result<String> {
        let ty = render_type_ref(&v.field_type)?;
        // `defaultValue` arrives as a GraphQL literal already, so it is
        // emitted verbatim rather than re-quoted.
        match v
            .default_value
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(default) => Ok(format!("{}: {ty} = {default}", v.name)),
            None => Ok(format!("{}: {ty}", v.name)),
        }
    }

    fn deprecated_suffix(is_deprecated: bool, reason: Option<&str>) -> String {
        if !is_deprecated {
            return String::new();
        }
        match reason.map(str::trim).filter(|s| !s.is_empty()) {
            Some(r) => format!(" @deprecated(reason: {})", graphql_string(r)),
            None => " @deprecated".to_string(),
        }
    }

    /// Render a wrapper stack (`NON_NULL` / `LIST` around a named type).
    pub fn render_type_ref(ty: &TypeRef) -> Result<String> {
        match ty.kind.as_str() {
            "NON_NULL" => {
                let inner = ty
                    .of_type
                    .as_deref()
                    .ok_or_else(|| anyhow!("NON_NULL without ofType"))?;
                Ok(format!("{}!", render_type_ref(inner)?))
            }
            "LIST" => {
                let inner = ty
                    .of_type
                    .as_deref()
                    .ok_or_else(|| anyhow!("LIST without ofType"))?;
                Ok(format!("[{}]", render_type_ref(inner)?))
            }
            _ => ty
                .name
                .clone()
                .ok_or_else(|| anyhow!("named type of kind {} without a name", ty.kind)),
        }
    }

    /// Quote a string as a GraphQL string literal.
    fn graphql_string(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for ch in s.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
}

// ─── check-ops ─────────────────────────────────────────────────────

fn check_ops() -> Result<()> {
    let workspace_root = workspace_root()?;
    let schema_path = workspace_root.join(SCHEMA_REL_PATH);

    if !schema_path.exists() {
        eprintln!(
            "xtask check-ops: WARNING schema not vendored yet ({} is absent); \
             check-ops is a no-op until sync-spec runs",
            schema_path.display()
        );
        eprintln!(
            "xtask check-ops: the curated documents in {OPS_REL_DIR} are UNVALIDATED \
             until a service account exists and `cargo xtask sync-spec` lands the schema"
        );
        return Ok(());
    }

    let schema_text = fs::read_to_string(&schema_path)
        .with_context(|| format!("read {}", schema_path.display()))?;
    let schema_doc = graphql_parser::parse_schema::<String>(&schema_text)
        .map_err(|e| anyhow!("parse {}: {e}", schema_path.display()))?;
    let index = SchemaIndex::build(&schema_doc)?;

    let registry_path = workspace_root.join(REGISTRY_REL_PATH);
    let registry_text = fs::read_to_string(&registry_path)
        .with_context(|| format!("read {}", registry_path.display()))?;
    let registry = parse_registry(&registry_text)?;

    let ops_dir = workspace_root.join(OPS_REL_DIR);
    let documents = list_documents(&ops_dir)?;
    if documents.is_empty() {
        bail!("no .graphql documents found in {}", ops_dir.display());
    }

    let mut problems: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for path in &documents {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("document {} has no usable stem", path.display()))?
            .to_string();
        let source =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let entry = registry.get(&name);
        if entry.is_none() {
            problems.push(format!(
                "{name}: document has no entry in {REGISTRY_REL_PATH} (OPERATIONS table)"
            ));
        }
        let outcome = check_document(&index, &name, &source, entry);
        problems.extend(outcome.problems);
        warnings.extend(outcome.warnings);
    }

    let on_disk: BTreeSet<String> = documents
        .iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
        .collect();
    for name in registry.keys() {
        if !on_disk.contains(name) {
            problems.push(format!(
                "{name}: registered in {REGISTRY_REL_PATH} but {OPS_REL_DIR}/{name}.graphql is missing"
            ));
        }
    }

    for w in &warnings {
        eprintln!("xtask check-ops: warning: {w}");
    }
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("xtask check-ops: error: {p}");
        }
        bail!(
            "{} problem(s) across {} document(s); fix the document or refresh the schema \
             with `cargo xtask sync-spec`",
            problems.len(),
            documents.len()
        );
    }

    eprintln!(
        "xtask check-ops: {} document(s) validated against {SCHEMA_REL_PATH}{}",
        documents.len(),
        if warnings.is_empty() {
            String::new()
        } else {
            format!(" ({} warning(s))", warnings.len())
        }
    );
    Ok(())
}

fn list_documents(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    let entries = fs::read_dir(dir).with_context(|| format!("read dir {}", dir.display()))?;
    for entry in entries {
        let path = entry
            .with_context(|| format!("read dir entry in {}", dir.display()))?
            .path();
        if path.extension().and_then(|e| e.to_str()) == Some("graphql") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// One row of the `OPERATIONS` table in `crates/stave-api/src/lib.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RegistryEntry {
    op_type: String,
    root_field: String,
}

/// Read the registry table out of the api crate's source.
///
/// The table is `const OPERATIONS: &[OperationDoc]`, whose fields are
/// declared in a fixed order (name, document, op_type, root_field), so
/// a regex over the source is enough and keeps xtask from having to
/// link the api crate just to read metadata. A shape change fails loudly
/// here rather than silently matching nothing.
fn parse_registry(source: &str) -> Result<BTreeMap<String, RegistryEntry>> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r#"(?s)name:\s*"(?P<name>[A-Za-z0-9_]+)"\s*,.*?op_type:\s*OpType::(?P<ty>Query|Mutation)\s*,\s*root_field:\s*"(?P<root>[^"]*)""#,
        )
        .expect("registry regex")
    });
    let mut out = BTreeMap::new();
    for caps in re.captures_iter(source) {
        let name = caps["name"].to_string();
        let entry = RegistryEntry {
            op_type: caps["ty"].to_ascii_lowercase(),
            root_field: caps["root"].to_string(),
        };
        if let Some(previous) = out.insert(name.clone(), entry) {
            bail!("registry lists {name} more than once (first: {previous:?})");
        }
    }
    if out.is_empty() {
        bail!(
            "parsed zero entries from {REGISTRY_REL_PATH}; the OPERATIONS table shape \
             changed and this parser needs updating"
        );
    }
    Ok(out)
}

/// Root type names plus a name-to-definition map over the vendored schema.
struct SchemaIndex<'a> {
    types: BTreeMap<&'a str, &'a graphql_parser::schema::TypeDefinition<'a, String>>,
    query_root: Option<String>,
    mutation_root: Option<String>,
}

impl<'a> SchemaIndex<'a> {
    fn build(doc: &'a graphql_parser::schema::Document<'a, String>) -> Result<Self> {
        use graphql_parser::schema::Definition;

        let mut types = BTreeMap::new();
        let mut declared_query: Option<String> = None;
        let mut declared_mutation: Option<String> = None;

        for def in &doc.definitions {
            match def {
                Definition::TypeDefinition(td) => {
                    types.insert(type_definition_name(td), td);
                }
                Definition::SchemaDefinition(sd) => {
                    declared_query = sd.query.clone();
                    declared_mutation = sd.mutation.clone();
                }
                _ => {}
            }
        }

        // A schema without a `schema { ... }` block uses the default
        // root names, which are only roots if the types exist.
        let query_root =
            declared_query.or_else(|| types.contains_key("Query").then(|| "Query".to_string()));
        let mutation_root = declared_mutation.or_else(|| {
            types
                .contains_key("Mutation")
                .then(|| "Mutation".to_string())
        });

        if query_root.is_none() {
            bail!("vendored schema declares no query root type");
        }
        Ok(Self {
            types,
            query_root,
            mutation_root,
        })
    }

    fn root_for(&self, op_type: &str) -> Option<&str> {
        match op_type {
            "mutation" => self.mutation_root.as_deref(),
            _ => self.query_root.as_deref(),
        }
    }

    fn get(&self, name: &str) -> Option<&'a graphql_parser::schema::TypeDefinition<'a, String>> {
        self.types.get(name).copied()
    }

    fn knows(&self, name: &str) -> bool {
        self.types.contains_key(name) || BUILT_IN_SCALARS.contains(&name)
    }
}

fn type_definition_name<'a>(td: &'a graphql_parser::schema::TypeDefinition<'a, String>) -> &'a str {
    use graphql_parser::schema::TypeDefinition as T;
    match td {
        T::Scalar(t) => &t.name,
        T::Object(t) => &t.name,
        T::Interface(t) => &t.name,
        T::Union(t) => &t.name,
        T::Enum(t) => &t.name,
        T::InputObject(t) => &t.name,
    }
}

fn composite_fields<'a>(
    td: &'a graphql_parser::schema::TypeDefinition<'a, String>,
) -> Option<&'a Vec<graphql_parser::schema::Field<'a, String>>> {
    use graphql_parser::schema::TypeDefinition as T;
    match td {
        T::Object(t) => Some(&t.fields),
        T::Interface(t) => Some(&t.fields),
        _ => None,
    }
}

/// Strip `!` and `[]` wrappers down to the named type.
fn named_type(ty: &graphql_parser::schema::Type<'_, String>) -> String {
    use graphql_parser::schema::Type as T;
    match ty {
        T::NamedType(n) => n.clone(),
        T::ListType(inner) => named_type(inner),
        T::NonNullType(inner) => named_type(inner),
    }
}

#[derive(Default)]
struct CheckOutcome {
    problems: Vec<String>,
    warnings: Vec<String>,
}

/// Validate one curated document against the schema.
///
/// Checks, in order: the document parses; it holds exactly one
/// operation; its operation type matches the registry; its variables
/// name types the schema knows; the registry's `root_field` is actually
/// selected; and every selected field exists on its parent type, walked
/// recursively. Unions, unresolvable fragment spreads, and types the
/// schema does not carry degrade to warnings so a partial schema cannot
/// produce a wall of false failures.
fn check_document(
    index: &SchemaIndex<'_>,
    label: &str,
    source: &str,
    entry: Option<&RegistryEntry>,
) -> CheckOutcome {
    use graphql_parser::query::{Definition, OperationDefinition};

    let mut outcome = CheckOutcome::default();

    let doc = match graphql_parser::parse_query::<String>(source) {
        Ok(d) => d,
        Err(e) => {
            outcome.problems.push(format!("{label}: parse error: {e}"));
            return outcome;
        }
    };

    let mut fragments = BTreeMap::new();
    let mut operations = Vec::new();
    for def in &doc.definitions {
        match def {
            Definition::Fragment(f) => {
                fragments.insert(f.name.clone(), f);
            }
            Definition::Operation(op) => operations.push(op),
        }
    }

    if operations.len() != 1 {
        outcome.problems.push(format!(
            "{label}: expected exactly one operation per document, found {}",
            operations.len()
        ));
        return outcome;
    }
    let operation = operations[0];

    // A bare selection set carries no variable definitions; this stands
    // in for the list the other arms borrow from the operation.
    let no_variables: Vec<graphql_parser::query::VariableDefinition<'_, String>> = Vec::new();
    let (doc_op_type, variables, selection_set) = match operation {
        OperationDefinition::Query(q) => ("query", &q.variable_definitions, &q.selection_set),
        OperationDefinition::Mutation(m) => ("mutation", &m.variable_definitions, &m.selection_set),
        OperationDefinition::Subscription(s) => {
            outcome.problems.push(format!(
                "{label}: subscriptions are not part of the curated surface"
            ));
            ("subscription", &s.variable_definitions, &s.selection_set)
        }
        OperationDefinition::SelectionSet(set) => ("query", &no_variables, set),
    };

    if let Some(entry) = entry {
        if entry.op_type != doc_op_type {
            outcome.problems.push(format!(
                "{label}: registry says op_type={} but the document is a {doc_op_type}",
                entry.op_type
            ));
        }
        if !entry.root_field.is_empty() {
            let selected = top_level_field_names(selection_set, &fragments);
            if !selected.contains(&entry.root_field) {
                outcome.problems.push(format!(
                    "{label}: registry root_field={} is not selected at the top level (selected: {})",
                    entry.root_field,
                    if selected.is_empty() {
                        "none".to_string()
                    } else {
                        selected.into_iter().collect::<Vec<_>>().join(", ")
                    }
                ));
            }
        }
    }

    for var in variables {
        let name = named_type(&var.var_type);
        if !index.knows(&name) {
            outcome.problems.push(format!(
                "{label}: variable ${} has type {name}, which the schema does not define",
                var.name
            ));
        }
    }

    let Some(root) = index.root_for(doc_op_type) else {
        outcome.problems.push(format!(
            "{label}: the schema declares no {doc_op_type} root type"
        ));
        return outcome;
    };

    let mut checker = Checker {
        index,
        fragments: &fragments,
        label,
        outcome,
        fragment_stack: Vec::new(),
    };
    checker.check_selection_set(root, selection_set, root);
    checker.outcome
}

fn top_level_field_names(
    set: &graphql_parser::query::SelectionSet<'_, String>,
    fragments: &BTreeMap<String, &graphql_parser::query::FragmentDefinition<'_, String>>,
) -> BTreeSet<String> {
    use graphql_parser::query::Selection;
    let mut out = BTreeSet::new();
    for selection in &set.items {
        match selection {
            Selection::Field(f) => {
                out.insert(f.name.clone());
            }
            Selection::InlineFragment(frag) => {
                out.extend(top_level_field_names(&frag.selection_set, fragments));
            }
            Selection::FragmentSpread(spread) => {
                if let Some(def) = fragments.get(&spread.fragment_name) {
                    out.extend(top_level_field_names(&def.selection_set, fragments));
                }
            }
        }
    }
    out
}

struct Checker<'a, 'b> {
    index: &'b SchemaIndex<'a>,
    fragments: &'b BTreeMap<String, &'b graphql_parser::query::FragmentDefinition<'b, String>>,
    label: &'b str,
    outcome: CheckOutcome,
    fragment_stack: Vec<String>,
}

impl Checker<'_, '_> {
    fn problem(&mut self, msg: String) {
        self.outcome.problems.push(format!("{}: {msg}", self.label));
    }

    fn warn(&mut self, msg: String) {
        self.outcome.warnings.push(format!("{}: {msg}", self.label));
    }

    fn check_selection_set(
        &mut self,
        type_name: &str,
        set: &graphql_parser::query::SelectionSet<'_, String>,
        path: &str,
    ) {
        use graphql_parser::query::Selection;
        use graphql_parser::schema::TypeDefinition as T;

        let Some(def) = self.index.get(type_name) else {
            self.warn(format!(
                "{path}: type {type_name} is not in the vendored schema; skipping its selections"
            ));
            return;
        };

        match def {
            T::Object(_) | T::Interface(_) => {}
            T::Union(u) => {
                let members: Vec<String> = u.types.clone();
                for selection in &set.items {
                    match selection {
                        Selection::Field(f) if f.name == "__typename" => {}
                        Selection::Field(f) => {
                            self.warn(format!(
                                "{path}: {type_name} is a union, so field {} cannot be checked \
                                 directly (use an inline fragment on one of: {})",
                                f.name,
                                members.join(", ")
                            ));
                        }
                        Selection::InlineFragment(frag) => {
                            self.check_inline_fragment(type_name, frag, path);
                        }
                        Selection::FragmentSpread(spread) => {
                            self.check_fragment_spread(spread, path);
                        }
                    }
                }
                return;
            }
            other => {
                self.problem(format!(
                    "{path}: cannot select fields on {} type {type_name}",
                    match other {
                        T::Scalar(_) => "scalar",
                        T::Enum(_) => "enum",
                        T::InputObject(_) => "input",
                        _ => "non-composite",
                    }
                ));
                return;
            }
        }

        let fields = composite_fields(def).map(|f| f.as_slice()).unwrap_or(&[]);

        for selection in &set.items {
            match selection {
                Selection::Field(f) if f.name == "__typename" => {}
                Selection::Field(f) => {
                    let Some(field_def) = fields.iter().find(|d| d.name == f.name) else {
                        self.problem(format!(
                            "{path}.{}: type {type_name} has no such field",
                            f.name
                        ));
                        continue;
                    };

                    if field_def.directives.iter().any(|d| d.name == "deprecated") {
                        self.warn(format!(
                            "{path}.{}: field is deprecated in the schema",
                            f.name
                        ));
                    }

                    for (arg_name, _value) in &f.arguments {
                        if !field_def.arguments.iter().any(|a| &a.name == arg_name) {
                            self.problem(format!(
                                "{path}.{}: argument {arg_name} is not defined on this field",
                                f.name
                            ));
                        }
                    }

                    let target = named_type(&field_def.field_type);
                    let child_path = format!("{path}.{}", f.name);
                    let target_is_composite = matches!(
                        self.index.get(&target),
                        Some(
                            graphql_parser::schema::TypeDefinition::Object(_)
                                | graphql_parser::schema::TypeDefinition::Interface(_)
                                | graphql_parser::schema::TypeDefinition::Union(_)
                        )
                    );

                    if f.selection_set.items.is_empty() {
                        if target_is_composite {
                            self.problem(format!(
                                "{child_path}: field returns composite type {target} and needs a selection set"
                            ));
                        }
                        continue;
                    }
                    if !target_is_composite && self.index.knows(&target) {
                        self.problem(format!(
                            "{child_path}: field returns leaf type {target} and cannot have a selection set"
                        ));
                        continue;
                    }
                    self.check_selection_set(&target, &f.selection_set, &child_path);
                }
                Selection::InlineFragment(frag) => {
                    self.check_inline_fragment(type_name, frag, path);
                }
                Selection::FragmentSpread(spread) => {
                    self.check_fragment_spread(spread, path);
                }
            }
        }
    }

    fn check_inline_fragment(
        &mut self,
        parent: &str,
        frag: &graphql_parser::query::InlineFragment<'_, String>,
        path: &str,
    ) {
        use graphql_parser::query::TypeCondition;
        let target = match &frag.type_condition {
            Some(TypeCondition::On(name)) => name.clone(),
            None => parent.to_string(),
        };
        let child_path = format!("{path}<{target}>");
        self.check_selection_set(&target, &frag.selection_set, &child_path);
    }

    fn check_fragment_spread(
        &mut self,
        spread: &graphql_parser::query::FragmentSpread<'_, String>,
        path: &str,
    ) {
        use graphql_parser::query::TypeCondition;
        let name = spread.fragment_name.clone();
        let Some(def) = self.fragments.get(&name).copied() else {
            self.warn(format!(
                "{path}: fragment {name} is not defined in this document; skipping it"
            ));
            return;
        };
        if self.fragment_stack.contains(&name) {
            self.problem(format!("{path}: fragment {name} is recursive"));
            return;
        }
        let TypeCondition::On(target) = &def.type_condition;
        let target = target.clone();
        let child_path = format!("{path}...{name}");
        self.fragment_stack.push(name);
        self.check_selection_set(&target, &def.selection_set, &child_path);
        self.fragment_stack.pop();
    }
}

// ─── shared ────────────────────────────────────────────────────────

fn workspace_root() -> Result<PathBuf> {
    // CARGO_MANIFEST_DIR points at xtask/; one level up is the workspace root.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR unset")?;
    Ok(PathBuf::from(manifest_dir)
        .parent()
        .context("xtask has no parent dir")?
        .to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SDL printer ────────────────────────────────────────────────

    fn sample_introspection() -> serde_json::Value {
        serde_json::json!({
            "queryType": { "name": "Query" },
            "mutationType": { "name": "Mutation" },
            "subscriptionType": serde_json::Value::Null,
            "types": [
                { "kind": "SCALAR", "name": "DateTime" },
                {
                    "kind": "OBJECT",
                    "name": "Query",
                    "interfaces": [],
                    "fields": [
                        {
                            "name": "issuesV2",
                            "args": [
                                { "name": "first", "type": { "kind": "SCALAR", "name": "Int" }, "defaultValue": "50" },
                                { "name": "after", "type": { "kind": "SCALAR", "name": "String" }, "defaultValue": null }
                            ],
                            "type": { "kind": "OBJECT", "name": "IssueConnection" },
                            "isDeprecated": false,
                            "deprecationReason": null
                        },
                        {
                            "name": "legacyIssues",
                            "args": [],
                            "type": { "kind": "OBJECT", "name": "IssueConnection" },
                            "isDeprecated": true,
                            "deprecationReason": "use issuesV2"
                        }
                    ]
                },
                {
                    "kind": "OBJECT",
                    "name": "Mutation",
                    "fields": [
                        {
                            "name": "deleteReport",
                            "args": [{ "name": "id", "type": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "ID" } } }],
                            "type": { "kind": "SCALAR", "name": "Boolean" },
                            "isDeprecated": false
                        }
                    ]
                },
                {
                    "kind": "OBJECT",
                    "name": "IssueConnection",
                    "fields": [
                        {
                            "name": "nodes",
                            "args": [],
                            "type": { "kind": "LIST", "ofType": { "kind": "NON_NULL", "ofType": { "kind": "OBJECT", "name": "Issue" } } },
                            "isDeprecated": false
                        },
                        {
                            "name": "pageInfo",
                            "args": [],
                            "type": { "kind": "NON_NULL", "ofType": { "kind": "OBJECT", "name": "PageInfo" } },
                            "isDeprecated": false
                        }
                    ]
                },
                {
                    "kind": "OBJECT",
                    "name": "PageInfo",
                    "fields": [
                        { "name": "endCursor", "args": [], "type": { "kind": "SCALAR", "name": "String" }, "isDeprecated": false },
                        { "name": "hasNextPage", "args": [], "type": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "Boolean" } }, "isDeprecated": false }
                    ]
                },
                {
                    "kind": "OBJECT",
                    "name": "Issue",
                    "interfaces": [{ "kind": "INTERFACE", "name": "Node" }],
                    "fields": [
                        { "name": "id", "args": [], "type": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "ID" } }, "isDeprecated": false },
                        { "name": "createdAt", "args": [], "type": { "kind": "SCALAR", "name": "DateTime" }, "isDeprecated": false },
                        { "name": "severity", "args": [], "type": { "kind": "ENUM", "name": "Severity" }, "isDeprecated": false },
                        { "name": "entity", "args": [], "type": { "kind": "UNION", "name": "Entity" }, "isDeprecated": false }
                    ]
                },
                {
                    "kind": "INTERFACE",
                    "name": "Node",
                    "fields": [
                        { "name": "id", "args": [], "type": { "kind": "NON_NULL", "ofType": { "kind": "SCALAR", "name": "ID" } }, "isDeprecated": false }
                    ]
                },
                { "kind": "UNION", "name": "Entity", "possibleTypes": [{ "kind": "OBJECT", "name": "Issue" }, { "kind": "OBJECT", "name": "PageInfo" }] },
                {
                    "kind": "ENUM",
                    "name": "Severity",
                    "enumValues": [
                        { "name": "LOW", "isDeprecated": false },
                        { "name": "CRITICAL", "isDeprecated": false },
                        { "name": "INFORMATIONAL", "isDeprecated": true, "deprecationReason": "folded into LOW" }
                    ]
                },
                {
                    "kind": "INPUT_OBJECT",
                    "name": "IssueFilters",
                    "inputFields": [
                        { "name": "severity", "type": { "kind": "ENUM", "name": "Severity" }, "defaultValue": "LOW" }
                    ]
                },
                { "kind": "OBJECT", "name": "__Hidden", "fields": [] },
                { "kind": "SCALAR", "name": "String" }
            ]
        })
    }

    fn sample_sdl() -> String {
        let schema: introspection::Schema =
            serde_json::from_value(sample_introspection()).expect("parse introspection");
        introspection::to_sdl(&schema).expect("print sdl")
    }

    #[test]
    fn sdl_declares_roots_and_types() {
        let sdl = sample_sdl();
        assert!(sdl.contains("schema {\n  query: Query\n  mutation: Mutation\n}"));
        assert!(sdl.contains("scalar DateTime"));
        assert!(sdl.contains("type Issue implements Node {"));
        assert!(sdl.contains("interface Node {"));
        assert!(sdl.contains("union Entity = Issue | PageInfo"));
        assert!(sdl.contains("enum Severity {"));
        assert!(sdl.contains("input IssueFilters {"));
    }

    #[test]
    fn sdl_omits_introspection_and_built_in_scalars() {
        let sdl = sample_sdl();
        assert!(!sdl.contains("__Hidden"));
        assert!(!sdl.contains("scalar String"));
    }

    #[test]
    fn sdl_renders_wrappers_args_and_deprecation() {
        let sdl = sample_sdl();
        assert!(sdl.contains("nodes: [Issue!]"));
        assert!(sdl.contains("pageInfo: PageInfo!"));
        assert!(sdl.contains("issuesV2(after: String, first: Int = 50): IssueConnection"));
        assert!(
            sdl.contains("legacyIssues: IssueConnection @deprecated(reason: \"use issuesV2\")")
        );
        assert!(sdl.contains("INFORMATIONAL @deprecated(reason: \"folded into LOW\")"));
    }

    #[test]
    fn sdl_is_sorted_and_therefore_stable() {
        let sdl = sample_sdl();
        // Types sorted by name: DateTime before Entity before Issue.
        let date = sdl.find("scalar DateTime").expect("DateTime");
        let entity = sdl.find("union Entity").expect("Entity");
        let issue = sdl.find("type Issue ").expect("Issue");
        assert!(date < entity && entity < issue);
        // Enum values sorted too, regardless of server order.
        let critical = sdl.find("CRITICAL").expect("CRITICAL");
        let low = sdl.find("\n  LOW").expect("LOW");
        assert!(critical < low);
        // Printing twice from the same input is byte-identical.
        assert_eq!(sdl, sample_sdl());
    }

    #[test]
    fn sdl_round_trips_through_the_parser() {
        let sdl = sample_sdl();
        let parsed = graphql_parser::parse_schema::<String>(&sdl);
        assert!(parsed.is_ok(), "emitted SDL must reparse: {parsed:?}");
    }

    #[test]
    fn render_type_ref_handles_nested_wrappers() {
        let ty: introspection::TypeRef = serde_json::from_value(serde_json::json!({
            "kind": "NON_NULL",
            "ofType": { "kind": "LIST", "ofType": { "kind": "NON_NULL", "ofType": { "kind": "OBJECT", "name": "Issue" } } }
        }))
        .expect("parse type ref");
        assert_eq!(
            introspection::render_type_ref(&ty).expect("render"),
            "[Issue!]!"
        );
    }

    #[test]
    fn render_type_ref_rejects_unnamed_leaf() {
        let ty: introspection::TypeRef =
            serde_json::from_value(serde_json::json!({ "kind": "OBJECT", "name": null }))
                .expect("parse type ref");
        assert!(introspection::render_type_ref(&ty).is_err());
    }

    // ── registry parsing ───────────────────────────────────────────

    #[test]
    fn parse_registry_reads_the_real_table() {
        let source = include_str!("../../crates/stave-api/src/lib.rs");
        let registry = parse_registry(source).expect("parse registry");
        let issues = registry.get("list_issues").expect("list_issues registered");
        assert_eq!(issues.op_type, "query");
        assert_eq!(issues.root_field, "issuesV2");
        assert_eq!(registry.len(), stave_api_operation_count(source));
    }

    /// Count the table's struct literals as an independent tally, so the
    /// regex cannot pass by matching a subset of the table. Line-exact so
    /// the `pub struct OperationDoc {` definition is not counted.
    fn stave_api_operation_count(source: &str) -> usize {
        source
            .lines()
            .filter(|line| line.trim() == "OperationDoc {")
            .count()
    }

    #[test]
    fn parse_registry_rejects_an_unrecognized_shape() {
        assert!(parse_registry("fn main() {}").is_err());
    }

    // ── document checking ──────────────────────────────────────────

    const TEST_SCHEMA: &str = r#"
schema { query: Query mutation: Mutation }
scalar DateTime
type Query { issuesV2(first: Int, after: String): IssueConnection }
type Mutation { deleteReport(id: ID!): Boolean }
type IssueConnection { nodes: [Issue!] pageInfo: PageInfo! }
type PageInfo { hasNextPage: Boolean! endCursor: String }
type Issue { id: ID! createdAt: DateTime entity: Entity old: String @deprecated(reason: "gone") }
union Entity = Issue | PageInfo
"#;

    fn run_check(source: &str, entry: Option<RegistryEntry>) -> CheckOutcome {
        let doc = graphql_parser::parse_schema::<String>(TEST_SCHEMA).expect("schema");
        let index = SchemaIndex::build(&doc).expect("index");
        check_document(&index, "probe", source, entry.as_ref())
    }

    fn query_entry(root_field: &str) -> RegistryEntry {
        RegistryEntry {
            op_type: "query".to_string(),
            root_field: root_field.to_string(),
        }
    }

    #[test]
    fn valid_document_has_no_problems() {
        let outcome = run_check(
            r#"query ListIssues($first: Int, $after: String) {
                 issuesV2(first: $first, after: $after) {
                   nodes { id createdAt }
                   pageInfo { hasNextPage endCursor }
                 }
               }"#,
            Some(query_entry("issuesV2")),
        );
        assert!(
            outcome.problems.is_empty(),
            "unexpected problems: {:?}",
            outcome.problems
        );
    }

    #[test]
    fn unknown_root_field_is_a_problem() {
        let outcome = run_check(
            "query X { nope { nodes { id } } }",
            Some(query_entry("nope")),
        );
        assert!(
            outcome.problems.iter().any(|p| p.contains("no such field")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn unknown_nested_field_is_a_problem() {
        let outcome = run_check(
            "query X { issuesV2 { nodes { id notAField } } }",
            Some(query_entry("issuesV2")),
        );
        assert!(
            outcome
                .problems
                .iter()
                .any(|p| p.contains("issuesV2.nodes.notAField")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn leaf_with_selection_set_is_a_problem() {
        let outcome = run_check(
            "query X { issuesV2 { nodes { id { deeper } } } }",
            Some(query_entry("issuesV2")),
        );
        assert!(
            outcome.problems.iter().any(|p| p.contains("leaf type ID")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn composite_without_selection_set_is_a_problem() {
        let outcome = run_check("query X { issuesV2 }", Some(query_entry("issuesV2")));
        assert!(
            outcome
                .problems
                .iter()
                .any(|p| p.contains("needs a selection set")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn unknown_argument_is_a_problem() {
        let outcome = run_check(
            "query X { issuesV2(limit: 5) { nodes { id } } }",
            Some(query_entry("issuesV2")),
        );
        assert!(
            outcome
                .problems
                .iter()
                .any(|p| p.contains("argument limit")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn unknown_variable_type_is_a_problem() {
        let outcome = run_check(
            "query X($f: NopeFilter) { issuesV2 { nodes { id } } }",
            Some(query_entry("issuesV2")),
        );
        assert!(
            outcome
                .problems
                .iter()
                .any(|p| p.contains("schema does not define")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn op_type_mismatch_is_a_problem() {
        let outcome = run_check(
            "mutation X { deleteReport(id: \"x\") }",
            Some(query_entry("deleteReport")),
        );
        assert!(
            outcome
                .problems
                .iter()
                .any(|p| p.contains("registry says op_type=query")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn root_field_not_selected_is_a_problem() {
        let outcome = run_check(
            "query X { issuesV2 { nodes { id } } }",
            Some(query_entry("somethingElse")),
        );
        assert!(
            outcome
                .problems
                .iter()
                .any(|p| p.contains("is not selected at the top level")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn mutation_document_validates_against_the_mutation_root() {
        let outcome = run_check(
            "mutation X { deleteReport(id: \"x\") }",
            Some(RegistryEntry {
                op_type: "mutation".to_string(),
                root_field: "deleteReport".to_string(),
            }),
        );
        assert!(
            outcome.problems.is_empty(),
            "unexpected problems: {:?}",
            outcome.problems
        );
    }

    #[test]
    fn union_selection_warns_and_inline_fragment_is_checked() {
        let outcome = run_check(
            r#"query X {
                 issuesV2 {
                   nodes {
                     entity { __typename name ... on Issue { id } }
                   }
                 }
               }"#,
            Some(query_entry("issuesV2")),
        );
        assert!(
            outcome.warnings.iter().any(|w| w.contains("is a union")),
            "{:?}",
            outcome.warnings
        );
        assert!(
            outcome.problems.is_empty(),
            "unexpected problems: {:?}",
            outcome.problems
        );
    }

    #[test]
    fn inline_fragment_on_a_union_member_still_checks_fields() {
        let outcome = run_check(
            r#"query X {
                 issuesV2 { nodes { entity { ... on Issue { bogus } } } }
               }"#,
            Some(query_entry("issuesV2")),
        );
        assert!(
            outcome.problems.iter().any(|p| p.contains("bogus")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn fragment_spread_is_followed() {
        let outcome = run_check(
            r#"query X { issuesV2 { nodes { ...IssueFields } } }
               fragment IssueFields on Issue { id bogus }"#,
            Some(query_entry("issuesV2")),
        );
        assert!(
            outcome.problems.iter().any(|p| p.contains("bogus")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn undefined_fragment_spread_warns() {
        let outcome = run_check(
            "query X { issuesV2 { nodes { ...Missing } } }",
            Some(query_entry("issuesV2")),
        );
        assert!(
            outcome
                .warnings
                .iter()
                .any(|w| w.contains("fragment Missing")),
            "{:?}",
            outcome.warnings
        );
        assert!(
            outcome.problems.is_empty(),
            "unexpected problems: {:?}",
            outcome.problems
        );
    }

    #[test]
    fn deprecated_field_selection_warns() {
        let outcome = run_check(
            "query X { issuesV2 { nodes { id old } } }",
            Some(query_entry("issuesV2")),
        );
        assert!(
            outcome.warnings.iter().any(|w| w.contains("deprecated")),
            "{:?}",
            outcome.warnings
        );
    }

    #[test]
    fn multiple_operations_in_one_document_is_a_problem() {
        let outcome = run_check(
            "query A { issuesV2 { nodes { id } } } query B { issuesV2 { nodes { id } } }",
            Some(query_entry("issuesV2")),
        );
        assert!(
            outcome
                .problems
                .iter()
                .any(|p| p.contains("exactly one operation")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn unparseable_document_is_a_problem() {
        let outcome = run_check("query X { issuesV2 {", Some(query_entry("issuesV2")));
        assert!(
            outcome.problems.iter().any(|p| p.contains("parse error")),
            "{:?}",
            outcome.problems
        );
    }

    #[test]
    fn subscription_is_rejected() {
        let outcome = run_check("subscription X { issuesV2 { nodes { id } } }", None);
        assert!(
            outcome
                .problems
                .iter()
                .any(|p| p.contains("subscriptions are not part")),
            "{:?}",
            outcome.problems
        );
    }
}
