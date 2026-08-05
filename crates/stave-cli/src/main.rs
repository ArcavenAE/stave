//! stave — Rust CLI for the iru Endpoint Management API.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::io::{BufReader, IsTerminal, Read, Write};
use std::process::ExitCode;

use anyhow::{Context, anyhow};
use stave_sdk::{
    CallOptions, Client, Record, SourceRef, audit, auth, cel, enrich, kind_spec, kinds, mcp,
    read_stream, registry, write_record,
};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Map, Value, json};

#[derive(Parser, Debug)]
#[command(
    name = "stave",
    version,
    about = "Rust CLI for the iru (Kandji) Endpoint Management API",
    long_about = "Agent-first CLI over the iru Endpoint Management API. Codegen from OpenAPI, \
                  audit-trail-as-feature, read-only by default against the live tenant.\n\n\
                  Set STAVE_API_TOKEN + STAVE_SUBDOMAIN to authenticate. Use \
                  `stave ops list` to discover operations and `stave api <operationId> \
                  --param k=v` to invoke any of them. `stave mcp tools` talks to iru's \
                  published MCP server with the same credentials discipline."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Invoke any operation in the OpenAPI spec by ID.
    Api(ApiArgs),
    /// List operations in the spec.
    Ops(OpsArgs),
    /// Manage stored credentials.
    Auth(AuthArgs),
    /// Operate iru's published MCP server (tools, calls, client config).
    Mcp(McpArgs),
    /// List records of a `_kind` from the API as a JSON-line stream.
    List(ListArgs),
    /// Fetch a single record by ID and emit one JSON line.
    Get(GetArgs),
    /// List + substring match against the kind's search field.
    Search(SearchArgs),
    /// Format a JSON-line stream from stdin.
    Emit(EmitArgs),
    /// Drop records that don't match a CEL predicate.
    Filter(FilterArgs),
    /// Attach computed/joined fields per a named recipe.
    Enrich(EnrichArgs),
    /// Inspect or modify the persisted config.
    Config(ConfigArgs),
}

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Manage credentials and persisted defaults for stave.\n\n\
                  Token resolution chain:\n  \
                  1. STAVE_API_TOKEN environment variable\n  \
                  2. Platform keyring (macOS Keychain, Linux Secret Service)\n  \
                  3. Config file at ~/.config/stave/config.toml \
                     (override with STAVE_CONFIG)\n     \
                     [auth] token = \"<value>\"\n\n\
                  Subdomain resolution chain (the tenant name in \
                  https://<subdomain>.api.kandji.io):\n  \
                  1. --subdomain flag (per-call override)\n  \
                  2. STAVE_SUBDOMAIN environment variable\n  \
                  3. [default] subdomain in the config file\n\n\
                  Use `stave auth login` to persist a token and/or subdomain.\n\
                  Use `stave config show` to see what's persisted.\n\
                  MCP credentials are managed separately via `stave mcp login`."
)]
struct AuthArgs {
    #[command(subcommand)]
    cmd: AuthCmd,
}

#[derive(Subcommand, Debug)]
enum AuthCmd {
    /// Persist an API token (keyring) and/or default subdomain/region (config file).
    Login(AuthLoginArgs),
    /// Show resolved token, subdomain, region, and MCP credentials with sources.
    Status,
    /// Remove the API token from the platform keyring.
    Logout,
}

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Persist credentials and per-machine defaults for stave.\n\n\
                  Token sources (--token wins over --stdin):\n  \
                  --token <value>      explicit, useful for scripts\n  \
                  --stdin              read whole stdin (so `echo $T | stave auth login --stdin`)\n\n\
                  Defaults written to ~/.config/stave/config.toml:\n  \
                  --subdomain <name>   persist [default] subdomain — the tenant name in\n                       \
                                       https://<subdomain>.api.kandji.io.\n  \
                  --region <us|eu>     persist [default] region — picks the US or EU API host.\n\n\
                  At least one of --token, --stdin, --subdomain, --region must be provided.\n\
                  Interactive prompting is not supported in v0.1.\n\
                  An existing keyring entry is overwritten without prompt; existing\n\
                  config values are replaced only by the flags you pass (others are kept)."
)]
struct AuthLoginArgs {
    /// API token. Redacted from the audit-trail argv.
    #[arg(long, value_name = "VALUE")]
    token: Option<String>,

    /// Read the token from stdin (entire stream, trimmed).
    #[arg(long, conflicts_with = "token")]
    stdin: bool,

    /// Persist `[default] subdomain` in the config file. Resolved per
    /// call when neither --subdomain nor STAVE_SUBDOMAIN is set.
    #[arg(long, value_name = "NAME")]
    subdomain: Option<String>,

    /// Persist `[default] region` (`us` or `eu`).
    #[arg(long, value_name = "REGION")]
    region: Option<String>,
}

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Invoke any operation in the iru OpenAPI spec by its operationId.\n\n\
                  Path and query parameters are supplied as repeatable --param k=v.\n\
                  Request bodies (POST/PATCH) are passed as --body '{...json...}'.\n\n\
                  stave is read-only by default: non-GET operations require\n\
                  --allow-write (or STAVE_ALLOW_WRITE=1 / `config set allow_writes true`).\n\n\
                  Examples:\n  \
                  stave api get_devices --param limit=5\n  \
                  stave api get_devices_device_id --param device_id=<uuid>\n  \
                  stave api patch_devices_device_id --param device_id=<uuid> \\\n      \
                  --body '{\"asset_tag\":\"A-100\"}' --allow-write"
)]
struct ApiArgs {
    /// operationId from the OpenAPI spec. Run `stave ops list` to discover.
    operation_id: String,

    /// Path or query parameter as `key=value`. Repeatable.
    #[arg(long = "param", short = 'p', value_name = "KEY=VALUE")]
    params: Vec<String>,

    /// Request body as JSON. For POST/PATCH operations.
    #[arg(long, value_name = "JSON")]
    body: Option<String>,

    /// Tenant subdomain override. Resolves through
    /// flag → STAVE_SUBDOMAIN env → [default] subdomain in config.
    #[arg(long)]
    subdomain: Option<String>,

    /// Permit a mutating (non-GET) operation. Off by default —
    /// stave treats the tenant as production.
    #[arg(long)]
    allow_write: bool,

    /// Skip operation/response detail in the audit trail (still records a stub).
    #[arg(long)]
    no_audit: bool,
}

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Stream records of a `_kind` from the iru API as JSON-lines.\n\n\
                  Each line carries `_kind` and `_source` (operation_id, response_index, \
                  fetched_at) plus the domain fields from the API response. Compose with \
                  `stave filter`, `stave enrich`, `stave emit`.\n\n\
                  The tenant subdomain resolves through flag → env → config; set it once \
                  via `stave auth login --subdomain <name>`. Query parameters are \
                  supplied via repeatable `--param k=v`.\n\n\
                  Examples:\n  \
                  stave list devices\n  \
                  stave list devices --param platform=Mac | stave emit --format md\n  \
                  stave list vulnerabilities --param size=50"
)]
struct ListArgs {
    /// Stream-contract `_kind`. Run `stave list --help` to see the v0.1 set.
    #[arg(value_parser = kind_value_parser())]
    kind: String,

    /// Tenant subdomain override. Resolves through
    /// flag → STAVE_SUBDOMAIN env → [default] subdomain in config.
    #[arg(long)]
    subdomain: Option<String>,

    /// Path or query parameter as `key=value`. Repeatable.
    #[arg(long = "param", short = 'p', value_name = "KEY=VALUE")]
    params: Vec<String>,

    /// Maximum number of records to emit. Applies after `--since`.
    #[arg(long, value_name = "N")]
    limit: Option<usize>,

    /// Drop records older than this duration. Format follows Go's
    /// duration syntax (`24h`, `30m`, `1h30m`, `0.5h`). Valid units:
    /// `ns`, `us`, `µs`, `ms`, `s`, `m`, `h`. (Go duration does not
    /// accept `d` for days — use `24h`, `48h`, etc.) Requires the
    /// kind to have a primary timestamp field.
    #[arg(long, value_name = "DUR")]
    since: Option<String>,

    /// Skip the per-call audit detail (still emits a stub).
    #[arg(long)]
    no_audit: bool,
}

#[derive(clap::Args, Debug)]
#[command(long_about = "Fetch a single record by ID.\n\n\
                  The `<id>` argument binds to the kind's id path parameter \
                  (`device_id` for devices, `blueprint_id` for blueprints, `cve_id` for \
                  vulnerabilities, etc.).\n\n\
                  Examples:\n  \
                  stave get device 7f9e2a10-1111-4a4a-9c1a-000000000001\n  \
                  stave get vulnerability CVE-2026-1234")]
struct GetArgs {
    /// Stream-contract `_kind` (must have a get-by-id endpoint).
    #[arg(value_parser = kind_value_parser())]
    kind: String,

    /// Identifier for the record. Maps to the kind's id path param.
    id: String,

    /// Tenant subdomain override. Resolves through
    /// flag → STAVE_SUBDOMAIN env → [default] subdomain in config.
    #[arg(long)]
    subdomain: Option<String>,

    /// Additional path / query parameters as `key=value`. Repeatable.
    #[arg(long = "param", short = 'p', value_name = "KEY=VALUE")]
    params: Vec<String>,

    /// Skip the per-call audit detail (still emits a stub).
    #[arg(long)]
    no_audit: bool,
}

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Substring-match a `list` stream against the kind's search field.\n\n\
                  Implementation: list under the hood, then drop records whose \
                  `search_field` does not contain the query (case-insensitive). \
                  This is a v0.1 fallback — kinds without dedicated search endpoints \
                  use the kind-specific search field defined in the SDK kind table.\n\n\
                  Examples:\n  \
                  stave search device macbook\n  \
                  stave search blueprint fleet --limit 5"
)]
struct SearchArgs {
    /// Stream-contract `_kind` (must define a search field).
    #[arg(value_parser = kind_value_parser())]
    kind: String,

    /// Substring to match (case-insensitive).
    query: String,

    /// Tenant subdomain override for the underlying `list` call.
    #[arg(long)]
    subdomain: Option<String>,

    /// Additional path / query parameters as `key=value`. Repeatable.
    #[arg(long = "param", short = 'p', value_name = "KEY=VALUE")]
    params: Vec<String>,

    /// Maximum number of matching records to emit.
    #[arg(long, value_name = "N")]
    limit: Option<usize>,

    /// Skip the per-call audit detail (still emits a stub).
    #[arg(long)]
    no_audit: bool,
}

#[derive(clap::Args, Debug)]
#[command(long_about = "Format a JSON-line stream from stdin.\n\n\
                  Records must follow the stave stream contract (`_kind`, `_source`, \
                  domain fields). Default output: `jsonl` (passthrough) for non-TTY, \
                  `md` markdown table for TTY.\n\n\
                  Formats:\n  \
                  jsonl   one record per line, exact passthrough\n  \
                  md      markdown table (kind, id, severity, primary timestamp)\n\n\
                  More formats (table, csv) ship in v0.2.")]
struct EmitArgs {
    /// Output format. If unset, defaults to `jsonl` (non-TTY) or `md` (TTY).
    #[arg(long, value_enum)]
    format: Option<EmitFormat>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum EmitFormat {
    Jsonl,
    Md,
}

#[derive(clap::Args, Debug)]
#[command(long_about = "Drop records that don't match a CEL predicate.\n\n\
                  The predicate is Common Expression Language (CEL). Each top-level field \
                  of a record is bound as a top-level variable, so you can write \
                  `platform == \"Mac\" && _kind == \"device\"` directly. The full record \
                  is also available as `record` for use with the `has()` macro \
                  (`has(record.asset_tag)`).\n\n\
                  Adapter rules (per sidestep finding-001, carried forward):\n  \
                  - `*_at`, `*_date`, `last_check_in`, and `ts` fields are promoted to \
                  timestamps so `last_check_in < now` works\n  \
                  - missing top-level field access raises a runtime error (use `has(record.X)` instead)\n  \
                  - `now` is bound to the current UTC time per query\n  \
                  - the predicate must return a boolean\n\n\
                  Use `--explain` to print the schema, parsed AST, and `now` binding without \
                  consuming any records.\n\n\
                  Examples:\n  \
                  stave filter --where '_kind == \"device\" && platform == \"Mac\"'\n  \
                  stave filter --where 'severity in [\"critical\",\"high\"]'\n  \
                  stave filter --where 'last_check_in < now - duration(\"720h\")'\n  \
                  stave filter --where 'has(record.asset_tag)' --explain")]
struct FilterArgs {
    /// CEL predicate. Returns one record per matching input.
    #[arg(long, value_name = "CEL")]
    r#where: String,

    /// Print the schema, parsed AST, and `now` binding, then exit
    /// without consuming stdin.
    #[arg(long)]
    explain: bool,
}

#[derive(clap::Args, Debug)]
#[command(
    long_about = "Attach computed or joined fields to records per a named recipe.\n\n\
                  Recipes (v0.1):\n  \
                  blueprint-context  for each device, attach its blueprint as `blueprint: {...}`. \
                                     Orphans get `blueprint: null`. Other kinds pass through. \
                                     Requires --blueprints.\n  \
                  severity-roll-up   for every record, set `severity_rollup` from the record's \
                                     own severity (null when absent).\n  \
                  device-platform    hoist `platform` to a top-level `_platform` field. \
                                     Records without a platform pass through.\n\n\
                  Auxiliary records come from --blueprints <FILE> (JSONL of blueprint \
                  records). Streaming auxiliary fetch via the API will land in a later slice.\n\n\
                  Examples:\n  \
                  stave list blueprints > blueprints.jsonl\n  \
                  stave list devices | stave enrich --with blueprint-context \\\n                                                          \
                  --blueprints blueprints.jsonl"
)]
struct EnrichArgs {
    /// Recipe name. One of: blueprint-context, severity-roll-up, device-platform.
    #[arg(long = "with", value_name = "RECIPE")]
    recipe: String,

    /// Auxiliary stream of blueprint records as JSONL.
    /// Required by `blueprint-context`.
    #[arg(long, value_name = "FILE")]
    blueprints: Option<std::path::PathBuf>,
}

#[derive(clap::Args, Debug)]
#[command(long_about = "Inspect or modify the persisted config at \
                  ~/.config/stave/config.toml (override STAVE_CONFIG).\n\n\
                  Subcommands:\n  \
                  show              print the current config, redacting secrets\n  \
                  path              print the resolved config path\n  \
                  set <key> <val>   set one of: subdomain, region, allow_writes, auth.token,\n                    \
                                    mcp.profile, mcp.url (tokens/keys prefer the keyring —\n                    \
                                    `stave auth login` / `stave mcp login`)\n  \
                  unset <key>       clear one of the same keys\n\n\
                  Set values are persisted to the config file, preserving any unrelated sections.")]
struct ConfigArgs {
    #[command(subcommand)]
    cmd: ConfigCmd,
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// Print the current config (secrets redacted).
    Show,
    /// Print the resolved config file path.
    Path,
    /// Set one of: `subdomain`, `region`, `allow_writes`, `auth.token`, `mcp.profile`, `mcp.url`.
    Set {
        /// Key to set.
        key: String,
        /// Value to write.
        value: String,
    },
    /// Clear one of the same keys.
    Unset {
        /// Key to clear.
        key: String,
    },
}

#[derive(clap::Args, Debug)]
#[command(long_about = "Operate iru's published MCP server.\n\n\
                  iru exposes its Enterprise API as MCP tools at\n  \
                  https://<subdomain>.connect.iru.com/mcp-server/connector/kandji/tools\n\
                  authenticated by the X-API-Key (sk_live:...) + X-MCP-Profile headers from \
                  the token's one-time MCP configuration.\n\n\
                  Subcommands:\n  \
                  login    store the MCP API key (keyring) + profile/url (config)\n  \
                  status   report resolved MCP credentials (never prints secrets)\n  \
                  config   emit MCP client configuration JSON for Claude/Cursor/Codex\n  \
                  tools    list the server's tools live (JSONL)\n  \
                  call     invoke one tool (write-gated unless the tool is read-shaped)\n  \
                  map      crosswalk live MCP tool names to local REST operationIds")]
struct McpArgs {
    #[command(subcommand)]
    cmd: McpCmd,
}

#[derive(Subcommand, Debug)]
enum McpCmd {
    /// Store MCP credentials: API key → keyring, profile/url → config.
    Login(McpLoginArgs),
    /// Report resolved MCP credential sources (no secrets printed).
    Status,
    /// Emit MCP client configuration JSON (use --reveal to include the key).
    Config {
        /// Include the actual API key instead of a redaction placeholder.
        #[arg(long)]
        reveal: bool,
    },
    /// List the MCP server's tools as a JSON-line stream (`_kind: mcp_tool`).
    Tools {
        /// Substring filter on the tool name.
        #[arg(long)]
        filter: Option<String>,
    },
    /// Invoke one MCP tool. Non-read tools require --allow-write.
    Call(McpCallArgs),
    /// Crosswalk live MCP tool names to local REST operationIds (JSONL).
    Map,
}

#[derive(clap::Args, Debug)]
struct McpLoginArgs {
    /// MCP API key (sk_live:...). Prefer --stdin so the key never
    /// appears in argv or shell history.
    #[arg(long, value_name = "VALUE")]
    api_key: Option<String>,

    /// Read the MCP API key from stdin (entire stream, trimmed).
    #[arg(long, conflicts_with = "api_key")]
    stdin: bool,

    /// Persist `[mcp] profile` (the X-MCP-Profile header value).
    #[arg(long, value_name = "HEX")]
    profile: Option<String>,

    /// Persist `[mcp] url` (overrides the subdomain-derived default).
    #[arg(long, value_name = "URL")]
    url: Option<String>,
}

#[derive(clap::Args, Debug)]
struct McpCallArgs {
    /// Tool name, e.g. `get-devices`. Run `stave mcp tools` to discover.
    tool: String,

    /// Tool arguments as a JSON object.
    #[arg(long, value_name = "JSON")]
    args: Option<String>,

    /// Permit a non-read tool (anything not `get-*`/`list-*`). Off by
    /// default — stave treats the tenant as production.
    #[arg(long)]
    allow_write: bool,
}

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
        Cmd::Api(args) => run_api(args),
        Cmd::Ops(args) => run_ops(args),
        Cmd::Auth(args) => run_auth(args),
        Cmd::Mcp(args) => run_mcp(args),
        Cmd::List(args) => run_list(args),
        Cmd::Get(args) => run_get(args),
        Cmd::Search(args) => run_search(args),
        Cmd::Emit(args) => run_emit(args),
        Cmd::Filter(args) => run_filter(args),
        Cmd::Enrich(args) => run_enrich(args),
        Cmd::Config(args) => run_config(args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("stave: {e:#}");
            ExitCode::FAILURE
        }
    }
}

#[derive(clap::Args, Debug)]
struct OpsArgs {
    #[command(subcommand)]
    cmd: OpsCmd,
}

#[derive(Subcommand, Debug)]
enum OpsCmd {
    /// List operationIds in the vendored spec.
    List {
        /// Substring filter on the operationId.
        #[arg(long)]
        filter: Option<String>,
    },
    /// Show details for one operation.
    Show {
        /// operationId from the OpenAPI spec.
        operation_id: String,
    },
}

fn run_ops(args: OpsArgs) -> anyhow::Result<()> {
    match args.cmd {
        OpsCmd::List { filter } => {
            let mut ids: Vec<&str> = registry()
                .iter()
                .map(|m| m.id.as_str())
                .filter(|id| match &filter {
                    Some(needle) => id.to_lowercase().contains(&needle.to_lowercase()),
                    None => true,
                })
                .collect();
            ids.sort_unstable();
            for id in ids {
                println!("{id}");
            }
            Ok(())
        }
        OpsCmd::Show { operation_id } => {
            let r = registry();
            let op = r.find(&operation_id).map_err(|e| anyhow!("{e}"))?;
            let summary = op.summary.as_deref().unwrap_or("");
            println!("operationId: {}", op.id);
            println!("method:      {}", op.method.as_str());
            println!(
                "path:        https://<subdomain>.api.kandji.io{}",
                op.path_template
            );
            if !summary.is_empty() {
                println!("summary:     {summary}");
            }
            if op.is_mutating() {
                println!("write-guard: mutating — requires --allow-write");
            }
            if !op.path_params.is_empty() {
                println!("path params:");
                for p in &op.path_params {
                    let req = if op.required_params.contains(p) {
                        " (required)"
                    } else {
                        ""
                    };
                    println!("  - {p}{req}");
                }
            }
            if !op.query_params.is_empty() {
                println!("query params:");
                for p in &op.query_params {
                    let req = if op.required_params.contains(p) {
                        " (required)"
                    } else {
                        ""
                    };
                    println!("  - {p}{req}");
                }
            }
            if op.has_body {
                println!("body:        required (pass --body '<json>')");
            }
            Ok(())
        }
    }
}

fn run_api(args: ApiArgs) -> anyhow::Result<()> {
    let mut params = parse_params(&args.params)?;
    if let Some(body) = args.body {
        let body_value: Value = serde_json::from_str(&body).context("--body must be valid JSON")?;
        params.insert("body".to_string(), body_value);
    }
    let params_value = Value::Object(params);
    let allow_write = resolve_allow_write(args.allow_write)?;

    let response = call_op_blocking_for_verb(
        &args.operation_id,
        params_value,
        args.subdomain.as_deref(),
        allow_write,
        args.no_audit,
        "api",
        &[],
    )?;

    let pretty = serde_json::to_string_pretty(&response).context("serialize response as JSON")?;
    println!("{pretty}");
    Ok(())
}

fn run_auth(args: AuthArgs) -> anyhow::Result<()> {
    match args.cmd {
        AuthCmd::Login(login) => auth_login(login),
        AuthCmd::Status => auth_status(),
        AuthCmd::Logout => auth_logout(),
    }
}

fn auth_login(args: AuthLoginArgs) -> anyhow::Result<()> {
    let token_source_provided = args.token.is_some() || args.stdin;
    let subdomain = args
        .subdomain
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let region = args
        .region
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(r) = region {
        if r != "us" && r != "eu" {
            return Err(anyhow!("--region must be `us` or `eu`, got {r:?}"));
        }
    }

    if !token_source_provided && subdomain.is_none() && region.is_none() {
        return Err(anyhow!(
            "no source. Pass `--token <value>`, `--stdin`, `--subdomain <name>`, \
             or `--region <us|eu>`. See `stave auth login --help`."
        ));
    }

    if token_source_provided {
        let token = if let Some(t) = args.token.as_deref() {
            t.to_string()
        } else {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("read --stdin")?;
            buf
        };
        let token = token.trim();
        if token.is_empty() {
            return Err(anyhow!("token must not be empty"));
        }
        auth::store_keyring(token).map_err(|e| anyhow!("{e}"))?;
        let target = match auth::read_keyring() {
            Some(_) => "stored in keyring",
            None => "stored — but immediate read-back failed (keyring backend may be unavailable)",
        };
        eprintln!("stave auth: token {target}");
    }

    if subdomain.is_some() || region.is_some() {
        let subdomain_owned = subdomain.map(str::to_string);
        let region_owned = region.map(str::to_string);
        let path = auth::write_config(|cfg| {
            if let Some(s) = subdomain_owned {
                cfg.default.subdomain = Some(s);
            }
            if let Some(r) = region_owned {
                cfg.default.region = Some(r);
            }
        })
        .map_err(|e| anyhow!("{e}"))?;
        let mut updated: Vec<&str> = Vec::new();
        if subdomain.is_some() {
            updated.push("subdomain");
        }
        if region.is_some() {
            updated.push("region");
        }
        eprintln!(
            "stave auth: persisted {} to {}",
            updated.join(" + "),
            path.display()
        );
    }

    Ok(())
}

fn auth_status() -> anyhow::Result<()> {
    let mut authenticated = false;
    let mut stdout = std::io::stdout().lock();
    match auth::resolve() {
        Ok(resolved) => {
            authenticated = true;
            // Never print the token. Length + source is the contract.
            writeln!(
                stdout,
                "token:       authenticated (source: {}, length: {} bytes)",
                resolved.source.as_str(),
                resolved.token.len()
            )?;
        }
        Err(e) => {
            writeln!(stdout, "token:       not authenticated ({e})")?;
        }
    }

    let subdomain_line = match auth::resolve_subdomain(None) {
        Ok(Some(r)) => format!("subdomain:   {} (source: {})", r.value, r.source.as_str()),
        Ok(None) => "subdomain:   unset (set via `stave auth login --subdomain <name>`, \
                     STAVE_SUBDOMAIN, or `stave config set subdomain <name>`)"
            .to_string(),
        Err(e) => format!("subdomain:   error ({e})"),
    };
    writeln!(stdout, "{subdomain_line}")?;

    let region_line = match auth::resolve_region(None) {
        Ok(Some(r)) => format!("region:      {} (source: {})", r.value, r.source.as_str()),
        Ok(None) => "region:      us (default)".to_string(),
        Err(e) => format!("region:      error ({e})"),
    };
    writeln!(stdout, "{region_line}")?;

    let writes_line = match auth::writes_allowed_by_default() {
        Ok(true) => "writes:      ALLOWED by standing opt-in (STAVE_ALLOW_WRITE or config)",
        Ok(false) => "writes:      guarded (read-only; --allow-write per call to override)",
        Err(_) => "writes:      guarded (config unreadable)",
    };
    writeln!(stdout, "{writes_line}")?;

    let mcp_key_line = match auth::resolve_mcp_api_key() {
        Ok(Some(k)) => format!(
            "mcp key:     configured (source: {}, length: {} bytes)",
            k.source.as_str(),
            k.token.len()
        ),
        Ok(None) => "mcp key:     unset (store via `stave mcp login --stdin`)".to_string(),
        Err(e) => format!("mcp key:     error ({e})"),
    };
    writeln!(stdout, "{mcp_key_line}")?;

    let mcp_profile_line = match auth::resolve_mcp_profile() {
        Ok(Some(p)) => format!("mcp profile: set (source: {})", p.source.as_str()),
        Ok(None) => "mcp profile: unset (set via `stave mcp login --profile <hex>`)".to_string(),
        Err(e) => format!("mcp profile: error ({e})"),
    };
    writeln!(stdout, "{mcp_profile_line}")?;

    drop(stdout);
    if !authenticated {
        std::process::exit(1);
    }
    Ok(())
}

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
        .ok_or_else(|| anyhow!("no discoverable config path (set $XDG_CONFIG_HOME or HOME)"))?;
    let cfg = auth::read_config().map_err(|e| anyhow!("{e}"))?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "# {}", path.display())?;
    match cfg {
        None => {
            writeln!(stdout, "(file does not exist yet)")?;
        }
        Some(cfg) => {
            match cfg.auth.token.as_deref() {
                Some(t) => writeln!(stdout, "[auth]\ntoken = \"<redacted, length={}>\"", t.len())?,
                None => writeln!(stdout, "[auth]\n# token: (unset)")?,
            }
            writeln!(stdout, "\n[default]")?;
            match cfg.default.subdomain.as_deref() {
                Some(v) => writeln!(stdout, "subdomain = \"{v}\"")?,
                None => writeln!(stdout, "# subdomain: (unset)")?,
            }
            match cfg.default.region.as_deref() {
                Some(v) => writeln!(stdout, "region = \"{v}\"")?,
                None => writeln!(stdout, "# region: (unset — defaults to us)")?,
            }
            match cfg.default.allow_writes {
                Some(v) => writeln!(stdout, "allow_writes = {v}")?,
                None => writeln!(stdout, "# allow_writes: (unset — defaults to false)")?,
            }
            writeln!(stdout, "\n[mcp]")?;
            match cfg.mcp.api_key.as_deref() {
                Some(k) => {
                    writeln!(stdout, "api_key = \"<redacted, length={}>\"", k.len())?;
                }
                None => writeln!(stdout, "# api_key: (unset — keyring preferred)")?,
            }
            match cfg.mcp.profile.as_deref() {
                Some(v) => writeln!(stdout, "profile = \"{v}\"")?,
                None => writeln!(stdout, "# profile: (unset)")?,
            }
            match cfg.mcp.url.as_deref() {
                Some(v) => writeln!(stdout, "url = \"{v}\"")?,
                None => writeln!(stdout, "# url: (unset — derived from subdomain)")?,
            }
        }
    }
    Ok(())
}

fn config_path_cmd() -> anyhow::Result<()> {
    let path = auth::config_path()
        .ok_or_else(|| anyhow!("no discoverable config path (set $XDG_CONFIG_HOME or HOME)"))?;
    println!("{}", path.display());
    Ok(())
}

const CONFIG_KEYS: &str = "subdomain, region, allow_writes, auth.token, mcp.profile, mcp.url";

fn config_set(key: &str, value: &str) -> anyhow::Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(anyhow!(
            "value must not be empty — use `stave config unset {key}` to clear"
        ));
    }
    let owned = trimmed.to_string();
    let path = match key {
        "subdomain" => auth::write_config(|cfg| cfg.default.subdomain = Some(owned)),
        "region" => {
            if trimmed != "us" && trimmed != "eu" {
                return Err(anyhow!("region must be `us` or `eu`, got {trimmed:?}"));
            }
            auth::write_config(|cfg| cfg.default.region = Some(owned))
        }
        "allow_writes" => {
            let parsed: bool = trimmed
                .parse()
                .map_err(|_| anyhow!("allow_writes must be `true` or `false`, got {trimmed:?}"))?;
            auth::write_config(|cfg| cfg.default.allow_writes = Some(parsed))
        }
        "auth.token" => auth::write_config(|cfg| cfg.auth.token = Some(owned)),
        "mcp.profile" => auth::write_config(|cfg| cfg.mcp.profile = Some(owned)),
        "mcp.url" => auth::write_config(|cfg| cfg.mcp.url = Some(owned)),
        other => {
            return Err(anyhow!(
                "unknown key '{other}'. Known keys: {CONFIG_KEYS}. \
                 (Secrets belong in the platform keyring — prefer \
                 `stave auth login` / `stave mcp login` over config storage.)"
            ));
        }
    }
    .map_err(|e| anyhow!("{e}"))?;
    eprintln!("stave config: set {key} in {}", path.display());
    Ok(())
}

fn config_unset(key: &str) -> anyhow::Result<()> {
    let path = match key {
        "subdomain" => auth::write_config(|cfg| cfg.default.subdomain = None),
        "region" => auth::write_config(|cfg| cfg.default.region = None),
        "allow_writes" => auth::write_config(|cfg| cfg.default.allow_writes = None),
        "auth.token" => auth::write_config(|cfg| cfg.auth.token = None),
        "mcp.profile" => auth::write_config(|cfg| cfg.mcp.profile = None),
        "mcp.url" => auth::write_config(|cfg| cfg.mcp.url = None),
        other => {
            return Err(anyhow!("unknown key '{other}'. Known keys: {CONFIG_KEYS}."));
        }
    }
    .map_err(|e| anyhow!("{e}"))?;
    eprintln!("stave config: cleared {key} in {}", path.display());
    Ok(())
}

fn auth_logout() -> anyhow::Result<()> {
    let removed = auth::delete_keyring().map_err(|e| anyhow!("{e}"))?;
    if removed {
        eprintln!("stave auth: keyring entry removed");
    } else {
        eprintln!("stave auth: no keyring entry to remove");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// MCP subcommand
// ---------------------------------------------------------------------------

fn run_mcp(args: McpArgs) -> anyhow::Result<()> {
    match args.cmd {
        McpCmd::Login(login) => mcp_login(login),
        McpCmd::Status => mcp_status(),
        McpCmd::Config { reveal } => mcp_config(reveal),
        McpCmd::Tools { filter } => mcp_tools(filter),
        McpCmd::Call(call) => mcp_call(call),
        McpCmd::Map => mcp_map(),
    }
}

fn mcp_login(args: McpLoginArgs) -> anyhow::Result<()> {
    let key_source_provided = args.api_key.is_some() || args.stdin;
    let profile = args
        .profile
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let url = args.url.as_deref().map(str::trim).filter(|s| !s.is_empty());

    if !key_source_provided && profile.is_none() && url.is_none() {
        return Err(anyhow!(
            "no source. Pass `--api-key <value>`, `--stdin`, `--profile <hex>`, \
             or `--url <url>`. See `stave mcp login --help`."
        ));
    }

    if key_source_provided {
        let key = if let Some(k) = args.api_key.as_deref() {
            k.to_string()
        } else {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("read --stdin")?;
            buf
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(anyhow!("MCP API key must not be empty"));
        }
        if !key.starts_with("sk_live:") && !key.starts_with("sk_test:") {
            eprintln!(
                "stave mcp: warning — key does not carry the expected `sk_live:` prefix; \
                 storing anyway"
            );
        }
        auth::store_mcp_keyring(key).map_err(|e| anyhow!("{e}"))?;
        eprintln!("stave mcp: API key stored in keyring");
    }

    if profile.is_some() || url.is_some() {
        let profile_owned = profile.map(str::to_string);
        let url_owned = url.map(str::to_string);
        let path = auth::write_config(|cfg| {
            if let Some(p) = profile_owned {
                cfg.mcp.profile = Some(p);
            }
            if let Some(u) = url_owned {
                cfg.mcp.url = Some(u);
            }
        })
        .map_err(|e| anyhow!("{e}"))?;
        let mut updated: Vec<&str> = Vec::new();
        if profile.is_some() {
            updated.push("mcp.profile");
        }
        if url.is_some() {
            updated.push("mcp.url");
        }
        eprintln!(
            "stave mcp: persisted {} to {}",
            updated.join(" + "),
            path.display()
        );
    }
    Ok(())
}

fn mcp_status() -> anyhow::Result<()> {
    let mut stdout = std::io::stdout().lock();
    match mcp::resolve_credentials() {
        Ok(creds) => {
            writeln!(stdout, "url:     {}", creds.url)?;
            writeln!(
                stdout,
                "api key: configured (source: {}, length: {} bytes)",
                creds.api_key_source.as_str(),
                creds.api_key.len()
            )?;
            writeln!(stdout, "profile: set ({} chars)", creds.profile.len())?;
        }
        Err(e) => {
            writeln!(stdout, "not configured: {e}")?;
            drop(stdout);
            std::process::exit(1);
        }
    }
    Ok(())
}

fn mcp_config(reveal: bool) -> anyhow::Result<()> {
    let creds = mcp::resolve_credentials().map_err(|e| anyhow!("{e}"))?;
    let key = if reveal {
        creds.api_key.clone()
    } else {
        "<redacted — rerun with --reveal>".to_string()
    };
    let config = json!({
        "mcpServers": {
            "iru": {
                "url": creds.url,
                "type": "http",
                "headers": {
                    "X-API-Key": key,
                    "X-MCP-Profile": creds.profile,
                }
            }
        }
    });
    println!("{}", serde_json::to_string_pretty(&config)?);
    if !reveal {
        eprintln!(
            "stave mcp: API key redacted. Rerun with --reveal to emit a paste-ready config."
        );
    }
    Ok(())
}

fn mcp_tools(filter: Option<String>) -> anyhow::Result<()> {
    let span = audit::Span::start_fresh().with_verb_phase("mcp");
    let client = mcp::McpClient::from_env().map_err(|e| anyhow!("{e}"))?;
    let tools = block_on(async {
        client.initialize().await?;
        client.tools_list().await
    })
    .map_err(|e| anyhow!("{e}"))?;

    let needle = filter.map(|f| f.to_lowercase());
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

    let mut extra = serde_json::Map::new();
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
        let standing = resolve_allow_write(args.allow_write)?;
        if !standing {
            return Err(anyhow!(
                "write-guard: MCP tool '{}' is not read-shaped (get-*/list-*) and stave \
                 is read-only by default against the live tenant. To proceed deliberately, \
                 pass --allow-write, set STAVE_ALLOW_WRITE=1, or persist \
                 `stave config set allow_writes true`.",
                args.tool
            ));
        }
    }

    let arguments: Value = match args.args.as_deref() {
        Some(raw) => serde_json::from_str(raw).context("--args must be valid JSON")?,
        None => json!({}),
    };
    if !arguments.is_object() {
        return Err(anyhow!("--args must be a JSON object"));
    }

    let span = audit::Span::start_fresh().with_verb_phase("mcp");
    let client = mcp::McpClient::from_env().map_err(|e| anyhow!("{e}"))?;
    let result = block_on(async {
        client.initialize().await?;
        client.tools_call(&args.tool, arguments.clone()).await
    });

    let mut extra = serde_json::Map::new();
    extra.insert("mcp_method".into(), json!("tools/call"));
    extra.insert("mcp_tool".into(), json!(args.tool));
    extra.insert("mcp_url".into(), json!(client.url()));
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

/// Crosswalk live MCP tool names to local REST operationIds. The iru
/// MCP tool vocabulary is kebab-case over roughly the same resource
/// paths; the local synthesized ids are snake_case over the REST
/// paths. A direct normalization catches the `get-*`/`list-*` reads;
/// unmatched tools emit `operation_id: null` so the gap itself is
/// data.
fn mcp_map() -> anyhow::Result<()> {
    let client = mcp::McpClient::from_env().map_err(|e| anyhow!("{e}"))?;
    let tools = block_on(async {
        client.initialize().await?;
        client.tools_list().await
    })
    .map_err(|e| anyhow!("{e}"))?;

    let r = registry();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for (idx, tool) in tools.iter().enumerate() {
        let candidate = tool.name.replace('-', "_");
        let matched = r.find(&candidate).ok().map(|op| op.id.clone());
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

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

fn run_list(args: ListArgs) -> anyhow::Result<()> {
    let spec = kind_spec(&args.kind).ok_or_else(|| {
        anyhow!(
            "unknown kind '{}' — run with --help to see the v0.1 set",
            args.kind
        )
    })?;
    let op_id = spec.list_operation_id.ok_or_else(|| {
        anyhow!(
            "kind '{}' has no list endpoint in the v0.1 spec — derive it from another kind via `enrich`",
            spec.name
        )
    })?;

    // Validate --since before any network call so format errors don't
    // burn an API request.
    let since_program = build_since_program(spec, args.since.as_deref())?;
    let params = Value::Object(parse_params(&args.params)?);
    let response = call_op_blocking_for_verb(
        op_id,
        params,
        args.subdomain.as_deref(),
        false,
        args.no_audit,
        "list",
        &[spec.id_field],
    )?;
    let items_owned: Vec<Value> = match kinds::extract_items(&response) {
        Some(items) => items.to_vec(),
        None => vec![response],
    };

    let now = chrono_now();

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut emitted = 0usize;
    for (idx, item) in items_owned.into_iter().enumerate() {
        let record = Record::wrap(spec.name, SourceRef::now(op_id, idx), item);
        if let Some(prog) = &since_program {
            if !cel::evaluate(prog, &record, now, "<--since predicate>")
                .map_err(|e| anyhow!("{e}"))?
            {
                continue;
            }
        }
        write_record(&mut out, &record).map_err(|e| anyhow!("{e}"))?;
        emitted += 1;
        if let Some(limit) = args.limit
            && emitted >= limit
        {
            break;
        }
    }
    Ok(())
}

fn run_get(args: GetArgs) -> anyhow::Result<()> {
    let spec = kind_spec(&args.kind).ok_or_else(|| {
        anyhow!(
            "unknown kind '{}' — run with --help to see the v0.1 set",
            args.kind
        )
    })?;
    let op_id = spec.get_operation_id.ok_or_else(|| {
        anyhow!(
            "kind '{}' has no get-by-id endpoint in v0.1 — try `stave list {} | stave filter --where '<id-field> == \"<your-id>\"'`",
            spec.name,
            spec.name
        )
    })?;
    let id_param = spec.id_path_param.ok_or_else(|| {
        anyhow!(
            "kind '{}' has a get endpoint but no declared id path parameter — file a bug",
            spec.name
        )
    })?;

    let mut params = parse_params(&args.params)?;
    params.insert(id_param.to_string(), Value::String(args.id.clone()));

    let response = call_op_blocking_for_verb(
        op_id,
        Value::Object(params),
        args.subdomain.as_deref(),
        false,
        args.no_audit,
        "get",
        &[spec.id_field],
    )?;
    let record = Record::wrap(spec.name, SourceRef::now(op_id, 0), response);

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    write_record(&mut out, &record).map_err(|e| anyhow!("{e}"))?;
    Ok(())
}

fn run_search(args: SearchArgs) -> anyhow::Result<()> {
    let spec = kind_spec(&args.kind).ok_or_else(|| {
        anyhow!(
            "unknown kind '{}' — run with --help to see the v0.1 set",
            args.kind
        )
    })?;
    let op_id = spec.list_operation_id.ok_or_else(|| {
        anyhow!(
            "kind '{}' has no list endpoint in v0.1 — search composes on top of list",
            spec.name
        )
    })?;
    let search_field = spec.search_field.ok_or_else(|| {
        anyhow!(
            "kind '{}' has no search field declared in v0.1 — operators compose `list | filter` instead",
            spec.name
        )
    })?;

    let params = Value::Object(parse_params(&args.params)?);
    let response = call_op_blocking_for_verb(
        op_id,
        params,
        args.subdomain.as_deref(),
        false,
        args.no_audit,
        "search",
        &[spec.id_field],
    )?;
    let items_owned: Vec<Value> = match kinds::extract_items(&response) {
        Some(items) => items.to_vec(),
        None => vec![response],
    };

    let needle = args.query.to_lowercase();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut emitted = 0usize;
    for (idx, item) in items_owned.into_iter().enumerate() {
        let record = Record::wrap(spec.name, SourceRef::now(op_id, idx), item);
        let Some(field_value) = record.get(search_field) else {
            continue;
        };
        let Some(haystack) = field_value.as_str() else {
            continue;
        };
        if !haystack.to_lowercase().contains(&needle) {
            continue;
        }
        write_record(&mut out, &record).map_err(|e| anyhow!("{e}"))?;
        emitted += 1;
        if let Some(limit) = args.limit
            && emitted >= limit
        {
            break;
        }
    }
    Ok(())
}

/// Resolve the effective write permission: per-call flag first, then
/// the standing opt-in chain (STAVE_ALLOW_WRITE env, config
/// `allow_writes = true`).
fn resolve_allow_write(flag: bool) -> anyhow::Result<bool> {
    if flag {
        return Ok(true);
    }
    auth::writes_allowed_by_default().map_err(|e| anyhow!("{e}"))
}

fn call_op_blocking_for_verb(
    op_id: &str,
    params: Value,
    subdomain: Option<&str>,
    allow_write: bool,
    no_audit: bool,
    verb_phase: &'static str,
    synthesis_keys: &[&str],
) -> anyhow::Result<Value> {
    let client = Client::from_env_with_subdomain(subdomain).map_err(|e| anyhow!("{e}"))?;
    let mut path_params_source = BTreeMap::new();
    if let Some(src) = client.subdomain_source() {
        // The subdomain isn't a path param, but its provenance is the
        // same chain signal the mining surface wants; record it under
        // a reserved name.
        path_params_source.insert("_subdomain".to_string(), src);
    }
    let opts = CallOptions {
        no_audit,
        verb_phase: Some(verb_phase),
        synthesis_keys: synthesis_keys.iter().map(|s| s.to_string()).collect(),
        path_params_source,
        allow_write,
        ..Default::default()
    };
    block_on(client.call_op(op_id, &params, opts)).map_err(|e| anyhow!("{e}"))
}

fn block_on<F, T, E>(fut: F) -> Result<T, E>
where
    F: std::future::Future<Output = Result<T, E>>,
    E: From<stave_sdk::StaveError>,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");
    runtime.block_on(fut)
}

/// Build a CEL post-filter program for `--since <duration>`. Returns
/// `None` when `--since` was not supplied. Errors when the kind has no
/// primary timestamp field — `--since` has no field to compare against.
///
/// The compiled predicate is `<primary_ts_field> > now - duration("<dur>")`,
/// reusing the cel adapter so timestamp promotion + `now` binding apply
/// consistently. CEL's `duration()` accepts Go-style durations
/// (`24h`, `30m`, `1h30m`). cel-rust 0.10 accepts malformed inputs at
/// compile time and only fails at runtime, so we pre-validate here to
/// fail before any network call.
fn build_since_program(
    spec: &stave_sdk::KindSpec,
    since: Option<&str>,
) -> anyhow::Result<Option<cel_interpreter::Program>> {
    let Some(dur) = since else {
        return Ok(None);
    };
    let ts_field = spec.primary_timestamp_field.ok_or_else(|| {
        anyhow!(
            "kind '{}' has no primary timestamp field — `--since` is not applicable",
            spec.name
        )
    })?;
    if dur.contains('"') {
        return Err(anyhow!("--since must not contain quotes: {dur:?}"));
    }
    if !is_valid_go_duration(dur) {
        return Err(anyhow!(
            "--since: invalid duration {dur:?} — expected Go-style (e.g. 24h, 30m, 1h30m). \
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

    let mut extra = serde_json::Map::new();
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

/// Stable, value-independent fingerprint of the parsed CEL program.
/// Two predicates with the same shape (same operators, same identifiers,
/// different literals) produce the same hash. Used by audit miners to
/// cluster predicates across runs.
///
/// Implementation: take the program's Debug repr, strip every quoted
/// string and every numeric literal (including AST node IDs) to a
/// constant placeholder, then sha256. This keeps operator structure +
/// identifier names + nesting shape; drops literal contents + node
/// IDs so cosmetic differences don't fragment the hash space.
fn predicate_ast_shape(program: &cel_interpreter::Program) -> String {
    use sha2::{Digest, Sha256};
    let debug = format!("{program:?}");
    let stripped = strip_literals(&debug);
    let mut hasher = Sha256::new();
    hasher.update(stripped.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn strip_literals(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push('"');
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    if nc == '\\' {
                        chars.next();
                    } else if nc == '"' {
                        break;
                    }
                }
                out.push('"');
            }
            c if c.is_ascii_digit() => {
                while let Some(&nc) = chars.peek() {
                    if nc.is_ascii_digit() || nc == '.' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push('0');
            }
            other => out.push(other),
        }
    }
    out
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
        "the full record is also available as `record` for `has()` checks."
    )?;
    Ok(())
}

fn chrono_now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

fn run_enrich(args: EnrichArgs) -> anyhow::Result<()> {
    let recipe = enrich::Recipe::parse(&args.recipe).ok_or_else(|| {
        anyhow!(
            "unknown recipe '{}'. v0.1 recipes: blueprint-context, severity-roll-up, device-platform",
            args.recipe
        )
    })?;

    let ctx = build_enrichment_context(args.blueprints.as_deref())?;
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

    let mut extra = serde_json::Map::new();
    extra.insert("recipe_id".into(), json!(recipe.as_str()));
    extra.insert(
        "transform_outcome".into(),
        json!({
            "transformed_count": transformed,
            "error_count": errors,
        }),
    );
    extra.insert(
        "auxiliary".into(),
        json!({
            "blueprints_loaded": ctx.blueprints_by_id.len(),
        }),
    );
    span.finish_as_verb(extra);

    match last_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn build_enrichment_context(
    blueprints_path: Option<&std::path::Path>,
) -> anyhow::Result<enrich::EnrichmentContext> {
    let Some(path) = blueprints_path else {
        return Ok(enrich::EnrichmentContext::default());
    };
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::new(file);
    let blueprints: Vec<Record> = read_stream(reader)
        .collect::<stave_sdk::Result<Vec<_>>>()
        .map_err(|e| anyhow!("read --blueprints {}: {e}", path.display()))?;
    if blueprints.iter().any(|b| b.kind != "blueprint") {
        return Err(anyhow!(
            "--blueprints {} contains records of kinds other than `blueprint`",
            path.display()
        ));
    }
    Ok(enrich::EnrichmentContext::with_blueprints(blueprints))
}

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
            // Buffer so we can emit the header once and rows after.
            let records: Vec<Record> = read_stream(stdin)
                .collect::<stave_sdk::Result<Vec<_>>>()
                .map_err(|e| anyhow!("{e}"))?;
            emit_markdown_table(&mut out, &records)?;
        }
    }
    Ok(())
}

/// Render a small markdown table of a stream of records.
///
/// Columns: `_kind`, `id`, `severity`, primary timestamp. The id /
/// severity / timestamp field names come from `KindSpec`. Records of
/// unknown kinds use generic fallbacks (`id`, `severity`, no timestamp).
fn emit_markdown_table<W: Write>(out: &mut W, records: &[Record]) -> anyhow::Result<()> {
    writeln!(out, "| _kind | id | severity | timestamp |")?;
    writeln!(out, "|---|---|---|---|")?;
    for r in records {
        let spec = kind_spec(&r.kind);
        let id_field = spec.map(|s| s.id_field).unwrap_or("id");
        let sev_field = spec.and_then(|s| s.severity_field).unwrap_or("severity");
        let ts_field = spec.and_then(|s| s.primary_timestamp_field);

        let id = r.get(id_field).and_then(scalar).unwrap_or_default();
        let sev = r.get(sev_field).and_then(scalar).unwrap_or_default();
        let ts = ts_field
            .and_then(|f| r.get(f))
            .and_then(scalar)
            .unwrap_or_default();

        writeln!(out, "| {} | {} | {} | {} |", r.kind, id, sev, ts)?;
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

fn kind_value_parser() -> clap::builder::PossibleValuesParser {
    clap::builder::PossibleValuesParser::new(kinds::ALL_KINDS)
}

fn parse_params(raw: &[String]) -> anyhow::Result<Map<String, Value>> {
    let mut out = Map::new();
    for entry in raw {
        let (k, v) = entry
            .split_once('=')
            .ok_or_else(|| anyhow!("--param expects `key=value`, got `{entry}`"))?;
        // Try to parse the value as JSON first (lets users pass numbers, bools,
        // arrays without quoting). Fall back to plain string.
        let value = serde_json::from_str(v).unwrap_or(Value::String(v.to_string()));
        out.insert(k.to_string(), value);
    }
    Ok(out)
}

#[cfg(test)]
mod since_tests {
    use super::is_valid_go_duration;

    #[test]
    fn accepts_simple_units() {
        assert!(is_valid_go_duration("24h"));
        assert!(is_valid_go_duration("30m"));
        assert!(is_valid_go_duration("60s"));
        assert!(is_valid_go_duration("500ms"));
        assert!(is_valid_go_duration("100ns"));
        assert!(is_valid_go_duration("100us"));
        assert!(is_valid_go_duration("100µs"));
    }

    #[test]
    fn accepts_compound_durations() {
        assert!(is_valid_go_duration("1h30m"));
        assert!(is_valid_go_duration("2h45m30s"));
    }

    #[test]
    fn accepts_decimal() {
        assert!(is_valid_go_duration("1.5h"));
        assert!(is_valid_go_duration("0.25s"));
    }

    #[test]
    fn rejects_empty() {
        assert!(!is_valid_go_duration(""));
    }

    #[test]
    fn rejects_unknown_unit() {
        assert!(!is_valid_go_duration("7d"));
        assert!(!is_valid_go_duration("1w"));
    }

    #[test]
    fn rejects_no_unit() {
        assert!(!is_valid_go_duration("24"));
    }

    #[test]
    fn rejects_no_number() {
        assert!(!is_valid_go_duration("h"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(!is_valid_go_duration("not-a-real-duration"));
        assert!(!is_valid_go_duration("24h-extra"));
    }
}
