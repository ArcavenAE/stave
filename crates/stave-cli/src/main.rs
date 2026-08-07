//! stave: Rust CLI for the Wiz GraphQL API.
//!
//! Unofficial; not affiliated with or endorsed by Wiz, Inc.
//!
//! Shape of the surface:
//!   * credentials and defaults: `auth`, `registry`, `config`
//!   * the schema surface: `ops`, `api`
//!   * stream primitives: `list`, `search`, `get`, `filter`, `enrich`, `emit`
//!   * the hosted MCP server: `mcp`
//!
//! stdout carries the contract (JSONL records, GraphQL documents, JSON
//! payloads). Prose, prompts, and diagnostics go to stderr. Exit codes:
//! 0 success, 1 operation failure, 2 argv/usage error.

#![forbid(unsafe_code)]

use std::io::{BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Map, Value, json};
use stave_sdk::stream::{Record, SourceRef, read_stream, write_record};
use stave_sdk::{
    ACCESS_TOKEN_ENV, CallOptions, Client, KindSpec, SCOPE_METADATA_PROVISIONAL, audit, auth, cel,
    enrich, kind_spec, kinds, mcp, ops, token, token_scopes,
};
use uuid::Uuid;

/// Largest page stave asks for in one GraphQL call. Wiz connections cap
/// `first` well below this on some fields; a smaller page just costs an
/// extra round trip, an oversized one is refused outright.
const MAX_PAGE_SIZE: usize = 500;

/// How many consecutive zero-node pages stave will follow before it
/// gives up on a connection. An empty page with `hasNextPage: true` is
/// legitimate and must be followed (see `stream_kind`), but a server
/// that emits them without end, each with a fresh cursor, would
/// otherwise page forever against a live tenant.
const MAX_EMPTY_PAGES: usize = 10;

/// Recipe names the SDK's enrichment library accepts. Mirrored here for
/// error messages only: `enrich::Recipe::parse` stays the authority, and
/// `recipe_names_match_sdk` in the tests below fails loudly if this list
/// drifts from it.
const RECIPES: &[&str] = &["account-context", "severity-roll-up", "entity-hoist"];

const CONFIG_KEYS: &str =
    "client_id, api_url, posture, token_url, mcp.url, registry.host, registry.username";

#[derive(Parser, Debug)]
#[command(
    name = "stave",
    version = stave_sdk::FULL_VERSION,
    about = "Unofficial Rust CLI for the Wiz GraphQL API",
    long_about = "Agent-first CLI over the Wiz GraphQL API. Curated operation library as the \
                  contract, audit-trail-as-feature, read-only against the live tenant by \
                  default.\n\n\
                  Authenticate with a Wiz service account: `stave auth login` prompts for the \
                  client ID and secret, puts the secret in the platform keyring, and verifies \
                  that a token mints. `stave ops list` shows the curated operations, \
                  `stave list <kind>` streams records as JSONL, and `stave api --query <doc>` \
                  runs an ad-hoc GraphQL document through the same parser, write-guard, and \
                  audit trail.\n\n\
                  Unofficial: not affiliated with, sponsored by, or endorsed by Wiz, Inc."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Manage service-account credentials and persisted defaults.
    Auth(AuthArgs),
    /// Manage container-registry credentials.
    Registry(RegistryArgs),
    /// Inspect or modify the persisted config.
    Config(ConfigArgs),
    /// List or show curated GraphQL operations.
    Ops(OpsArgs),
    /// Run a curated operation by name, or an ad-hoc GraphQL document.
    Api(ApiArgs),
    /// Stream records of a `_kind` as a JSON-line stream.
    List(ListArgs),
    /// List, then substring-match the kind's search field.
    Search(SearchArgs),
    /// Fetch one record by ID (not supported in v0.1).
    Get(GetArgs),
    /// Drop records that don't match a CEL predicate.
    Filter(FilterArgs),
    /// Attach computed or joined fields per a named recipe.
    Enrich(EnrichArgs),
    /// Format a JSON-line stream from stdin.
    Emit(EmitArgs),
    /// Operate the hosted Wiz MCP server (tools, calls, client config).
    Mcp(McpArgs),
}

// ---------------------------------------------------------------------------
// auth
// ---------------------------------------------------------------------------

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Manage Wiz service-account credentials and persisted defaults.\n\n\
                  Client-ID chain:\n  \
                  1. --client-id flag (per call)\n  \
                  2. STAVE_CLIENT_ID environment variable\n  \
                  3. [auth] client_id in the config file\n\n\
                  Client-secret chain:\n  \
                  1. STAVE_CLIENT_SECRET environment variable\n  \
                  2. platform keyring (macOS Keychain, Linux Secret Service)\n  \
                  3. [auth] client_secret in the config file (discouraged)\n\n\
                  API-endpoint chain:\n  \
                  1. --api-url flag (per call)\n  \
                  2. STAVE_API_URL environment variable\n  \
                  3. [default] api_url in the config file\n  \
                  4. derived from the minted token's data-center claim\n\n\
                  Config lives at ~/.config/stave/config.toml (override with STAVE_CONFIG). \
                  Minted tokens are cached in the XDG state dir, never in config."
)]
struct AuthArgs {
    #[command(subcommand)]
    cmd: AuthCmd,
}

#[derive(Subcommand, Debug)]
enum AuthCmd {
    /// Store credentials, then verify that a token mints.
    Login(AuthLoginArgs),
    /// Report every resolved credential, endpoint, and default with its source.
    Status,
    /// Remove the stored client secret and the cached token.
    Logout,
    /// List the scopes the current token carries (decoded from the
    /// token at hand; no mint, no API call).
    Scopes,
    /// Answer whether the current token can run an operation. Exit 0
    /// yes, 1 no. Reads the token's scopes against the registry.
    CanI {
        /// Operation name. Run `stave ops list` to discover.
        operation: String,
    },
    /// Print the least-privilege scope set to provision a service
    /// account for the selected operations, and the scopes to withhold.
    Plan(AuthPlanArgs),
}

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Produce the provisioning checklist for a Wiz service account.\n\n\
                  GRANT lists the least-privilege union of scopes the selected operations \
                  need (default: all curated operations). DO NOT GRANT names scopes to \
                  withhold and why. Scope names are PROVISIONAL until validated against a \
                  live tenant.\n\n\
                  --check compares the scopes the current token carries against what the \
                  selected operations require, exits nonzero on drift, and reports MISSING \
                  (credential is unusable for some operation) separately from EXCESS \
                  (credential is over-privileged)."
)]
struct AuthPlanArgs {
    /// Operations to plan for. Repeatable. Default: all curated.
    #[arg(long = "op", value_name = "NAME")]
    ops: Vec<String>,

    /// Compare the current token's scopes against the requirement and
    /// report missing vs excess. Exits nonzero on any drift.
    #[arg(long)]
    check: bool,
}

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Store Wiz service-account credentials and verify them.\n\n\
                  On a terminal, stave prompts for the client ID (when --client-id is absent) \
                  and reads the secret without echoing it. In a pipeline, pass --stdin so the \
                  secret arrives on stdin and nothing prompts:\n\n  \
                  printf '%s' \"$WIZ_CLIENT_SECRET\" | stave auth login --client-id <id> --stdin\n\n\
                  The secret goes to the platform keyring. The client ID and any --api-url / \
                  --token-url go to the config file. The cached token is cleared so a rotated \
                  secret cannot be shadowed by a token minted from the old one.\n\n\
                  stave then mints a token to prove the credentials work and reports the \
                  endpoint it resolved (including the data-center claim derivation). Pass \
                  --no-verify to skip that round trip; stored values are kept either way."
)]
struct AuthLoginArgs {
    /// Service-account client ID. Prompted for on a terminal when absent.
    #[arg(long, value_name = "ID")]
    client_id: Option<String>,

    /// Read the client secret from stdin (entire stream, trimmed).
    #[arg(long)]
    stdin: bool,

    /// Persist `[default] api_url`, the tenant GraphQL endpoint
    /// (`https://api.<region>.app.wiz.io/graphql`).
    #[arg(long, value_name = "URL")]
    api_url: Option<String>,

    /// Persist `[auth] token_url`, the OAuth token endpoint. Only
    /// needed when the tenant's endpoint differs from the built-in
    /// default (gov or Auth0-era tenants).
    #[arg(long, value_name = "URL")]
    token_url: Option<String>,

    /// Store the credentials without minting a verification token.
    #[arg(long)]
    no_verify: bool,
}

// ---------------------------------------------------------------------------
// registry
// ---------------------------------------------------------------------------

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Manage the container-registry credentials a Wiz tenant issues for pulling \
                  vendor images.\n\n\
                  The password follows the same chain as every other secret \
                  (STAVE_REGISTRY_PASSWORD environment variable, then the platform keyring, \
                  then config). Host and username persist in the config file.\n\n\
                  The username is tenant-identifying: it embeds the tenant ID. Keep it out of \
                  commits, issues, and shared logs."
)]
struct RegistryArgs {
    #[command(subcommand)]
    cmd: RegistryCmd,
}

#[derive(Subcommand, Debug)]
enum RegistryCmd {
    /// Store the registry password (keyring) and host/username (config).
    Login(RegistryLoginArgs),
    /// Report the resolved registry host, username, and password source.
    Status,
    /// Print the password for piping into `docker login --password-stdin`.
    Credential {
        /// Write the password itself to stdout instead of a masked placeholder.
        #[arg(long)]
        reveal: bool,
    },
    /// Remove the registry password from the platform keyring.
    Logout,
}

#[derive(clap::Args, Debug)]
struct RegistryLoginArgs {
    /// Registry hostname.
    #[arg(long, value_name = "HOST")]
    host: Option<String>,

    /// Registry username (tenant-identifying).
    #[arg(long, value_name = "USER")]
    username: Option<String>,

    /// Read the password from stdin (entire stream, trimmed).
    #[arg(long)]
    stdin: bool,
}

// ---------------------------------------------------------------------------
// config
// ---------------------------------------------------------------------------

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Inspect or modify the persisted config at ~/.config/stave/config.toml \
                  (override with STAVE_CONFIG).\n\n\
                  Subcommands:\n  \
                  show              print the current config with secrets masked\n  \
                  path              print the resolved config path\n  \
                  set <key> <val>   set one key\n  \
                  unset <key>       clear one key\n\n\
                  Keys: client_id, api_url, posture, token_url, mcp.url, registry.host, \
                  registry.username.\n\n\
                  Secrets are not settable here: use `stave auth login` and \
                  `stave registry login` so they land in the platform keyring. Setting a key \
                  preserves every other section of the file."
)]
struct ConfigArgs {
    #[command(subcommand)]
    cmd: ConfigCmd,
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// Print the current config (secrets masked).
    Show,
    /// Print the resolved config file path.
    Path,
    /// Set one config key.
    Set {
        /// Key to set.
        key: String,
        /// Value to write.
        value: String,
    },
    /// Clear one config key.
    Unset {
        /// Key to clear.
        key: String,
    },
}

// ---------------------------------------------------------------------------
// ops
// ---------------------------------------------------------------------------

#[derive(clap::Args, Debug)]
#[command(long_about = "Inspect the curated GraphQL operation library.\n\n\
                  Each operation is a checked-in document validated against the vendored \
                  schema by `cargo xtask check-ops`. `ops list` emits one JSON line per \
                  operation; `ops show` writes the document itself to stdout so it can be \
                  saved, edited, and re-run through `stave api --query`.")]
struct OpsArgs {
    #[command(subcommand)]
    cmd: OpsCmd,
}

#[derive(Subcommand, Debug)]
enum OpsCmd {
    /// List curated operations as a JSON-line stream.
    List {
        /// Substring filter on the operation name.
        #[arg(long, value_name = "TEXT")]
        filter: Option<String>,
    },
    /// Print one operation's GraphQL document.
    Show {
        /// Operation name. Run `stave ops list` to discover.
        name: String,
    },
    /// Report the Wiz scopes and effect metadata each operation needs.
    /// Offline; pure registry metadata, no tenant contact. Scope names
    /// are provisional until validated against a live tenant.
    Permissions {
        /// Substring filter on the operation name.
        #[arg(long, value_name = "TEXT")]
        filter: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// api
// ---------------------------------------------------------------------------

#[derive(clap::Args, Debug)]
#[command(
    group(clap::ArgGroup::new("document").required(true).args(["name", "query"])),
    long_about = "Run a curated operation by name, or an ad-hoc GraphQL document.\n\n\
                  Variables come from --vars '<json object>' and repeatable --var key=value \
                  (later --var wins). A --var value is parsed as JSON when it parses, and \
                  taken as a string otherwise, so --var first=5 sends a number and \
                  --var status=OPEN sends a string.\n\n\
                  Ad-hoc documents are parsed before anything reaches the wire: an \
                  unparseable document is refused, and a mutation or subscription anywhere in \
                  it is refused unconditionally (stave is read-only against live tenants). \
                  Ad-hoc documents run only under the exploratory read posture \
                  (`stave config set posture exploratory`); the default curated posture \
                  refuses them.\n\n\
                  Examples:\n  \
                  stave api list_issues --var first=5\n  \
                  stave api list_projects --vars '{\"first\": 100}'\n  \
                  stave api --query ./my-query.graphql --vars '{\"first\": 5}'\n  \
                  cat query.graphql | stave api --query -"
)]
struct ApiArgs {
    /// Curated operation name. Run `stave ops list` to discover.
    name: Option<String>,

    /// Path to a GraphQL document, or `-` for stdin.
    #[arg(long, value_name = "FILE", conflicts_with = "name")]
    query: Option<String>,

    /// GraphQL variables as a JSON object.
    #[arg(long, value_name = "JSON")]
    vars: Option<String>,

    /// One variable as `key=value`. Repeatable. Overrides --vars.
    #[arg(long = "var", value_name = "KEY=VALUE")]
    var: Vec<String>,

    /// Tenant GraphQL endpoint override. Resolves through
    /// flag, then STAVE_API_URL, then config, then the token's
    /// data-center claim.
    #[arg(long, value_name = "URL")]
    api_url: Option<String>,

    /// Skip operation and response detail in the audit trail (a stub
    /// line is still recorded).
    #[arg(long)]
    no_audit: bool,
}

// ---------------------------------------------------------------------------
// primitives
// ---------------------------------------------------------------------------

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Stream records of a `_kind` from the Wiz API as JSON lines.\n\n\
                  Each line carries `_kind`, `_source` (operation name, response index, \
                  fetched_at), and the record's own fields. Compose with `stave filter`, \
                  `stave enrich`, and `stave emit`.\n\n\
                  stave pages the underlying GraphQL connection until --limit records are \
                  emitted or the connection runs out, sharing one audit trace across the \
                  pages of a single call.\n\n\
                  Examples:\n  \
                  stave list issue --limit 20\n  \
                  stave list vulnerability_finding --limit 200 | stave emit --format md\n  \
                  stave list issue --since 24h"
)]
struct ListArgs {
    /// Stream-contract `_kind`.
    #[arg(value_parser = kind_value_parser())]
    kind: String,

    /// Maximum number of records to emit.
    #[arg(long, value_name = "N", default_value_t = 50)]
    limit: usize,

    /// Drop records older than this duration, in Go duration syntax
    /// (`24h`, `30m`, `1h30m`, `0.5h`). Units: ns, us, µs, ms, s, m, h.
    /// Go duration has no `d`, so use `24h` rather than `1d`. Requires
    /// the kind to have a primary timestamp field.
    #[arg(long, value_name = "DUR")]
    since: Option<String>,

    /// Tenant GraphQL endpoint override.
    #[arg(long, value_name = "URL")]
    api_url: Option<String>,

    /// Skip the per-call audit detail (a stub line is still recorded).
    #[arg(long)]
    no_audit: bool,
}

#[derive(clap::Args, Debug)]
#[command(
    long_about = "List a kind, then keep the records whose search field contains the query \
                  (case-insensitive substring).\n\n\
                  This is a v0.1 client-side fallback: the curated list operations do not yet \
                  carry server-side filter variables (charter F2). Kinds with no declared \
                  search field error and point at `stave list | stave filter` instead.\n\n\
                  Examples:\n  \
                  stave search project platform\n  \
                  stave search issue exposure --limit 5"
)]
struct SearchArgs {
    /// Stream-contract `_kind`.
    #[arg(value_parser = kind_value_parser())]
    kind: String,

    /// Substring to match (case-insensitive).
    query: String,

    /// Maximum number of matching records to emit.
    #[arg(long, value_name = "N", default_value_t = 50)]
    limit: usize,

    /// Tenant GraphQL endpoint override.
    #[arg(long, value_name = "URL")]
    api_url: Option<String>,

    /// Skip the per-call audit detail (a stub line is still recorded).
    #[arg(long)]
    no_audit: bool,
}

#[derive(clap::Args, Debug)]
#[command(long_about = "Fetch one record by ID. Not supported in v0.1.\n\n\
                  Singular lookups need per-kind singular queries or filter input types, and \
                  stave does not guess at schema shapes (charter F2, resolved by schema \
                  introspection). Until then, write the document yourself:\n\n  \
                  stave api --query ./issue-by-id.graphql --var id=<id>\n\n\
                  or select client-side from a list stream:\n\n  \
                  stave list issue --limit 500 | stave filter --where 'id == \"<id>\"'")]
struct GetArgs {
    /// Stream-contract `_kind`.
    #[arg(value_parser = kind_value_parser())]
    kind: String,

    /// Identifier for the record.
    id: String,
}

#[derive(clap::Args, Debug)]
#[command(long_about = "Drop records that don't match a CEL predicate.\n\n\
                  The predicate is Common Expression Language. Each top-level field of a \
                  record is bound as a top-level variable, so `severity == \"CRITICAL\"` \
                  works directly. The whole record is also bound as `record` for the `has()` \
                  macro (`has(record.resolvedAt)`).\n\n\
                  Adapter rules:\n  \
                  - camelCase `*At` fields plus `timestamp`, `*_at`, `*_date`, and `ts` are \
                  promoted to timestamps, so `createdAt < now` works\n  \
                  - accessing a field the record does not carry is a runtime error; use \
                  `has(record.X)` to test for absence\n  \
                  - `now` is bound to the current UTC time once per query\n  \
                  - the predicate must return a boolean\n\n\
                  Use --explain to print the schema table, the parsed AST, and the `now` \
                  binding without reading stdin.\n\n\
                  Examples:\n  \
                  stave filter --where '_kind == \"issue\" && severity == \"CRITICAL\"'\n  \
                  stave filter --where 'severity in [\"CRITICAL\", \"HIGH\"]'\n  \
                  stave filter --where 'createdAt > now - duration(\"720h\")'\n  \
                  stave filter --where 'has(record.resolvedAt)' --explain")]
struct FilterArgs {
    /// CEL predicate. Emits one record per matching input.
    #[arg(long, value_name = "CEL")]
    r#where: String,

    /// Print the schema, parsed AST, and `now` binding, then exit
    /// without reading stdin.
    #[arg(long)]
    explain: bool,
}

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Attach computed or joined fields to records per a named recipe.\n\n\
                  Recipes (v0.1):\n  \
                  account-context   for each cloud_resource, attach its owning cloud account \
                  as `account`, joined on subscriptionExternalId. Resources whose \
                  subscription has no matching account get `account: null`; other kinds pass \
                  through. Requires --accounts.\n  \
                  severity-roll-up  set `severity_rollup` from whichever severity field the \
                  kind carries (vendorSeverity, else severity), null when neither is present.\n  \
                  entity-hoist      lift issue.entitySnapshot name, type, and cloudPlatform \
                  to top-level entity_name, entity_type, entity_cloud_platform.\n\n\
                  Auxiliary records come from --accounts <FILE>, a JSONL stream of \
                  cloud_account records. Fetching them from the API mid-stream lands in a \
                  later slice.\n\n\
                  Example:\n  \
                  stave list cloud_account > accounts.jsonl\n  \
                  stave list cloud_resource | stave enrich --with account-context \\\n      \
                  --accounts accounts.jsonl"
)]
struct EnrichArgs {
    /// Recipe name. One of: account-context, severity-roll-up, entity-hoist.
    #[arg(long = "with", value_name = "RECIPE")]
    recipe: String,

    /// Auxiliary stream of cloud_account records as JSONL.
    /// Required by `account-context`.
    #[arg(long, value_name = "FILE")]
    accounts: Option<PathBuf>,
}

#[derive(clap::Args, Debug)]
#[command(long_about = "Format a JSON-line stream from stdin.\n\n\
                  Records must follow the stave stream contract (`_kind`, `_source`, then the \
                  record's own fields). With no --format, output is jsonl when stdout is a \
                  pipe and md when stdout is a terminal.\n\n\
                  Formats:\n  \
                  jsonl   one record per line, exact passthrough\n  \
                  md      markdown table (kind, id, severity, primary timestamp)\n  \
                  json    one pretty-printed JSON array of every record")]
struct EmitArgs {
    /// Output format. Defaults to jsonl on a pipe and md on a terminal.
    #[arg(long, value_enum)]
    format: Option<EmitFormat>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum EmitFormat {
    Jsonl,
    Md,
    Json,
}

// ---------------------------------------------------------------------------
// mcp
// ---------------------------------------------------------------------------

#[derive(clap::Args, Debug)]
#[command(long_about = "Operate the hosted Wiz MCP server.\n\n\
                  Wiz publishes a remote MCP server (streamable HTTP, JSON-RPC 2.0) \
                  authenticated with the same OAuth bearer tokens the GraphQL API uses, so \
                  there is no separate MCP credential: the client-ID and client-secret chains \
                  supply it, or STAVE_ACCESS_TOKEN short-circuits the mint.\n\n\
                  The endpoint resolves through STAVE_MCP_URL, then [mcp] url in config, then \
                  the hosted default.\n\n\
                  Subcommands:\n  \
                  status   report the endpoint and whether credentials resolve\n  \
                  tools    list the server's tools live (JSONL)\n  \
                  call     invoke one tool (write-gated unless the name is read-shaped)\n  \
                  config   emit client configuration for wiring stave's endpoint elsewhere\n  \
                  map      crosswalk live tool names to curated operation names")]
struct McpArgs {
    #[command(subcommand)]
    cmd: McpCmd,
}

#[derive(Subcommand, Debug)]
enum McpCmd {
    /// Report the resolved MCP endpoint and credential sources.
    Status,
    /// List the server's tools as a JSON-line stream.
    Tools {
        /// Substring filter on the tool name.
        #[arg(long, value_name = "TEXT")]
        filter: Option<String>,
    },
    /// Invoke one MCP tool.
    Call(McpCallArgs),
    /// Emit MCP client configuration JSON.
    Config {
        /// Include the bearer token instead of a masked placeholder.
        #[arg(long)]
        reveal: bool,
    },
    /// Crosswalk live tool names to curated operation names (JSONL).
    Map,
}

#[derive(clap::Args, Debug)]
struct McpCallArgs {
    /// Tool name. Run `stave mcp tools` to discover.
    tool: String,

    /// Tool arguments as a JSON object.
    #[arg(long, value_name = "JSON")]
    args: Option<String>,
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let cmd = match cli.cmd {
        Some(c) => c,
        None => {
            println!(
                "stave {}\n\nUse `stave --help` for usage.",
                env!("CARGO_PKG_VERSION")
            );
            return ExitCode::SUCCESS;
        }
    };

    let result = match cmd {
        Cmd::Auth(args) => run_auth(args),
        Cmd::Registry(args) => run_registry(args),
        Cmd::Config(args) => run_config(args),
        Cmd::Ops(args) => run_ops(args),
        Cmd::Api(args) => run_api(args),
        Cmd::List(args) => run_list(args),
        Cmd::Search(args) => run_search(args),
        Cmd::Get(args) => run_get(args),
        Cmd::Filter(args) => run_filter(args),
        Cmd::Enrich(args) => run_enrich(args),
        Cmd::Emit(args) => run_emit(args),
        Cmd::Mcp(args) => run_mcp(args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("stave: {e:#}");
            ExitCode::FAILURE
        }
    }
}

// ---------------------------------------------------------------------------
// auth
// ---------------------------------------------------------------------------

fn run_auth(args: AuthArgs) -> anyhow::Result<()> {
    match args.cmd {
        AuthCmd::Login(login) => auth_login(login),
        AuthCmd::Status => auth_status(),
        AuthCmd::Logout => auth_logout(),
        AuthCmd::Scopes => auth_scopes(),
        AuthCmd::CanI { operation } => auth_can_i(&operation),
        AuthCmd::Plan(plan) => auth_plan(plan),
    }
}

fn auth_login(args: AuthLoginArgs) -> anyhow::Result<()> {
    // Client ID first, so the prompts read in the order a person expects
    // and so a --stdin secret is never consumed by an ID prompt.
    let client_id = match args.client_id.as_deref().map(str::trim) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => resolve_login_client_id(args.stdin)?,
    };

    let secret = read_secret(args.stdin, "Wiz client secret: ", "--client-id")?;

    auth::store_client_secret(&secret).map_err(|e| anyhow!("{e}"))?;
    match auth::read_client_secret_keyring() {
        Some(_) => eprintln!("stave auth: client secret stored in the platform keyring"),
        None => eprintln!(
            "stave auth: client secret written, but reading it back failed. The keyring \
             backend may be unavailable; set {} instead.",
            auth::CLIENT_SECRET_ENV
        ),
    }

    let api_url = args
        .api_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let api_url_owned = api_url.map(str::to_string);
    let token_url_arg = args
        .token_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let token_url_owned = token_url_arg.map(str::to_string);
    let client_id_owned = client_id.clone();
    let path = auth::write_config(|cfg| {
        cfg.auth.client_id = Some(client_id_owned);
        if let Some(url) = api_url_owned {
            cfg.default.api_url = Some(url);
        }
        if let Some(url) = token_url_owned {
            cfg.auth.token_url = Some(url);
        }
    })
    .map_err(|e| anyhow!("{e}"))?;
    let mut persisted = vec!["client_id"];
    if api_url.is_some() {
        persisted.push("api_url");
    }
    if token_url_arg.is_some() {
        persisted.push("token_url");
    }
    eprintln!(
        "stave auth: persisted {} to {}",
        persisted.join(" + "),
        path.display()
    );

    // A token minted from the previous secret must not outlive it.
    token::clear_cache().map_err(|e| anyhow!("{e}"))?;

    if args.no_verify {
        eprintln!("stave auth: skipped verification (--no-verify)");
        return Ok(());
    }

    let token_url = auth::resolve_token_url(token_url_arg).map_err(|e| anyhow!("{e}"))?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("build HTTP client for the token mint")?;
    let minted = match block_on(token::mint(&http, &token_url.value, &client_id, &secret)) {
        Ok(minted) => minted,
        Err(e) => {
            eprintln!(
                "stave auth: credentials were stored. Verification failed, so fix the \
                 credentials or the token endpoint and rerun `stave auth login`."
            );
            return Err(anyhow!("{e}"));
        }
    };

    eprintln!(
        "stave auth: token minted from {} (expires {})",
        token_url.value,
        minted.expires_at.to_rfc3339()
    );
    let endpoint = match auth::resolve_api_url(api_url).map_err(|e| anyhow!("{e}"))? {
        Some(resolved) => format!("{} (source: {})", resolved.value, resolved.source.as_str()),
        None => match token::dc_claim(&minted.access_token) {
            Some(dc) => format!("{} (source: derived)", auth::api_url_from_dc(&dc)),
            None => format!(
                "unresolved. The minted token carries no data-center claim, so set \
                 {}=<url> or run `stave config set api_url <url>`.",
                auth::API_URL_ENV
            ),
        },
    };
    eprintln!("stave auth: endpoint {endpoint}");
    Ok(())
}

/// Client ID for `auth login` when the flag is absent: prompt on a
/// terminal, otherwise fall back to the env and config layers of the
/// chain. Never prompts when the secret is arriving on stdin.
fn resolve_login_client_id(secret_on_stdin: bool) -> anyhow::Result<String> {
    if !secret_on_stdin && std::io::stdin().is_terminal() {
        let id = prompt_line("Wiz client ID: ")?;
        if !id.is_empty() {
            return Ok(id);
        }
        return Err(anyhow!("client ID must not be empty"));
    }
    match auth::resolve_client_id(None).map_err(|e| anyhow!("{e}"))? {
        Some(resolved) => {
            eprintln!(
                "stave auth: using client ID from {}",
                resolved.source.as_str()
            );
            Ok(resolved.value)
        }
        None => Err(anyhow!(
            "no client ID. Pass --client-id <id>, set {}=<id>, or run \
             `stave config set client_id <id>`. On a terminal, `stave auth login` \
             prompts for it.",
            auth::CLIENT_ID_ENV
        )),
    }
}

fn auth_status() -> anyhow::Result<()> {
    let client_id = auth::resolve_client_id(None).map_err(|e| anyhow!("{e}"))?;
    let client_secret = auth::resolve_client_secret().map_err(|e| anyhow!("{e}"))?;

    let client_id_line = match &client_id {
        Some(r) => format!("{} (source: {})", r.value, r.source.as_str()),
        None => format!(
            "unset. Pass --client-id, set {}, or run `stave config set client_id <id>`",
            auth::CLIENT_ID_ENV
        ),
    };
    let secret_line = match &client_secret {
        Some(r) => format!(
            "present (source: {}, length: {} bytes)",
            r.source.as_str(),
            r.value.len()
        ),
        None => format!(
            "unset. Run `stave auth login` or set {}",
            auth::CLIENT_SECRET_ENV
        ),
    };

    let fields = vec![
        ("client_id", client_id_line),
        ("client_secret", secret_line),
        ("api_url", api_url_status()),
        ("token_url", token_url_status()),
        ("token_cache", token_cache_status()),
        ("writes", writes_status()),
        ("posture", posture_status()),
        ("audit_dir", audit_dir_status()),
        ("config", config_path_status()),
    ];
    emit_status(&fields)?;

    if client_id.is_none() || client_secret.is_none() {
        std::process::exit(1);
    }
    Ok(())
}

fn auth_logout() -> anyhow::Result<()> {
    let secret_removed = auth::delete_client_secret_keyring().map_err(|e| anyhow!("{e}"))?;
    if secret_removed {
        eprintln!("stave auth: client secret removed from the keyring");
    } else {
        eprintln!("stave auth: no keyring entry to remove");
    }
    let cache_removed = token::clear_cache().map_err(|e| anyhow!("{e}"))?;
    if cache_removed {
        eprintln!("stave auth: cached token removed");
    } else {
        eprintln!("stave auth: no cached token to remove");
    }
    eprintln!(
        "stave auth: the config file is untouched. Clear persisted values with \
         `stave config unset client_id` / `stave config unset api_url`."
    );
    Ok(())
}

/// The OAuth token endpoint as `auth status` sees it. The chain
/// bottoms out at the built-in default, so this always resolves;
/// the source tells the user whether they overrode it.
fn token_url_status() -> String {
    match auth::resolve_token_url(None) {
        Ok(r) => format!("{} (source: {})", r.value, r.source.as_str()),
        Err(e) => format!("error ({e})"),
    }
}

/// The endpoint chain as `auth status` sees it, including the derivation
/// layer: with no explicit source, a token already at hand (env or the
/// cache) names the tenant's data center.
fn api_url_status() -> String {
    match auth::resolve_api_url(None) {
        Ok(Some(r)) => format!("{} (source: {})", r.value, r.source.as_str()),
        Ok(None) => match available_access_token()
            .as_deref()
            .and_then(token::dc_claim)
        {
            Some(dc) => format!("{} (source: derived)", auth::api_url_from_dc(&dc)),
            None => format!(
                "unset. Set {}=<url>, run `stave config set api_url <url>`, or mint a \
                 token so its data-center claim can supply it",
                auth::API_URL_ENV
            ),
        },
        Err(e) => format!("error ({e})"),
    }
}

fn token_cache_status() -> String {
    let Some(path) = token::cache_path() else {
        return "no discoverable cache path".to_string();
    };
    let display = path.display();
    match std::fs::read_to_string(&path) {
        Ok(body) => match serde_json::from_str::<Value>(&body) {
            Ok(parsed) => {
                let expires = parsed
                    .get("expires_at")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                format!("cached, expires {expires} ({display})")
            }
            Err(_) => format!("unreadable, will be replaced on the next mint ({display})"),
        },
        Err(_) => format!("empty ({display})"),
    }
}

fn writes_status() -> String {
    // D1: mutations are refused unconditionally. There is no opt-in to
    // report; this line exists so `auth status` states the posture
    // plainly rather than leaving it implied.
    "refused (read-only against live tenants; not configurable)".to_string()
}

fn posture_status() -> String {
    // D11: the read posture governs ad-hoc documents only.
    match auth::resolve_posture() {
        Ok(p) => match p {
            auth::Posture::Curated => {
                "curated (curated operations only; ad-hoc --query refused)".to_string()
            }
            auth::Posture::Exploratory => {
                "exploratory (ad-hoc --query permitted; mutations still refused)".to_string()
            }
        },
        Err(e) => format!("error ({e})"),
    }
}

fn audit_dir_status() -> String {
    match audit::audit_dir() {
        Some(dir) => dir.display().to_string(),
        None => "disabled (STAVE_AUDIT=off)".to_string(),
    }
}

fn config_path_status() -> String {
    match auth::config_path() {
        Some(path) => path.display().to_string(),
        None => "no discoverable path (set XDG_CONFIG_HOME, HOME, or STAVE_CONFIG)".to_string(),
    }
}

/// A bearer token already in hand, without minting one: the env
/// short-circuit first, then the local cache. Used for endpoint
/// derivation in `auth status`, which must not make network calls.
fn available_access_token() -> Option<String> {
    if let Ok(v) = std::env::var(ACCESS_TOKEN_ENV) {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    let path = token::cache_path()?;
    let body = std::fs::read_to_string(path).ok()?;
    let parsed: Value = serde_json::from_str(&body).ok()?;
    parsed
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// registry
// ---------------------------------------------------------------------------

fn run_registry(args: RegistryArgs) -> anyhow::Result<()> {
    match args.cmd {
        RegistryCmd::Login(login) => registry_login(login),
        RegistryCmd::Status => registry_status(),
        RegistryCmd::Credential { reveal } => registry_credential(reveal),
        RegistryCmd::Logout => registry_logout(),
    }
}

fn registry_login(args: RegistryLoginArgs) -> anyhow::Result<()> {
    let password = read_secret(args.stdin, "Registry password: ", "--host / --username")?;
    auth::store_registry_password(&password).map_err(|e| anyhow!("{e}"))?;
    eprintln!("stave registry: password stored in the platform keyring");

    let host = args
        .host
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let username = args
        .username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if host.is_none() && username.is_none() {
        return Ok(());
    }

    let host_owned = host.map(str::to_string);
    let username_owned = username.map(str::to_string);
    let path = auth::write_config(|cfg| {
        if let Some(h) = host_owned {
            cfg.registry.host = Some(h);
        }
        if let Some(u) = username_owned {
            cfg.registry.username = Some(u);
        }
    })
    .map_err(|e| anyhow!("{e}"))?;
    let mut persisted: Vec<&str> = Vec::new();
    if host.is_some() {
        persisted.push("registry.host");
    }
    if username.is_some() {
        persisted.push("registry.username");
    }
    eprintln!(
        "stave registry: persisted {} to {}",
        persisted.join(" + "),
        path.display()
    );
    Ok(())
}

fn registry_status() -> anyhow::Result<()> {
    let cfg = auth::read_config().map_err(|e| anyhow!("{e}"))?;
    let host = cfg
        .as_ref()
        .and_then(|c| c.registry.host.clone())
        .unwrap_or_else(|| "unset (run `stave registry login --host <host>`)".to_string());
    let username = cfg
        .as_ref()
        .and_then(|c| c.registry.username.clone())
        .unwrap_or_else(|| "unset (run `stave registry login --username <user>`)".to_string());
    let password = match auth::resolve_registry_password().map_err(|e| anyhow!("{e}"))? {
        Some(r) => format!(
            "present (source: {}, length: {} bytes)",
            r.source.as_str(),
            r.value.len()
        ),
        None => format!(
            "unset. Run `stave registry login` or set {}",
            auth::REGISTRY_PASSWORD_ENV
        ),
    };
    emit_status(&[
        ("registry_host", host),
        ("registry_username", username),
        ("registry_password", password),
    ])
}

fn registry_credential(reveal: bool) -> anyhow::Result<()> {
    let resolved = auth::resolve_registry_password()
        .map_err(|e| anyhow!("{e}"))?
        .ok_or_else(|| {
            anyhow!(
                "no registry password resolved through any layer of the chain. Set {}=<value>, \
                 or run `stave registry login --stdin`.",
                auth::REGISTRY_PASSWORD_ENV
            )
        })?;

    let cfg = auth::read_config().map_err(|e| anyhow!("{e}"))?;
    let host = cfg
        .as_ref()
        .and_then(|c| c.registry.host.clone())
        .unwrap_or_else(|| "<registry-host>".to_string());
    let username = cfg
        .as_ref()
        .and_then(|c| c.registry.username.clone())
        .unwrap_or_else(|| "<registry-username>".to_string());

    if reveal {
        println!("{}", resolved.value);
        eprintln!(
            "stave registry: password written to stdout. Pipe it, never paste it: \
             stave registry credential --reveal | docker login {host} -u {username} \
             --password-stdin"
        );
        return Ok(());
    }

    println!("{}", mask(resolved.value.len()));
    eprintln!(
        "stave registry: masked. To log a container runtime in:\n  \
         stave registry credential --reveal | docker login {host} -u {username} \
         --password-stdin\n\
         Source: {}. The username is tenant-identifying; keep it out of commits and \
         shared logs.",
        resolved.source.as_str()
    );
    Ok(())
}

fn registry_logout() -> anyhow::Result<()> {
    let removed = auth::delete_registry_keyring().map_err(|e| anyhow!("{e}"))?;
    if removed {
        eprintln!("stave registry: password removed from the keyring");
    } else {
        eprintln!("stave registry: no keyring entry to remove");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// config
// ---------------------------------------------------------------------------

fn run_config(args: ConfigArgs) -> anyhow::Result<()> {
    match args.cmd {
        ConfigCmd::Show => config_show(),
        ConfigCmd::Path => config_path_cmd(),
        ConfigCmd::Set { key, value } => config_set(&key, &value),
        ConfigCmd::Unset { key } => config_unset(&key),
    }
}

fn config_show() -> anyhow::Result<()> {
    let path = auth::config_path()
        .ok_or_else(|| anyhow!("no discoverable config path (set XDG_CONFIG_HOME or HOME)"))?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "# {}", path.display())?;

    let Some(mut cfg) = auth::read_config().map_err(|e| anyhow!("{e}"))? else {
        writeln!(stdout, "# (file does not exist yet)")?;
        return Ok(());
    };

    // Secrets are shown as shapes, never values. A config file that
    // carries them at all is already the discouraged path.
    if let Some(secret) = &cfg.auth.client_secret {
        cfg.auth.client_secret = Some(mask(secret.len()));
    }
    if let Some(password) = &cfg.registry.password {
        cfg.registry.password = Some(mask(password.len()));
    }

    let body = toml::to_string_pretty(&cfg).context("serialize config as TOML")?;
    if body.trim().is_empty() {
        writeln!(stdout, "# (no values set)")?;
    } else {
        write!(stdout, "{body}")?;
    }
    Ok(())
}

fn config_path_cmd() -> anyhow::Result<()> {
    let path = auth::config_path()
        .ok_or_else(|| anyhow!("no discoverable config path (set XDG_CONFIG_HOME or HOME)"))?;
    println!("{}", path.display());
    Ok(())
}

fn config_set(key: &str, value: &str) -> anyhow::Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(anyhow!(
            "value must not be empty. Use `stave config unset {key}` to clear it."
        ));
    }
    let owned = trimmed.to_string();
    let path = match key {
        "client_id" => auth::write_config(|cfg| cfg.auth.client_id = Some(owned)),
        "api_url" => auth::write_config(|cfg| cfg.default.api_url = Some(owned)),
        "token_url" => auth::write_config(|cfg| cfg.auth.token_url = Some(owned)),
        "posture" => {
            match trimmed {
                "curated" | "exploratory" => {}
                other => {
                    return Err(anyhow!(
                        "posture must be `curated` or `exploratory`, got {other:?}"
                    ));
                }
            }
            auth::write_config(|cfg| cfg.default.posture = Some(owned))
        }
        "mcp.url" => auth::write_config(|cfg| cfg.mcp.url = Some(owned)),
        "registry.host" => auth::write_config(|cfg| cfg.registry.host = Some(owned)),
        "registry.username" => auth::write_config(|cfg| cfg.registry.username = Some(owned)),
        "client_secret" | "auth.client_secret" | "registry.password" => {
            return Err(anyhow!(
                "'{key}' is a secret and is not settable here. Run `stave auth login` or \
                 `stave registry login` so it lands in the platform keyring."
            ));
        }
        other => {
            return Err(anyhow!("unknown key '{other}'. Known keys: {CONFIG_KEYS}."));
        }
    }
    .map_err(|e| anyhow!("{e}"))?;
    eprintln!("stave config: set {key} in {}", path.display());
    Ok(())
}

fn config_unset(key: &str) -> anyhow::Result<()> {
    let path = match key {
        "client_id" => auth::write_config(|cfg| cfg.auth.client_id = None),
        "api_url" => auth::write_config(|cfg| cfg.default.api_url = None),
        "token_url" => auth::write_config(|cfg| cfg.auth.token_url = None),
        "posture" => auth::write_config(|cfg| cfg.default.posture = None),
        "mcp.url" => auth::write_config(|cfg| cfg.mcp.url = None),
        "registry.host" => auth::write_config(|cfg| cfg.registry.host = None),
        "registry.username" => auth::write_config(|cfg| cfg.registry.username = None),
        "client_secret" | "auth.client_secret" => {
            auth::write_config(|cfg| cfg.auth.client_secret = None)
        }
        "registry.password" => auth::write_config(|cfg| cfg.registry.password = None),
        other => {
            return Err(anyhow!("unknown key '{other}'. Known keys: {CONFIG_KEYS}."));
        }
    }
    .map_err(|e| anyhow!("{e}"))?;
    eprintln!("stave config: cleared {key} in {}", path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// ops
// ---------------------------------------------------------------------------

fn run_ops(args: OpsArgs) -> anyhow::Result<()> {
    match args.cmd {
        OpsCmd::List { filter } => ops_list(filter.as_deref()),
        OpsCmd::Show { name } => ops_show(&name),
        OpsCmd::Permissions { filter } => ops_permissions(filter.as_deref()),
    }
}

fn ops_list(filter: Option<&str>) -> anyhow::Result<()> {
    let needle = filter.map(str::to_lowercase);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for (idx, op) in ops::all().iter().enumerate() {
        if let Some(n) = &needle {
            if !op.name.to_lowercase().contains(n) {
                continue;
            }
        }
        let record = Record::wrap(
            "operation",
            SourceRef::now("ops:list", idx),
            json!({
                "name": op.name,
                "op_type": op.op_type.as_str(),
                "root_field": op.root_field,
                "description": op.description,
            }),
        );
        write_record(&mut out, &record).map_err(|e| anyhow!("{e}"))?;
    }
    Ok(())
}

fn ops_show(name: &str) -> anyhow::Result<()> {
    let op = ops::find(name).map_err(|e| anyhow!("{e}"))?;
    // The document is the contract, so it owns stdout: `stave ops show
    // list_issues > my-query.graphql` should produce a runnable file.
    eprintln!(
        "stave ops: {} ({}, root field {}) {}",
        op.name,
        op.op_type.as_str(),
        op.root_field,
        op.description
    );
    print!("{}", op.document);
    if !op.document.ends_with('\n') {
        println!();
    }
    Ok(())
}

/// `stave ops permissions` — static registry report (D5). Offline; no
/// token, no API call. Scope names are provisional (D3) until F1.
fn ops_permissions(filter: Option<&str>) -> anyhow::Result<()> {
    let needle = filter.map(str::to_lowercase);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for (idx, op) in ops::all().iter().enumerate() {
        if let Some(n) = &needle {
            if !op.name.to_lowercase().contains(n) {
                continue;
            }
        }
        let effects = op.effects.map(|e| {
            json!({
                "reversibility": reversibility_str(e.reversibility),
                "side_effects": side_effects_str(e.side_effects),
                "egress": egress_str(e.egress),
            })
        });
        let record = Record::wrap(
            "operation_permissions",
            SourceRef::now("ops:permissions", idx),
            json!({
                "name": op.name,
                "op_type": op.op_type.as_str(),
                "required_scopes": op.required_scopes,
                "sensitivity": op.sensitivity.as_str(),
                "cost_hint": op.cost_hint.as_str(),
                "effects": effects,
                "scopes_provisional": SCOPE_METADATA_PROVISIONAL,
            }),
        );
        write_record(&mut out, &record).map_err(|e| anyhow!("{e}"))?;
    }
    Ok(())
}

fn reversibility_str(r: stave_sdk::Reversibility) -> &'static str {
    match r {
        stave_sdk::Reversibility::Reversible => "reversible",
        stave_sdk::Reversibility::Irreversible => "irreversible",
        stave_sdk::Reversibility::Unknown => "unknown",
    }
}

fn side_effects_str(s: stave_sdk::SideEffects) -> &'static str {
    match s {
        stave_sdk::SideEffects::None => "none",
        stave_sdk::SideEffects::Notifies => "notifies",
        stave_sdk::SideEffects::TriggersIntegrations => "triggers-integrations",
        stave_sdk::SideEffects::Unknown => "unknown",
    }
}

fn egress_str(e: stave_sdk::Egress) -> &'static str {
    match e {
        stave_sdk::Egress::None => "none",
        stave_sdk::Egress::ProducesEgressArtifact => "produces-egress-artifact",
        stave_sdk::Egress::Unknown => "unknown",
    }
}

/// The scope information the token at hand carries. Never mints —
/// reads env or the cache only (`stave auth scopes`). `Absent` when no
/// token is available at all.
fn resolved_token_scopes() -> stave_sdk::TokenScopes {
    match available_access_token() {
        Some(token) => token_scopes(&token),
        None => stave_sdk::TokenScopes::Absent,
    }
}

/// The message printed when a real Wiz token is present but its scopes
/// are opaque (F1: `encodedScopes` bitmask). Enumeration and
/// grant-checking are impossible client-side; provisioning still works
/// statically via `auth plan`.
const OPAQUE_SCOPES_NOTE: &str = "this tenant's token encodes scopes as an opaque bitmask (encodedScopes), \
     so stave cannot enumerate or check granted scopes from the token. Use \
     `stave auth plan` for the least-privilege provisioning checklist, and set \
     the service account's scopes in the Wiz portal.";

/// `read:all` is treated as granting any `read:*` scope. Provisional
/// rule (D3) until F1 confirms Wiz's scope-implication semantics.
fn scope_granted(required: &str, granted: &[String]) -> bool {
    if granted.iter().any(|g| g == required) {
        return true;
    }
    if required.starts_with("read:") && granted.iter().any(|g| g == "read:all") {
        return true;
    }
    false
}

fn auth_scopes() -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match resolved_token_scopes() {
        stave_sdk::TokenScopes::Readable { scopes, field } => {
            let record = json!({
                "scopes": scopes,
                "claim_field": field,
                "provisional": SCOPE_METADATA_PROVISIONAL,
            });
            writeln!(out, "{record}").map_err(|e| anyhow!("{e}"))?;
            Ok(())
        }
        stave_sdk::TokenScopes::Opaque { field } => {
            let record = json!({
                "scopes": Value::Null,
                "claim_field": field,
                "enumerable": false,
                "note": OPAQUE_SCOPES_NOTE,
            });
            writeln!(out, "{record}").map_err(|e| anyhow!("{e}"))?;
            Ok(())
        }
        stave_sdk::TokenScopes::Absent => Err(anyhow!(
            "no token scopes available. Provide a token first: `stave auth login` (mints \
             one), or set {}.",
            ACCESS_TOKEN_ENV
        )),
    }
}

fn auth_can_i(operation: &str) -> anyhow::Result<()> {
    let op = ops::find(operation).map_err(|e| anyhow!("{e}"))?;
    let granted = match resolved_token_scopes() {
        stave_sdk::TokenScopes::Readable { scopes, .. } => scopes,
        // Never report a false "no": if scopes are opaque we cannot
        // decide grant membership, so say exactly that.
        stave_sdk::TokenScopes::Opaque { .. } => {
            return Err(anyhow!(
                "cannot determine whether '{}' is permitted: {OPAQUE_SCOPES_NOTE}",
                op.name
            ));
        }
        stave_sdk::TokenScopes::Absent => {
            return Err(anyhow!(
                "no token scopes available to check against. Run `stave auth login` or set {}.",
                ACCESS_TOKEN_ENV
            ));
        }
    };
    let missing: Vec<&str> = op
        .required_scopes
        .iter()
        .copied()
        .filter(|s| !scope_granted(s, &granted))
        .collect();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let allowed = missing.is_empty();
    let record = json!({
        "operation": op.name,
        "allowed": allowed,
        "required_scopes": op.required_scopes,
        "missing_scopes": missing,
        "provisional": SCOPE_METADATA_PROVISIONAL,
    });
    writeln!(out, "{record}").map_err(|e| anyhow!("{e}"))?;
    if allowed {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn auth_plan(args: AuthPlanArgs) -> anyhow::Result<()> {
    // Resolve the selected operations (default: all curated).
    let selected: Vec<&'static ops::OperationDoc> = if args.ops.is_empty() {
        ops::all().iter().collect()
    } else {
        let mut v = Vec::new();
        for name in &args.ops {
            v.push(ops::find(name).map_err(|e| anyhow!("{e}"))?);
        }
        v
    };

    // Least-privilege union of required scopes.
    let mut grant: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for op in &selected {
        for s in op.required_scopes {
            grant.insert((*s).to_string());
        }
    }

    // DO NOT GRANT: scopes present anywhere in the registry that the
    // selected set does not require (future/other-verb scopes).
    let mut all_scopes: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for op in ops::all() {
        for s in op.required_scopes {
            all_scopes.insert((*s).to_string());
        }
    }
    let do_not_grant: Vec<String> = all_scopes.difference(&grant).cloned().collect();
    let grant_vec: Vec<String> = grant.iter().cloned().collect();

    if args.check {
        return auth_plan_check(&grant_vec);
    }

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let record = json!({
        "grant": grant_vec,
        "do_not_grant": do_not_grant,
        "operations": selected.iter().map(|o| o.name).collect::<Vec<_>>(),
        "provisional": SCOPE_METADATA_PROVISIONAL,
        "notes": [
            "Create a Custom Integration (GraphQL API) service account in the Wiz portal \
             (Settings -> Access Management -> Service Accounts) with exactly the GRANT scopes.",
            "Prefer the enumerated GRANT set over a read:all bundle.",
            "DO NOT GRANT: withholding these is the read-only boundary. When a write verb is \
             ever needed, provision a SEPARATE write-scoped service account; never widen this \
             read-only one.",
            "Scope names are provisional until validated against a live tenant."
        ],
    });
    // On a TTY, present the checklist readably; otherwise JSON.
    if out.is_terminal() {
        writeln!(out, "GRANT (least-privilege, for the selected operations):")?;
        for s in &grant_vec {
            writeln!(out, "  {s}")?;
        }
        writeln!(out, "\nDO NOT GRANT (withhold; the read-only boundary):")?;
        if do_not_grant.is_empty() {
            writeln!(
                out,
                "  (none — every registry scope is required by the selection)"
            )?;
        }
        for s in &do_not_grant {
            writeln!(out, "  {s}")?;
        }
        writeln!(
            out,
            "\nProvision a Custom Integration (GraphQL API) service account with exactly the \
             GRANT scopes. When a write verb is ever needed, provision a SEPARATE write-scoped \
             account; never widen this one. Scope names are provisional until live validation."
        )?;
    } else {
        writeln!(out, "{record}")?;
    }
    Ok(())
}

/// `auth plan --check`: compare the token's scopes against the
/// requirement, reporting MISSING (unusable) vs EXCESS
/// (over-privileged) separately. Exits nonzero on any drift.
fn auth_plan_check(required: &[String]) -> anyhow::Result<()> {
    let granted = match resolved_token_scopes() {
        stave_sdk::TokenScopes::Readable { scopes, .. } => scopes,
        // Cannot compare against an opaque bitmask without reporting a
        // false drift. Say so, and exit nonzero (the check did not pass).
        stave_sdk::TokenScopes::Opaque { .. } => {
            return Err(anyhow!(
                "cannot check the credential against the requirement: {OPAQUE_SCOPES_NOTE} \
                 The GRANT set to provision is: {}",
                required.join(", ")
            ));
        }
        stave_sdk::TokenScopes::Absent => {
            return Err(anyhow!(
                "no token scopes available to check against. Run `stave auth login` or set {}.",
                ACCESS_TOKEN_ENV
            ));
        }
    };
    let missing: Vec<String> = required
        .iter()
        .filter(|s| !scope_granted(s, &granted))
        .cloned()
        .collect();
    // Excess: a granted scope that no required scope matches. read:all
    // is a deliberate bundle, but for the purpose of least-privilege
    // reporting it is still "more than required" when the selection
    // does not literally list it.
    let excess: Vec<String> = granted
        .iter()
        .filter(|g| !required.iter().any(|r| r == *g))
        .cloned()
        .collect();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let record = json!({
        "required": required,
        "granted": granted,
        "missing": missing,
        "excess": excess,
        "provisional": SCOPE_METADATA_PROVISIONAL,
    });
    writeln!(out, "{record}").map_err(|e| anyhow!("{e}"))?;
    if missing.is_empty() && excess.is_empty() {
        Ok(())
    } else {
        // Nonzero on any drift; the record above distinguishes the two
        // directions (missing = unusable, excess = over-privileged).
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// api
// ---------------------------------------------------------------------------

fn run_api(args: ApiArgs) -> anyhow::Result<()> {
    let variables = merge_variables(args.vars.as_deref(), &args.var)?;
    let posture = auth::resolve_posture().map_err(|e| anyhow!("{e}"))?;
    // D11: ad-hoc documents (--query) run only under the exploratory
    // posture. Curated operations (by name) always run. Mutations are
    // refused unconditionally regardless of posture, in the SDK.
    if args.query.is_some() && posture != auth::Posture::Curated {
        // exploratory: allowed to proceed
    } else if args.query.is_some() {
        return Err(anyhow!(
            "read posture is `curated`: ad-hoc GraphQL documents (--query) are refused. \
             Run a curated operation by name (`stave ops list`), or deliberately enter the \
             exploratory read posture with `stave config set posture exploratory`."
        ));
    }
    let opts = CallOptions {
        no_audit: args.no_audit,
        verb_phase: Some("api"),
        posture: args.query.as_ref().map(|_| posture.as_str().to_string()),
        ..Default::default()
    };

    let data = block_on(async {
        let client = Client::from_env_with_api_url(args.api_url.as_deref()).await?;
        match (&args.name, &args.query) {
            (Some(name), _) => client.call_op(name, &variables, opts).await,
            (None, Some(source)) => {
                let document = read_document(source)?;
                client.call_document(&document, &variables, opts).await
            }
            // clap's ArgGroup makes this unreachable; keep it honest
            // rather than panicking.
            (None, None) => Err(stave_sdk::StaveError::InvalidParam(
                "api".into(),
                "pass an operation name or --query <file|->".into(),
            )),
        }
    })
    .map_err(|e| anyhow!("{e}"))?;

    let pretty = serde_json::to_string_pretty(&data).context("serialize response as JSON")?;
    println!("{pretty}");
    Ok(())
}

// ---------------------------------------------------------------------------
// list / search / get
// ---------------------------------------------------------------------------

fn run_list(args: ListArgs) -> anyhow::Result<()> {
    let spec = lookup_kind(&args.kind)?;
    // Validate --since before any network call so a format error never
    // costs an API request.
    let since = build_since_program(spec, args.since.as_deref())?;
    let now = chrono_now();

    block_on(stream_kind(
        spec,
        args.api_url.as_deref(),
        args.no_audit,
        "list",
        args.limit,
        // `--since` filters client-side, so with one in play the page
        // size must not be derived from the limit.
        since.is_some(),
        |record| match &since {
            None => Ok(true),
            Some(program) => cel::evaluate(program, record, now, "<--since predicate>")
                .map_err(|e| anyhow!("{e}")),
        },
    ))
}

fn run_search(args: SearchArgs) -> anyhow::Result<()> {
    let spec = lookup_kind(&args.kind)?;
    let search_field = spec.search_field.ok_or_else(|| {
        anyhow!(
            "kind '{}' declares no search field, so `search` has nothing to match against. \
             Compose `stave list {} | stave filter --where '<CEL>'` instead.",
            spec.name,
            spec.name
        )
    })?;
    let needle = args.query.to_lowercase();

    block_on(stream_kind(
        spec,
        args.api_url.as_deref(),
        args.no_audit,
        "search",
        args.limit,
        // Every search is a client-side substring pass (charter F2).
        true,
        |record| {
            let Some(haystack) = record.get(search_field).and_then(Value::as_str) else {
                return Ok(false);
            };
            Ok(haystack.to_lowercase().contains(&needle))
        },
    ))
}

fn run_get(args: GetArgs) -> anyhow::Result<()> {
    Err(anyhow!(
        "`stave get` is not supported in v0.1. Singular lookups need per-kind singular \
         queries or filter input types, and stave does not guess at schema shapes \
         (charter F2, resolved by schema introspection).\n\
         Two ways through today:\n  \
         1. write the document: `stave api --query ./{kind}-by-id.graphql --var id={id}`\n  \
         2. select client-side: `stave list {kind} --limit 500 | stave filter --where \
         '{id_field} == \"{id}\"'`",
        kind = args.kind,
        id = args.id,
        id_field = kind_spec(&args.kind).map_or("id", |s| s.id_field),
    ))
}

/// Page one kind's curated list operation, wrapping each node as a
/// stream record and writing the ones `keep` accepts. All pages of a
/// single invocation share one audit trace ID so a miner can see the
/// read as one logical operation.
async fn stream_kind(
    spec: &'static KindSpec,
    api_url: Option<&str>,
    no_audit: bool,
    verb_phase: &'static str,
    limit: usize,
    filtered: bool,
    mut keep: impl FnMut(&Record) -> anyhow::Result<bool>,
) -> anyhow::Result<()> {
    let op = ops::find(spec.list_operation).map_err(|e| anyhow!("{e}"))?;
    let client = Client::from_env_with_api_url(api_url)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    let trace_id = Uuid::now_v7();

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    let mut emitted = 0usize;
    let mut index = 0usize;
    let mut after: Option<String> = None;

    let mut empty_pages = 0usize;

    while emitted < limit {
        // Page size is a fetch concern; `--limit` is an output concern.
        // With no predicate the two coincide, because every fetched
        // record is emitted, and asking for exactly what is left is the
        // cheapest correct request.
        //
        // With a predicate they are unrelated. `emitted` counts records
        // that PASSED `keep`, so sizing the request from the remaining
        // limit pins every page to `--limit` however large the
        // connection is: `search cloud_resource <rare> --limit 5` walked
        // a twenty-thousand-record connection five records per HTTP
        // request. Fetch whole pages whenever a predicate is filtering.
        let page = if filtered {
            MAX_PAGE_SIZE
        } else {
            limit.saturating_sub(emitted).min(MAX_PAGE_SIZE)
        };
        let mut variables = Map::new();
        variables.insert("first".to_string(), json!(page));
        if let Some(cursor) = &after {
            variables.insert("after".to_string(), json!(cursor));
        }
        let variables = Value::Object(variables);

        let opts = CallOptions {
            trace_id: Some(trace_id),
            no_audit,
            verb_phase: Some(verb_phase),
            synthesis_keys: vec![spec.id_field.to_string()],
            ..Default::default()
        };
        let data = client
            .call_op(spec.list_operation, &variables, opts)
            .await
            .map_err(|e| anyhow!("{e}"))?;

        let items: Vec<Value> = kinds::extract_items(&data, Some(op.root_field))
            .map(<[Value]>::to_vec)
            .unwrap_or_default();
        let page_len = items.len();

        for item in items {
            let record = Record::wrap(spec.name, SourceRef::now(spec.list_operation, index), item);
            index += 1;
            if !keep(&record)? {
                continue;
            }
            write_record(&mut out, &record).map_err(|e| anyhow!("{e}"))?;
            emitted += 1;
            if emitted >= limit {
                break;
            }
        }

        match next_cursor(&data, op.root_field) {
            // A page can be empty while the connection has more to
            // give: server-side filtering, permission scoping, and
            // deleted rows all produce a page of zero nodes with
            // `hasNextPage: true`. Following the cursor is the only
            // correct move. Stopping here reported a short read as a
            // complete one, exit 0, nothing on stderr, and a caller
            // counting records got a smaller number than the truth.
            Some(cursor) => {
                if after.as_deref() == Some(cursor.as_str()) {
                    // The cursor did not advance, so following it again
                    // repeats this request forever. THIS is the
                    // termination hazard, and it is a property of the
                    // cursor, never of the page being empty.
                    eprintln!(
                        "stave: {} returned the same cursor twice; stopping after {emitted} \
                         record(s). The read is incomplete.",
                        op.root_field
                    );
                    break;
                }
                if page_len == 0 {
                    empty_pages += 1;
                    if empty_pages > MAX_EMPTY_PAGES {
                        eprintln!(
                            "stave: {} returned {MAX_EMPTY_PAGES} consecutive empty pages; \
                             stopping after {emitted} record(s). The read is incomplete.",
                            op.root_field
                        );
                        break;
                    }
                    eprintln!(
                        "stave: {} returned an empty page with more pages available; following \
                         the cursor.",
                        op.root_field
                    );
                } else {
                    empty_pages = 0;
                }
                after = Some(cursor);
            }
            None => break,
        }
    }
    Ok(())
}

/// The connection's `endCursor`, but only while it has another page.
fn next_cursor(data: &Value, root_field: &str) -> Option<String> {
    let page_info = data.get(root_field)?.get("pageInfo")?;
    if page_info.get("hasNextPage").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    page_info
        .get("endCursor")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn lookup_kind(name: &str) -> anyhow::Result<&'static KindSpec> {
    kind_spec(name).ok_or_else(|| {
        anyhow!(
            "unknown kind '{name}'. The v0.1 set is: {}",
            kinds::ALL_KINDS.join(", ")
        )
    })
}

// ---------------------------------------------------------------------------
// filter
// ---------------------------------------------------------------------------

fn run_filter(args: FilterArgs) -> anyhow::Result<()> {
    let predicate = &args.r#where;
    let program = cel::compile(predicate).map_err(|e| anyhow!("{e}"))?;

    if args.explain {
        return explain_filter(predicate, &program);
    }

    let span = audit::Span::start_fresh().with_verb_phase("filter");
    let ast_shape = predicate_ast_shape(&program);

    let now = chrono_now();
    let stdin = std::io::stdin();
    let stdin = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    let mut kept = 0usize;
    let mut dropped = 0usize;
    let mut errors = 0usize;
    let mut last_error: Option<anyhow::Error> = None;

    for record in read_stream(stdin) {
        let record = match record {
            Ok(r) => r,
            Err(e) => {
                errors += 1;
                last_error = Some(anyhow!("{e}"));
                break;
            }
        };
        match cel::evaluate(&program, &record, now, predicate) {
            Ok(true) => {
                kept += 1;
                if let Err(e) = write_record(&mut out, &record) {
                    errors += 1;
                    last_error = Some(anyhow!("{e}"));
                    break;
                }
            }
            Ok(false) => {
                dropped += 1;
            }
            Err(e) => {
                errors += 1;
                last_error = Some(anyhow!("{e}"));
                break;
            }
        }
    }

    let mut extra = Map::new();
    extra.insert("predicate_text".into(), json!(predicate));
    extra.insert("predicate_ast_shape".into(), json!(ast_shape));
    extra.insert(
        "predicate_outcome".into(),
        json!({
            "kept_count": kept,
            "dropped_count": dropped,
            "error_count": errors,
        }),
    );
    span.finish_as_verb(extra);

    match last_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn explain_filter(predicate: &str, program: &cel_interpreter::Program) -> anyhow::Result<()> {
    let now = chrono_now();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    writeln!(out, "predicate: {predicate}")?;
    writeln!(out, "now:       {}", now.to_rfc3339())?;
    writeln!(out, "ast:       {program:#?}")?;
    writeln!(out)?;
    writeln!(out, "v0.1 kind schemas (for predicate authoring):")?;
    for spec in kinds::all_kinds() {
        writeln!(
            out,
            "  {:<22}  id={}  severity={}  ts={}",
            spec.name,
            spec.id_field,
            spec.severity_field.unwrap_or("-"),
            spec.primary_timestamp_field.unwrap_or("-"),
        )?;
    }
    writeln!(out)?;
    writeln!(
        out,
        "Bindings per record: each top-level field becomes a CEL variable;"
    )?;
    writeln!(
        out,
        "the whole record is also bound as `record` for `has()` checks."
    )?;
    Ok(())
}

/// Stable, value-independent fingerprint of the parsed CEL program, so
/// audit miners can cluster predicates across runs. Two predicates that
/// compare the same field with the same operator to different values
/// share a hash; change the field or the operator and the hash moves.
///
/// The hash is safe to share: it carries operators and the schema field
/// names the predicate touches, never the values compared against.
/// `predicate_text` on the same audit line carries those values, and is
/// tenant-identifying.
fn predicate_ast_shape(program: &cel_interpreter::Program) -> String {
    use sha2::{Digest, Sha256};
    let debug = format!("{program:?}");
    let stripped = strip_literals(&debug);
    let mut hasher = Sha256::new();
    hasher.update(stripped.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// Fold a CEL program's Debug representation into a shape: literal
/// payloads collapse to a placeholder, as do the numeric AST node IDs
/// that renumber whenever a predicate is edited. Operators and
/// identifiers survive.
///
/// The representation quotes all three (`func_name: "_==_"`,
/// `Ident("severity")`, `Literal(String("CRITICAL"))`), so blanking
/// every quoted run would erase the operator and the field name along
/// with the value. Instead, a quoted run is blanked only inside a
/// `Literal(...)`, tracked by paren depth.
fn strip_literals(s: &str) -> String {
    const LITERAL: &str = "Literal";

    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut depth = 0usize;
    // Depth at which the innermost `Literal(` opened, while inside one.
    let mut literal_depth: Option<usize> = None;
    // The bare word most recently emitted, to recognize `Literal(`.
    let mut last_word = String::new();

    while let Some(c) = chars.next() {
        match c {
            '(' => {
                depth += 1;
                if last_word == LITERAL && literal_depth.is_none() {
                    literal_depth = Some(depth);
                }
                last_word.clear();
                out.push(c);
            }
            ')' => {
                if literal_depth == Some(depth) {
                    literal_depth = None;
                }
                depth = depth.saturating_sub(1);
                last_word.clear();
                out.push(c);
            }
            '"' => {
                last_word.clear();
                let mut quoted = String::new();
                while let Some(nc) = chars.next() {
                    if nc == '\\' {
                        // Keep the escape pair intact so an escaped
                        // quote does not end the run early.
                        quoted.push(nc);
                        if let Some(escaped) = chars.next() {
                            quoted.push(escaped);
                        }
                    } else if nc == '"' {
                        break;
                    } else {
                        quoted.push(nc);
                    }
                }
                out.push('"');
                if literal_depth.is_none() {
                    out.push_str(&quoted);
                }
                out.push('"');
            }
            c if c.is_ascii_digit() => {
                last_word.clear();
                while let Some(&nc) = chars.peek() {
                    if nc.is_ascii_digit() || nc == '.' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push('0');
            }
            other => {
                if other.is_alphanumeric() || other == '_' {
                    last_word.push(other);
                } else {
                    last_word.clear();
                }
                out.push(other);
            }
        }
    }
    out
}

/// Build a CEL post-filter program for `--since <duration>`. Returns
/// `None` when `--since` was not supplied, and errors when the kind has
/// no primary timestamp field, since there is then nothing to compare
/// against.
///
/// The compiled predicate is `<primary_ts_field> > now - duration("<dur>")`,
/// reusing the CEL adapter so timestamp promotion and the `now` binding
/// apply consistently. CEL's `duration()` accepts Go-style durations.
/// cel-rust 0.10 accepts malformed input at compile time and only fails
/// at runtime, so validate here to fail before any network call.
fn build_since_program(
    spec: &KindSpec,
    since: Option<&str>,
) -> anyhow::Result<Option<cel_interpreter::Program>> {
    let Some(dur) = since else {
        return Ok(None);
    };
    let ts_field = spec.primary_timestamp_field.ok_or_else(|| {
        anyhow!(
            "kind '{}' has no primary timestamp field, so `--since` has nothing to compare \
             against. Filter on a field you can name: `stave list {} | stave filter --where \
             '<CEL>'`.",
            spec.name,
            spec.name
        )
    })?;
    if dur.contains('"') {
        return Err(anyhow!("--since must not contain quotes: {dur:?}"));
    }
    if !is_valid_go_duration(dur) {
        return Err(anyhow!(
            "--since: invalid duration {dur:?}. Expected Go style (24h, 30m, 1h30m). \
             Valid units: ns, us, µs, ms, s, m, h."
        ));
    }
    let predicate = format!(r#"{ts_field} > now - duration("{dur}")"#);
    let program = cel::compile(&predicate).map_err(|e| anyhow!("--since: {e}"))?;
    Ok(Some(program))
}

/// Lightweight Go-duration validator. Accepts a non-empty sequence of
/// `<number><unit>` pairs where units are one of `ns`, `us`, `µs`, `ms`,
/// `s`, `m`, `h`. Numbers may carry a single decimal point. Empty input
/// or missing/unknown units fail.
fn is_valid_go_duration(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars().peekable();
    let mut had_pair = false;
    while chars.peek().is_some() {
        let mut saw_digit = false;
        let mut saw_dot = false;
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                saw_digit = true;
                chars.next();
            } else if c == '.' && !saw_dot {
                saw_dot = true;
                chars.next();
            } else {
                break;
            }
        }
        if !saw_digit {
            return false;
        }
        let mut unit = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() || c == 'µ' {
                unit.push(c);
                chars.next();
            } else {
                break;
            }
        }
        match unit.as_str() {
            "ns" | "us" | "µs" | "ms" | "s" | "m" | "h" => had_pair = true,
            _ => return false,
        }
    }
    had_pair
}

// ---------------------------------------------------------------------------
// enrich
// ---------------------------------------------------------------------------

fn run_enrich(args: EnrichArgs) -> anyhow::Result<()> {
    let recipe = enrich::Recipe::parse(&args.recipe).ok_or_else(|| {
        anyhow!(
            "unknown recipe '{}'. Accepted recipes: {}.",
            args.recipe,
            RECIPES.join(", ")
        )
    })?;

    let ctx = build_enrichment_context(args.accounts.as_deref())?;
    ctx.validate_for(recipe).map_err(|e| anyhow!("{e}"))?;

    let span = audit::Span::start_fresh().with_verb_phase("enrich");

    let stdin = std::io::stdin();
    let stdin = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    let mut transformed = 0usize;
    let mut errors = 0usize;
    let mut last_error: Option<anyhow::Error> = None;

    for record in read_stream(stdin) {
        let record = match record {
            Ok(r) => r,
            Err(e) => {
                errors += 1;
                last_error = Some(anyhow!("{e}"));
                break;
            }
        };
        let enriched = enrich::apply(recipe, record, &ctx);
        if let Err(e) = write_record(&mut out, &enriched) {
            errors += 1;
            last_error = Some(anyhow!("{e}"));
            break;
        }
        transformed += 1;
    }

    let mut extra = Map::new();
    extra.insert("recipe_id".into(), json!(recipe.as_str()));
    extra.insert(
        "transform_outcome".into(),
        json!({
            "transformed_count": transformed,
            "error_count": errors,
        }),
    );
    // The indexed count, not the line count: an account with no
    // `externalId` cannot participate in the join, so counting the file
    // would overstate what the recipe had to work with.
    extra.insert(
        "auxiliary".into(),
        json!({"accounts_loaded": ctx.accounts_by_external_id.len()}),
    );
    span.finish_as_verb(extra);

    match last_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Load the auxiliary stream `account-context` joins against.
///
/// Records of other kinds are rejected here rather than being silently
/// indexed: a stream of the wrong kind would produce an all-null join
/// that looks like real orphan data.
fn build_enrichment_context(accounts: Option<&Path>) -> anyhow::Result<enrich::EnrichmentContext> {
    let Some(path) = accounts else {
        return Ok(enrich::EnrichmentContext::default());
    };
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let records: Vec<Record> = read_stream(BufReader::new(file))
        .collect::<stave_sdk::Result<Vec<_>>>()
        .map_err(|e| anyhow!("read --accounts {}: {e}", path.display()))?;
    if let Some(other) = records.iter().find(|r| r.kind != kinds::KIND_CLOUD_ACCOUNT) {
        return Err(anyhow!(
            "--accounts {} carries a `{}` record; it must be a stream of `{}` records \
             (capture one with `stave list {}`)",
            path.display(),
            other.kind,
            kinds::KIND_CLOUD_ACCOUNT,
            kinds::KIND_CLOUD_ACCOUNT,
        ));
    }
    Ok(enrich::EnrichmentContext::with_accounts(records))
}

// ---------------------------------------------------------------------------
// emit
// ---------------------------------------------------------------------------

fn run_emit(args: EmitArgs) -> anyhow::Result<()> {
    let format = args.format.unwrap_or_else(|| {
        if std::io::stdout().is_terminal() {
            EmitFormat::Md
        } else {
            EmitFormat::Jsonl
        }
    });

    let stdin = std::io::stdin();
    let stdin = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match format {
        EmitFormat::Jsonl => {
            for record in read_stream(stdin) {
                let record = record.map_err(|e| anyhow!("{e}"))?;
                write_record(&mut out, &record).map_err(|e| anyhow!("{e}"))?;
            }
        }
        EmitFormat::Md => {
            // Buffer so the header lands once, ahead of the rows.
            let records: Vec<Record> = read_stream(stdin)
                .collect::<stave_sdk::Result<Vec<_>>>()
                .map_err(|e| anyhow!("{e}"))?;
            emit_markdown_table(&mut out, &records)?;
        }
        EmitFormat::Json => {
            let records: Vec<Record> = read_stream(stdin)
                .collect::<stave_sdk::Result<Vec<_>>>()
                .map_err(|e| anyhow!("{e}"))?;
            let body = serde_json::to_string_pretty(&records)
                .context("serialize records as a JSON array")?;
            writeln!(out, "{body}")?;
        }
    }
    Ok(())
}

/// Render a small markdown table over a stream of records.
///
/// Columns: `_kind`, id, severity, primary timestamp. The field names
/// come from the kind table; records of unknown kinds fall back to `id`
/// and `severity` with no timestamp.
fn emit_markdown_table<W: Write>(out: &mut W, records: &[Record]) -> anyhow::Result<()> {
    writeln!(out, "| _kind | id | severity | timestamp |")?;
    writeln!(out, "|---|---|---|---|")?;
    for r in records {
        let spec = kind_spec(&r.kind);
        let id_field = spec.map_or("id", |s| s.id_field);
        let sev_field = spec.and_then(|s| s.severity_field).unwrap_or("severity");
        let ts_field = spec.and_then(|s| s.primary_timestamp_field);

        let id = r.get(id_field).and_then(scalar).unwrap_or_default();
        let sev = r.get(sev_field).and_then(scalar).unwrap_or_default();
        let ts = ts_field
            .and_then(|f| r.get(f))
            .and_then(scalar)
            .unwrap_or_default();

        writeln!(out, "| {} | {id} | {sev} | {ts} |", r.kind)?;
    }
    Ok(())
}

fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => None,
        _ => Some(v.to_string()),
    }
}

// ---------------------------------------------------------------------------
// mcp
// ---------------------------------------------------------------------------

fn run_mcp(args: McpArgs) -> anyhow::Result<()> {
    match args.cmd {
        McpCmd::Status => mcp_status(),
        McpCmd::Tools { filter } => mcp_tools(filter.as_deref()),
        McpCmd::Call(call) => mcp_call(call),
        McpCmd::Config { reveal } => mcp_config(reveal),
        McpCmd::Map => mcp_map(),
    }
}

fn mcp_status() -> anyhow::Result<()> {
    let url = mcp::resolve_url().map_err(|e| anyhow!("{e}"))?;
    let url_line = format!("{url} (source: {})", mcp_url_source()?);

    let bearer_line = if std::env::var(ACCESS_TOKEN_ENV)
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
    {
        format!("pre-minted token from {ACCESS_TOKEN_ENV}")
    } else {
        let id = auth::resolve_client_id(None).map_err(|e| anyhow!("{e}"))?;
        let secret = auth::resolve_client_secret().map_err(|e| anyhow!("{e}"))?;
        match (id, secret) {
            (Some(id), Some(secret)) => format!(
                "mints from the OAuth chain (client_id source: {}, secret source: {})",
                id.source.as_str(),
                secret.source.as_str()
            ),
            _ => "unavailable. Run `stave auth login`; MCP rides the same OAuth credentials \
                  as the GraphQL API."
                .to_string(),
        }
    };

    emit_status(&[
        ("mcp_url", url_line),
        ("mcp_bearer", bearer_line),
        ("protocol_version", mcp::PROTOCOL_VERSION.to_string()),
    ])
}

fn mcp_url_source() -> anyhow::Result<&'static str> {
    if std::env::var(mcp::MCP_URL_ENV)
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
    {
        return Ok("env");
    }
    let configured = auth::read_config()
        .map_err(|e| anyhow!("{e}"))?
        .and_then(|c| c.mcp.url)
        .is_some_and(|u| !u.trim().is_empty());
    Ok(if configured { "config" } else { "default" })
}

fn mcp_tools(filter: Option<&str>) -> anyhow::Result<()> {
    let span = audit::Span::start_fresh().with_verb_phase("mcp");
    let url = mcp::resolve_url().map_err(|e| anyhow!("{e}"))?;
    let (client, tools) = block_on(async {
        let bearer = mcp_bearer().await?;
        let client = mcp::McpClient::new(url, bearer)?;
        client.initialize().await?;
        let tools = client.tools_list().await?;
        Ok::<_, stave_sdk::StaveError>((client, tools))
    })
    .map_err(|e| anyhow!("{e}"))?;

    let needle = filter.map(str::to_lowercase);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut emitted = 0usize;
    for (idx, tool) in tools.iter().enumerate() {
        if let Some(n) = &needle {
            if !tool.name.to_lowercase().contains(n) {
                continue;
            }
        }
        let record = Record::wrap(
            "mcp_tool",
            SourceRef::now("mcp:tools/list", idx),
            json!({
                "name": tool.name,
                "description": tool.description,
                "read_only": mcp::is_read_only_tool(&tool.name),
            }),
        );
        write_record(&mut out, &record).map_err(|e| anyhow!("{e}"))?;
        emitted += 1;
    }

    let mut extra = Map::new();
    extra.insert("mcp_method".into(), json!("tools/list"));
    extra.insert("mcp_url".into(), json!(client.url()));
    extra.insert(
        "mcp_outcome".into(),
        json!({"tools_total": tools.len(), "tools_emitted": emitted}),
    );
    span.finish_as_verb(extra);
    Ok(())
}

fn mcp_call(args: McpCallArgs) -> anyhow::Result<()> {
    if !mcp::is_read_only_tool(&args.tool) {
        return Err(anyhow!(
            "write-guard: stave is read-only against live tenants; MCP tool '{}' is not \
             read-shaped and is refused. This is not configurable in this session.",
            args.tool
        ));
    }

    let arguments: Value = match args.args.as_deref() {
        Some(raw) => serde_json::from_str(raw).context("--args must be valid JSON")?,
        None => json!({}),
    };
    if !arguments.is_object() {
        return Err(anyhow!("--args must be a JSON object"));
    }

    let span = audit::Span::start_fresh().with_verb_phase("mcp");
    let url = mcp::resolve_url().map_err(|e| anyhow!("{e}"))?;
    let result = block_on(async {
        let bearer = mcp_bearer().await?;
        let client = mcp::McpClient::new(&url, bearer)?;
        client.initialize().await?;
        client.tools_call(&args.tool, arguments.clone()).await
    });

    let mut extra = Map::new();
    extra.insert("mcp_method".into(), json!("tools/call"));
    extra.insert("mcp_tool".into(), json!(args.tool));
    extra.insert("mcp_url".into(), json!(url));
    extra.insert(
        "mcp_args_shape".into(),
        json!(audit::shape_hash(&arguments)),
    );

    match result {
        Ok(raw) => {
            let payload = mcp::extract_call_payload(&raw);
            extra.insert(
                "mcp_outcome".into(),
                json!({
                    "ok": raw.get("isError").and_then(Value::as_bool) != Some(true),
                    "payload_shape": audit::shape_hash(&payload),
                }),
            );
            span.finish_as_verb(extra);
            println!("{}", serde_json::to_string_pretty(&payload)?);
            Ok(())
        }
        Err(e) => {
            extra.insert(
                "mcp_outcome".into(),
                json!({"ok": false, "error": format!("{e}")}),
            );
            span.finish_as_verb(extra);
            Err(anyhow!("{e}"))
        }
    }
}

fn mcp_config(reveal: bool) -> anyhow::Result<()> {
    let url = mcp::resolve_url().map_err(|e| anyhow!("{e}"))?;
    let auth_value = if reveal {
        let bearer = block_on(mcp_bearer()).map_err(|e| anyhow!("{e}"))?;
        format!("bearer {bearer}")
    } else {
        "bearer <redacted, rerun with --reveal>".to_string()
    };
    let config = json!({"url": url, "auth": auth_value});
    println!("{}", serde_json::to_string_pretty(&config)?);
    if !reveal {
        eprintln!(
            "stave mcp: bearer redacted. Rerun with --reveal to emit a paste-ready config. \
             The token is short-lived, so prefer wiring the client to `stave mcp` over \
             pasting one."
        );
    }
    Ok(())
}

/// Crosswalk live MCP tool names to curated operation names. The
/// vocabularies differ in separator and in curation, so a normalized
/// exact match catches the overlap and everything else emits
/// `operation_id: null`, making the gap itself data (charter F3/F4).
fn mcp_map() -> anyhow::Result<()> {
    let url = mcp::resolve_url().map_err(|e| anyhow!("{e}"))?;
    let tools = block_on(async {
        let bearer = mcp_bearer().await?;
        let client = mcp::McpClient::new(url, bearer)?;
        client.initialize().await?;
        client.tools_list().await
    })
    .map_err(|e| anyhow!("{e}"))?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for (idx, tool) in tools.iter().enumerate() {
        let candidate = tool.name.to_ascii_lowercase().replace('-', "_");
        let matched = ops::find(&candidate).ok().map(|op| op.name);
        let record = Record::wrap(
            "mcp_tool_mapping",
            SourceRef::now("mcp:map", idx),
            json!({
                "tool": tool.name,
                "operation_id": matched,
                "read_only": mcp::is_read_only_tool(&tool.name),
            }),
        );
        write_record(&mut out, &record).map_err(|e| anyhow!("{e}"))?;
    }
    Ok(())
}

/// Bearer token for the MCP server: the same OAuth credentials the
/// GraphQL API uses, with `STAVE_ACCESS_TOKEN` short-circuiting the
/// mint. Minted tokens land in the shared cache, so `mcp tools`
/// followed by `mcp call` costs one mint, not two.
async fn mcp_bearer() -> stave_sdk::Result<String> {
    if let Ok(v) = std::env::var(ACCESS_TOKEN_ENV) {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let client_id = auth::resolve_client_id(None)?.ok_or_else(auth::credentials_chain_error)?;
    let client_secret = auth::resolve_client_secret()?.ok_or_else(auth::credentials_chain_error)?;
    let token_url = auth::resolve_token_url(None)?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| stave_sdk::StaveError::Network(e.to_string()))?;
    let minted = token::cached_or_mint(
        &http,
        &token_url.value,
        &client_id.value,
        &client_secret.value,
    )
    .await?;
    Ok(minted.access_token)
}

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build a current-thread tokio runtime");
    runtime.block_on(fut)
}

fn chrono_now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

/// GraphQL variables from `--vars` plus `--var key=value` overrides.
/// A `--var` value is parsed as JSON when it parses, so numbers, bools,
/// and arrays need no quoting, and taken as a string otherwise.
fn merge_variables(vars: Option<&str>, pairs: &[String]) -> anyhow::Result<Value> {
    let mut map = match vars {
        Some(raw) => {
            let parsed: Value = serde_json::from_str(raw).context("--vars must be valid JSON")?;
            match parsed {
                Value::Object(map) => map,
                other => {
                    return Err(anyhow!(
                        "--vars must be a JSON object, got {}",
                        json_type_name(&other)
                    ));
                }
            }
        }
        None => Map::new(),
    };
    for entry in pairs {
        let (key, raw) = entry
            .split_once('=')
            .ok_or_else(|| anyhow!("--var expects `key=value`, got `{entry}`"))?;
        let value = serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()));
        map.insert(key.to_string(), value);
    }
    Ok(Value::Object(map))
}

fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// Read a GraphQL document from a path, or from stdin when the path is
/// `-`.
fn read_document(source: &str) -> stave_sdk::Result<String> {
    if source == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        return Ok(buf);
    }
    std::fs::read_to_string(source).map_err(|e| {
        stave_sdk::StaveError::InvalidParam("--query".into(), format!("read {source}: {e}"))
    })
}

/// Read a secret without echoing it: from stdin when `use_stdin` is set,
/// otherwise from the terminal. There is no prompt when stdin is not a
/// terminal, per the no-captive-UI rule; the error names the flag that
/// makes the non-interactive path explicit.
fn read_secret(use_stdin: bool, prompt: &str, sibling_flags: &str) -> anyhow::Result<String> {
    let raw = if use_stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("read the secret from stdin")?;
        buf
    } else if std::io::stdin().is_terminal() {
        rpassword::prompt_password(prompt).context("read the secret from the terminal")?
    } else {
        return Err(anyhow!(
            "stdin is not a terminal, so there is nothing to prompt. Pass --stdin to feed \
             the secret on stdin (alongside {sibling_flags}): \
             printf '%s' \"$SECRET\" | stave ... --stdin"
        ));
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("the secret must not be empty"));
    }
    Ok(trimmed.to_string())
}

fn prompt_line(prompt: &str) -> anyhow::Result<String> {
    let mut stderr = std::io::stderr();
    write!(stderr, "{prompt}")?;
    stderr.flush()?;
    let mut buf = String::new();
    std::io::stdin()
        .read_line(&mut buf)
        .context("read from stdin")?;
    Ok(buf.trim().to_string())
}

/// Shape of a secret, never its value.
fn mask(len: usize) -> String {
    format!("<redacted, length={len}>")
}

/// Render a status report: aligned key/value lines on a terminal, one
/// JSON object on a pipe. Status is a sink, not a stream source, so the
/// JSON form carries no `_kind` or `_source`.
fn emit_status(fields: &[(&str, String)]) -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if out.is_terminal() {
        let width = fields.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        for (key, value) in fields {
            writeln!(out, "{key:<width$}  {value}")?;
        }
    } else {
        let mut map = Map::new();
        for (key, value) in fields {
            map.insert((*key).to_string(), Value::String(value.clone()));
        }
        writeln!(out, "{}", Value::Object(map))?;
    }
    Ok(())
}

fn kind_value_parser() -> clap::builder::PossibleValuesParser {
    clap::builder::PossibleValuesParser::new(kinds::ALL_KINDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_names_match_sdk() {
        // RECIPES exists for error messages; the SDK's parser is the
        // authority. If the recipe library is re-cut, this fails and
        // names the drift instead of shipping a stale suggestion.
        for name in RECIPES {
            assert!(
                enrich::Recipe::parse(name).is_some(),
                "RECIPES lists '{name}', which the SDK no longer accepts"
            );
        }
    }

    #[test]
    fn every_kind_has_a_registered_list_operation() {
        for spec in kinds::all_kinds() {
            assert!(
                ops::find(spec.list_operation).is_ok(),
                "kind {} points at unregistered operation {}",
                spec.name,
                spec.list_operation
            );
        }
    }

    #[test]
    fn merge_variables_parses_json_scalars_and_falls_back_to_strings() {
        let vars = merge_variables(None, &["first=5".into(), "status=OPEN".into()]).unwrap();
        assert_eq!(vars["first"], json!(5));
        assert_eq!(vars["status"], json!("OPEN"));
    }

    #[test]
    fn merge_variables_lets_var_override_vars() {
        let vars = merge_variables(Some(r#"{"first": 10}"#), &["first=25".into()]).unwrap();
        assert_eq!(vars["first"], json!(25));
    }

    #[test]
    fn merge_variables_rejects_non_object_vars() {
        let err = merge_variables(Some("[1,2]"), &[]).unwrap_err();
        assert!(format!("{err}").contains("must be a JSON object"));
    }

    #[test]
    fn merge_variables_rejects_var_without_equals() {
        let err = merge_variables(None, &["first".into()]).unwrap_err();
        assert!(format!("{err}").contains("key=value"));
    }

    #[test]
    fn next_cursor_only_when_more_pages() {
        let more = json!({"issuesV2": {"nodes": [],
            "pageInfo": {"hasNextPage": true, "endCursor": "abc"}}});
        assert_eq!(next_cursor(&more, "issuesV2").as_deref(), Some("abc"));
        let done = json!({"issuesV2": {"nodes": [],
            "pageInfo": {"hasNextPage": false, "endCursor": "abc"}}});
        assert_eq!(next_cursor(&done, "issuesV2"), None);
        assert_eq!(next_cursor(&json!({}), "issuesV2"), None);
    }

    fn shape_of(predicate: &str) -> String {
        predicate_ast_shape(&cel::compile(predicate).expect("compile"))
    }

    #[test]
    fn ast_shape_ignores_literal_values() {
        assert_eq!(
            shape_of(r#"severity == "CRITICAL""#),
            shape_of(r#"severity == "HIGH""#)
        );
        assert_eq!(shape_of("first == 5"), shape_of("first == 4096"));
    }

    #[test]
    fn ast_shape_distinguishes_the_field_filtered_on() {
        // The question audit mining asks first. Blanking every quoted
        // run in the Debug representation would collapse these, since
        // identifiers are quoted there too.
        assert_ne!(
            shape_of(r#"severity == "CRITICAL""#),
            shape_of(r#"status == "CRITICAL""#)
        );
    }

    #[test]
    fn ast_shape_distinguishes_the_operator() {
        assert_ne!(
            shape_of(r#"severity == "CRITICAL""#),
            shape_of(r#"severity != "CRITICAL""#)
        );
        assert_ne!(shape_of("first > 5"), shape_of("first < 5"));
    }

    #[test]
    fn ast_shape_distinguishes_structure() {
        assert_ne!(
            shape_of(r#"severity == "CRITICAL""#),
            shape_of(r#"severity == "CRITICAL" && status == "OPEN""#)
        );
        assert_ne!(
            shape_of(r#"severity == "CRITICAL" && status == "OPEN""#),
            shape_of(r#"severity == "CRITICAL" || status == "OPEN""#)
        );
    }

    #[test]
    fn strip_literals_keeps_operators_and_fields_drops_values_and_ids() {
        let folded = strip_literals(
            r#"IdedExpr { id: 12, expr: Call(CallExpr { func_name: "_==_", args: [IdedExpr { id: 1, expr: Ident("severity") }, IdedExpr { id: 3, expr: Literal(String("CRITICAL")) }] }) }"#,
        );
        assert!(folded.contains(r#""_==_""#), "operator dropped: {folded}");
        assert!(
            folded.contains(r#"Ident("severity")"#),
            "field name dropped: {folded}"
        );
        assert!(
            folded.contains(r#"Literal(String(""))"#),
            "literal value survived: {folded}"
        );
        assert!(!folded.contains("CRITICAL"), "value survived: {folded}");
        assert!(!folded.contains("12"), "node id survived: {folded}");
    }

    #[test]
    fn mask_reports_length_not_value() {
        let masked = mask(40);
        assert!(masked.contains("40"));
        assert!(!masked.contains("secret"));
    }

    #[test]
    fn since_program_requires_a_timestamp_field() {
        let with_ts = kind_spec("issue").expect("issue in the kind table");
        assert!(build_since_program(with_ts, Some("24h")).unwrap().is_some());
        assert!(build_since_program(with_ts, None).unwrap().is_none());

        let without_ts = kind_spec("project").expect("project in the kind table");
        let err = build_since_program(without_ts, Some("24h")).unwrap_err();
        assert!(format!("{err}").contains("no primary timestamp field"));
    }

    #[test]
    fn since_rejects_quotes_and_bad_units() {
        let spec = kind_spec("issue").expect("issue in the kind table");
        assert!(build_since_program(spec, Some("24h\" || true")).is_err());
        assert!(build_since_program(spec, Some("7d")).is_err());
    }

    #[test]
    fn go_duration_accepts_valid_units() {
        for good in ["24h", "30m", "60s", "500ms", "100ns", "100us", "100µs"] {
            assert!(is_valid_go_duration(good), "{good} should parse");
        }
    }

    #[test]
    fn go_duration_accepts_compound_and_decimal() {
        for good in ["1h30m", "2h45m30s", "1.5h", "0.25s"] {
            assert!(is_valid_go_duration(good), "{good} should parse");
        }
    }

    #[test]
    fn go_duration_rejects_malformed() {
        for bad in [
            "",
            "7d",
            "1w",
            "24",
            "h",
            "not-a-real-duration",
            "24h-extra",
        ] {
            assert!(!is_valid_go_duration(bad), "{bad} should not parse");
        }
    }

    #[test]
    fn json_type_name_covers_every_variant() {
        assert_eq!(json_type_name(&Value::Null), "null");
        assert_eq!(json_type_name(&json!(true)), "a boolean");
        assert_eq!(json_type_name(&json!(1)), "a number");
        assert_eq!(json_type_name(&json!("s")), "a string");
        assert_eq!(json_type_name(&json!([])), "an array");
        assert_eq!(json_type_name(&json!({})), "an object");
    }
}
