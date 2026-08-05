# stave Audit Trail Format

> **Sharing caveat:** audit lines are tenant-identifying even though
> they omit response bodies and tokens. They carry GraphQL variable
> values (cloud account ids, subscription ids, resource names), the
> region-bearing API hostname, pagination cursors that can embed tenant
> data, the local hostname and username, and CEL predicate text. Never
> paste raw lines into issues, PRs, discussions, or commits. Run
> shareable repros with `STAVE_AUDIT=off`, and reproduce against
> `examples/fixtures/` where you can. See SECURITY.md "Tenant Data
> Hygiene" and `.claude/rules/tenant-data-hygiene.md`.

> Status: schema_version 2, implemented in `crates/stave-sdk/src/audit.rs`.
> The schema below is the contract; deviations require bumping
> `schema_version`.

## Goals

1. Capture every API call stave makes, curated operation or `stave api`
   ad-hoc document, plus every MCP tool call, without exception.
2. Make the trail mineable by a future LLM session for *patterns*
   (which operations cluster, which fail together, which sequences hint
   at a composite verb) without that session needing to read payloads.
3. Never write secrets. Authentication headers and known sensitive
   response fields are redacted before write.
4. Stay local. The trail is one user's record on one machine. OTEL
   export is a future opt-in, not a requirement.

## Location

- Default: `$XDG_STATE_HOME/stave/audit/YYYY-MM-DD.jsonl`, which is
  `~/.local/state/stave/audit/` on Linux.
- Fallback when no state dir is available: `~/.stave/audit/YYYY-MM-DD.jsonl`.
  macOS takes this path, since `dirs::state_dir()` is `None` there.
- Override the directory: `STAVE_AUDIT_DIR`.
- Disable globally: `STAVE_AUDIT=off`, matched case-insensitively.
  Nothing is written at all.
- Disable per-call: `--no-audit`, which still writes a stub line (see
  below).

Files roll at UTC midnight. Old files are not pruned by stave, and that
is intentional: the trail is the corpus, so pruning is the user's call.

## What is and is not audited

Audited: every GraphQL call (curated operation or ad-hoc document),
every MCP `tools/list` and `tools/call`, and the stream-transform verbs
`filter` and `enrich`, which emit a verb-shape line with no operation or
response section.

Not audited among the verbs: **`emit`**. It is a pure sink that makes no
call and changes no record, so a line would record only that formatting
happened. A pipeline's shape is already recoverable from the `filter` and
`enrich` lines that precede it. `get` is likewise absent because it is
not supported in v0.1.

Not audited: **token mint**. The OAuth client-credentials exchange
against the token endpoint is authentication machinery, not an API
call, and its request body carries the client secret. Minting is
visible indirectly through `invocation.auth_source` on the call that
triggered it. Registry credential handling is likewise not audited.

## Schema (v2)

One JSON object per line. Two shapes share a common header: the
API shape (below) and the verb shape (further down).

```json
{
  "schema_version": 2,
  "trace_id": "0198a2c1-7f3e-7c21-9b04-000000000000",
  "span_id": "0198a2c1-7f3e-7c21-9b04-000000000001",
  "parent_span_id": null,
  "ts_start": "2026-08-05T18:42:11.234Z",
  "duration_ms": 187,
  "invocation": {
    "argv": ["stave", "list", "issue", "--limit", "50"],
    "binary_version": "0.1.0",
    "host": "example-host",
    "user": "operator",
    "tty": false,
    "auth_source": "keyring",
    "api_url_source": "derived"
  },
  "verb_phase": "list",
  "synthesis_keys": ["id"],
  "path_params_source": {"_api_url": "derived"},
  "operation": {
    "id": "list_issues",
    "method": "query",
    "url_template": "issuesV2",
    "path_params": {"first": 50, "after": null},
    "query_params": {}
  },
  "response": {
    "status": 200,
    "size_bytes": 14823,
    "items_returned": 50,
    "next_cursor": "<opaque cursor>",
    "shape_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
  },
  "result": "ok",
  "redacted_fields": ["authorization"]
}
```

### Header fields

| Field | Purpose |
|-------|---------|
| `schema_version` | Integer. Bump on incompatible schema change. v2 is current. |
| `trace_id` | UUIDv7. Shared across one CLI invocation; every page of a paginated list shares one. |
| `span_id` | UUIDv7. Unique per call. |
| `parent_span_id` | UUIDv7 or null. For nested operations (a composite verb that fans out). |
| `ts_start` | RFC 3339 UTC, millisecond precision, from when the request was sent. |
| `duration_ms` | Integer milliseconds, request-send to response-fully-read. |
| `invocation.argv` | Full argv with secret values redacted. |
| `invocation.binary_version` | `CARGO_PKG_VERSION` of the running stave. |
| `invocation.host` | Hostname, best-effort. |
| `invocation.user` | `$USER`, best-effort. |
| `invocation.tty` | `true` if stdout is a tty. |
| `invocation.auth_source` | Where the client secret was resolved from: `"env"` \| `"keyring"` \| `"config"` \| `null`. `null` means an explicit-token constructor was used and nothing was resolved. |
| `invocation.api_url_source` | Where the API endpoint came from: `"flag"` \| `"env"` \| `"config"` \| `"derived"`. `"derived"` means it came from the minted token's data-center claim. `null` for explicit-base-URL clients (tests) and MCP-only spans. |
| `verb_phase` | Which verb emitted the line: `list`, `search`, `api`, `filter`, `enrich`, or `mcp`. Absent on legacy-shape lines. |
| `synthesis_keys` | The kind's primary-key field names, so a miner can join records across runs without re-deriving them. Omitted when empty. |
| `path_params_source` | Provenance per chain-resolved value, same vocabulary as `api_url_source`. Omitted when empty. The endpoint records under the reserved key `_api_url`. |

`api_url_source` replaces v1's REST-era `subdomain_source`: Wiz has one
region-bearing endpoint rather than a tenant subdomain, and the chain
gained a real derivation layer, so the field name and its value set both
changed.

### Operation fields (GraphQL semantics)

The `operation` object keeps its field names from the REST-era schema
and re-reads them for GraphQL. The names are the mining join keys, so
they were not renamed; what changed is what they carry.

| Field | Carries |
|-------|---------|
| `operation.id` | The curated operation name (`list_issues`) or, for `stave api --query`, the ad-hoc document's operation name. The stable join key for pattern mining. |
| `operation.method` | The GraphQL operation type: `"query"` or `"mutation"`. Not an HTTP method; every call is an HTTP POST to one endpoint, so the HTTP verb carries no signal. |
| `operation.url_template` | The GraphQL root field (`issuesV2`, `cloudResources`). This is what aggregates cleanly across calls, the way a path template did for REST. |
| `operation.path_params` | The GraphQL variables map (`{"first": 50, "after": "..."}`), redacted per policy. |
| `operation.query_params` | Always empty. GraphQL carries no query string; the key is retained so a v1-shaped miner keeps parsing v2 lines. |

### Response fields

| Field | Purpose |
|-------|---------|
| `response.status` | HTTP status. A GraphQL error typically arrives as 200, which is why `result` and not `status` is the outcome to read. |
| `response.size_bytes` | Decompressed bytes received. |
| `response.items_returned` | Integer when the response is a connection. Omitted otherwise. |
| `response.next_cursor` | `pageInfo.endCursor` when present. Omitted if absent. Cursors can embed tenant data; treat as sensitive. |
| `response.shape_hash` | sha256 over the redacted response *shape* (keys and types, not values). Detects schema drift without storing payloads. |

### Outcome

| Field | Purpose |
|-------|---------|
| `result` | `ok` \| `http_error` \| `graphql_error` \| `network_error` \| `auth_error` \| `redacted_block`. |
| `redacted_fields` | Field paths the redaction policy stripped. Useful for verifying the policy worked. |

`graphql_error` is new in v2 and it is the reason the outcome taxonomy
could not stay as it was. A GraphQL API answers a failed request with
HTTP 200 and an `errors` array, so a v1 miner counting non-200 statuses
would score a schema-validation failure or a permission denial as a
success. Anything with a non-empty top-level `errors` array records
`graphql_error`, whatever the status line said.

## Verb-shape lines

The stream transforms have no API call to describe, so they emit the
common header plus verb-specific fields and no `operation`, `response`,
or `result`:

```json
{
  "schema_version": 2,
  "trace_id": "0198a2c1-7f3e-7c21-9b04-00000000000a",
  "span_id": "0198a2c1-7f3e-7c21-9b04-00000000000b",
  "parent_span_id": null,
  "ts_start": "2026-08-05T18:42:12.001Z",
  "duration_ms": 4,
  "invocation": {
    "argv": ["stave", "enrich", "--with", "account-context", "--accounts", "accounts.jsonl"],
    "binary_version": "0.1.0",
    "host": "example-host",
    "user": "operator",
    "tty": false,
    "auth_source": null,
    "api_url_source": null
  },
  "verb_phase": "enrich",
  "recipe_id": "account-context",
  "auxiliary": {"accounts_loaded": 2},
  "transform_outcome": {"transformed_count": 4, "error_count": 0}
}
```

`recipe_id` is the recipe that ran. `auxiliary` counts what each
auxiliary stream contributed, and it counts *indexed* records rather
than lines read: a `cloud_account` with no `externalId` cannot
participate in the join, so a line count would overstate what the
recipe had to work with. `transform_outcome` separates records
transformed from records that errored.

`filter` lines carry a different set:

```json
{
  "verb_phase": "filter",
  "predicate_text": "severity == \"CRITICAL\"",
  "predicate_ast_shape": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
  "predicate_outcome": {"kept_count": 1, "dropped_count": 3, "error_count": 0}
}
```

`predicate_text` is the CEL source as written, so a miner can see which
predicates operators actually reach for. It can name real accounts and
resources, so it falls under the sharing caveat above.

`predicate_ast_shape` hashes the compiled predicate's structure, keeping
operators and schema field names and discarding the values compared
against. That combination is what makes it both a clustering key and
safe to share: field names come from the Wiz schema and describe no
tenant, while the values are exactly the part that names real accounts
and resources.

Measured 2026-08-05 against `target/debug/stave`, holding one variable
at a time:

```
63da2d876a35  <-  severity == "CRITICAL"
63da2d876a35  <-  severity == "HIGH"
63da2d876a35  <-  severity == "OPEN"
d3c89c56196c  <-  status == "OPEN"
a14249e21130  <-  type == "OPEN"
f0f1e8a0b123  <-  severity != "CRITICAL"
b0e76baa16aa  <-  severity == "CRITICAL" && status == "OPEN"
```

The first three share a hash, so the value is ignored. The next two hold
operator and value constant and separate, so the field is kept. The last
two change the operator and the structure and each separate. So the
field answers "which field, with what operator, in what shape", which is
the question the composite-verb analysis in charter F4 needs, and it
never answers "compared against what". Read `predicate_text` when the
value matters, and remember that it is the tenant-identifying half.

Properties pinned by five unit tests in `crates/stave-cli/src/main.rs`:
`ast_shape_ignores_literal_values`,
`ast_shape_distinguishes_the_field_filtered_on`,
`ast_shape_distinguishes_the_operator`,
`ast_shape_distinguishes_structure`, and
`strip_literals_keeps_operators_and_fields_drops_values_and_ids`.
Hash values are not part of the contract: they move whenever the
underlying AST representation changes, so compare shapes within one
binary version rather than across upgrades.

## Redaction Policy (initial)

- **Always redacted:** `authorization`, `x-api-key`, and any header
  named like a token. Authentication never lands in the trail.
- **Default-deny field names** in payloads: `secret`, `token`, `key`
  (when in a credential-shaped context, not `key` as a generic map
  key, which is a heuristic refined as real cases arrive), `password`,
  `client_secret`.
- **Registry credentials** never appear: the registry password is
  keyring-held and its reveal path is not an audited call.
- **User-defined extra paths:** `~/.config/stave/redaction.toml` adds
  paths beyond the defaults. It never *removes* defaults.

When a field is redacted, its path appears in `redacted_fields` and its
value never reaches disk.

## `--no-audit` and stub lines

`stave --no-audit <op>` skips the operation-level detail but still
writes:

```json
{
  "schema_version": 2,
  "trace_id": "0198a2c1-7f3e-7c21-9b04-00000000000c",
  "span_id": "0198a2c1-7f3e-7c21-9b04-00000000000d",
  "ts_start": "2026-08-05T18:42:13.500Z",
  "duration_ms": 187,
  "invocation": {"argv": ["stave", "--no-audit", "..."], "binary_version": "0.1.0"},
  "result": "redacted_block"
}
```

This preserves a usage signal (an operation happened, took roughly this
long, for this user, at this time) without recording its content. A
later analyzer can see *that* opted-out calls occur even if it cannot
see *what* they were. `STAVE_AUDIT=off` writes nothing at all: opt-out
is observable, silence is available, and the two are different choices.

## Environment variables

| Variable | Effect |
|---|---|
| `STAVE_AUDIT` | `off` disables the trail entirely. Any other value is ignored. |
| `STAVE_AUDIT_DIR` | Overrides the audit directory, ahead of the XDG state dir and the `~/.stave` fallback. |

Both are read on every emission, so they can be set per command.

## Future analysis surface

Out of scope for v0.1. Charter F4 tracks this. Likely shapes:

- `stave audit query --since 7d --group-by operation.id`, local
  aggregation over the JSONL.
- `stave audit traces`, rendering `trace_id` trees.
- `stave meta propose`, an LLM call over a window of the trail to
  suggest composite verbs.
- A flyloft catalog adapter that indexes audit lines for retrieval.

## Why this schema, specifically

- `operation.id` is the stable join key. Root fields aggregate by
  *operation*, not by concrete request, which keeps patterns visible
  across projects, accounts, and tenants.
- `trace_id` lets an analyzer ask "what does a triage session look
  like", because a workflow is a tree of related calls rather than a
  flat list.
- `shape_hash` over keys and types detects schema drift and unexpected
  response shapes without storing payloads. Cheap, and informative out
  of proportion to its cost.
- `result` as a taxonomy, with `graphql_error` distinct from
  `http_error`, lets an analyzer find operations that fail often enough
  to want a composite wrapper without parsing prose error messages.
- `path_params_source` separates per-call intent (`flag`) from constant
  defaults (`env`, `config`, `derived`). Without it, a near-constant
  value looks like a deliberate choice on every single line, and the
  mining surface reads noise as signal.
- `--no-audit` produces stub lines rather than silence, so opt-out is
  observable. Privacy-preserving, not invisibility-preserving.
