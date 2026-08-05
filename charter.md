# stave Charter

> Re-introduction document for stave, an unofficial Rust CLI for the
> Wiz API (not affiliated with or endorsed by Wiz, Inc.). Restores
> context for a collaborator who was present but does not persist.
> Follows the kos process: Orient → Ideate → Question → Probe →
> Harvest → Promote.

Last updated: 2026-08-05 (scaffold session)

---

## The Problem Statement

Agents working on cloud-security operations need programmatic access
to the Wiz API. Wiz's surface is GraphQL
(`https://api.<region>.app.wiz.io/graphql`), authenticated by OAuth2
client-credentials service accounts, with a hosted MCP server at
`https://mcp.app.wiz.io`. There is a first-party scanner binary
(wizcli, a different tool for scanning artifacts) and an MCP story,
but no first-party general-purpose CLI over the API — agents either
write ad-hoc curl wrappers per session or hand the whole conversation
to MCP without a durable usage record.

A CLI alone is not enough. To eventually curate *composite verbs* —
the workflows Wiz operators actually run (issue triage, vulnerability
exposure sweeps, resource inventory, compliance posture checks) — we
need a durable record of how the API is used in practice. That record
has to be structured enough for a future LLM session to mine for
patterns, and it has to capture every call: GraphQL and MCP alike.

Like bloomctl and unlike sidestep: **the only tenant we can develop
against is in-use production.** Safety posture is a design value, not
an afterthought.

## Design Values

1. **Schema is the contract.** The vendored GraphQL schema (once
   introspected — F2) pins the surface stave talks to. Curated
   operations in `stave-api` are checked against it (`cargo xtask
   check-ops`); `stave api` runs ad-hoc documents through the same
   parser and write-guard. Keeping up with the vendor is one
   `sync-spec` away.
2. **SDK-first.** All shared logic — auth chains, token mint + cache,
   audit emission, redaction, the write-guard, the MCP client — lives
   in `stave-sdk`. The CLI is presentation; future MCP surfaces are
   sibling consumers.
3. **Audit trail is a feature, not a log.** Every GraphQL call and MCP
   call writes a structured JSONL line locally. See
   `docs/audit-trail-format.md`.
4. **Production-safe by default.** GraphQL mutations refuse without
   explicit opt-in (`--allow-write` / env / config). The guard applies
   to curated operations, ad-hoc documents (parsed before they reach
   the wire), and non-read MCP tools.
5. **Agent-first ergonomics.** JSONL for non-TTY, predictable verb
   shape, stable exit codes, chain-naming errors.
6. **User sovereignty.** Local-first audit trail, local config,
   keychain custody for secrets, no phone-home, no telemetry.
7. **Nominative use only.** The Wiz mark stays out of the binary name,
   org name, and any logo; prose says what the tool talks to and that
   it is unofficial.

## Non-Goals

- **Not a curl replacement.** No raw HTTP escape hatch (inherited
  ruling — sidestep G1). `stave api --query <doc>` is the spec-aware
  escape hatch: still GraphQL, still parsed, still write-guarded,
  still audited.
- **Not a multi-tenant service.** stave is a local CLI. Authentication
  is per-user, per-tenant.
- **Not a remediation console.** Mutations (resolve issue, rotate
  secrets, delete report) stay behind the write-guard; stave is not
  trying to make them frictionless.
- **Not wizcli.** The vendor's scanner binary solves a different
  problem (artifact/IaC scanning). stave is an API operations tool.

---

## Bedrock

*Established. Evidence-based or decided with rationale.*

### B1: The Sidestep Pattern Transfers to GraphQL

stave was scaffolded by porting bloomctl's entire workspace (4 crates
+ xtask + tests + CI + rules) and re-instantiating the vendor-specific
~30%. The vendor-agnostic ~70% — stream contract, CEL adapter, audit
machinery, redaction, keyring handling, wiremock harness, distribution
workflows — transferred again, now across an API *paradigm* change
(REST/OpenAPI → GraphQL), not just a vendor change. What changed:
the generated client became a curated operation library; the operation
registry reads GraphQL documents instead of an OpenAPI spec; the
write-guard classifies operation types instead of HTTP methods.

Third instantiation of the pattern (sidestep → bloomctl → stave).
Evidence: this scaffold session; orc finding (stave-scaffold).

### B2: Curated GraphQL Operations, Not Whole-Schema Codegen

The Wiz schema is large and introspection requires tenant credentials.
Instead of generating a client from the whole schema (progenitor's
analog does not exist for GraphQL at acceptable compile cost),
`stave-api` ships curated `.graphql` documents with a static registry
(name, op_type, root_field, description). `cargo xtask check-ops`
validates documents against the vendored schema once `sync-spec` has
introspected it. Field selections are conservative and **provisional
until live-validated** (F1) — a wrong guess fails loudly as a GraphQL
validation error, never silently.

### B3: Auth — OAuth Client-Credentials with Keychain Custody and a Real Derivation Layer

Fourth val-resolution-chain instantiation, and the first where the
chain's *derivation* layer is real:

- client ID: flag → env (`STAVE_CLIENT_ID`) → config.
- client secret: env → **platform keyring** (`stave auth login`
  prompts and stores) → config (discouraged) → chain-naming error.
- API endpoint: flag → env (`STAVE_API_URL`) → config → **derived
  from the minted token's data-center claim** → chain-naming error.
- Minted tokens are cached in the XDG state dir (mode 0600), keyed by
  (token_url, client_id), refreshed inside a 5-minute expiry margin.
  Tokens are short-lived derivatives of the secret; they do not live
  in config or keyring.
- `STAVE_ACCESS_TOKEN` short-circuits the mint (CI, wiremock).

The audit trail records `auth_source`, `api_url_source`, and
`path_params_source._api_url` so the mining surface separates
constant defaults from per-call intent.

### B4: Write-Guard — Mutations Refuse by Default, Ad-Hoc Documents Included

Any GraphQL mutation errors with a repair-friendly `WriteGuard`
message unless allowed by: `--allow-write` (per call) →
`STAVE_ALLOW_WRITE` env → `[default] allow_writes = true` config.
Ad-hoc documents (`stave api --query`) are parsed before anything
reaches the wire: a mutation or subscription anywhere in the document
trips the guard, and an unparseable document is refused outright. The
same posture gates MCP `tools/call` for tools whose names are not
read-shaped (conservative allowlist of read prefixes; unknown shapes
are write-gated). Enforced in the SDK so every consumer inherits it.

### B5: Registry Credential Custody

Wiz tenants pull vendor images from a container registry with a
tenant-scoped username and password. `stave registry login` stores the
password in the platform keyring (env → keyring → config chain, same
shape as everything else) and `stave registry credential --reveal`
feeds `docker login --password-stdin`. The username embeds the tenant
ID and is therefore tenant-identifying (tenant-data-hygiene rule).

---

## Frontier

*Actively open. Expected to resolve through design work or probes.*

### F1: Live Validation

Everything vendor-shaped is provisional until validated against a real
tenant: the curated field selections (B2), the kind-table metadata,
the token-mint request shape (audience value), the data-center claim
derivation, and the MCP transport (F3). Gated on a Wiz service
account with read scopes. First smoke: `auth login` → `auth status` →
`list issue --limit 5` → `api list_projects` → the filter/emit
pipeline on real payloads.

### F2: Schema Introspection, get-by-id, and Server-Side Filters

`cargo xtask sync-spec` introspects the schema once credentials exist;
`check-ops` then validates the curated documents. Open questions:
which kinds get `stave get <kind> <id>` (needs per-kind singular
queries or filter input types — deliberately not guessed at scaffold
time); which list operations grow `filterBy` variables (server-side
filtering vs the current client-side `stave filter`); whether
`graphSearch` becomes a first-class verb.

### F3: MCP Live Validation

The MCP client follows the streamable-HTTP shape verified against
another vendor's server (bloomctl B6/B8). Wiz's hosted server at
`https://mcp.app.wiz.io` needs live verification: auth handshake
(bearer from the same OAuth flow is the working assumption), tool
vocabulary (feeds the read-only heuristic), and the `mcp map`
crosswalk to curated operations.

### F4: Audit-Trail Mining → v0.2 Composite Verbs

Same question as the siblings: which composite verbs do Wiz workflows
want (`issue-triage`, `vuln-exposure`, `posture-report`)? Deferred
until real usage accumulates — the explicit purpose of shipping the
audit trail first.

### F5: Distribution

CI workflows (ci/alpha/release, harden-runner, gated signing) ported
from bloomctl and renamed. Signing gates on `vars.SIGNING_ENABLED`;
enabling requires adding stave to the org-level signing/notary/tap
secret allowlists (same 7 secrets the siblings use) and the `release`
environment. First alpha publishes `Formula/stave.rb` to
`ArcavenAE/homebrew-tap`.

---

## Graveyard

*Ruled out. Kept for the reasoning.*

### G1: Raw HTTP Escape Hatch (inherited)

Same ruling as sidestep G1 and bloomctl G1: the schema is the
canonical surface; a raw REST/HTTP hatch would let drift hide. The
GraphQL document hatch (`stave api --query`) is the sanctioned escape:
it stays inside the schema, the parser, the write-guard, and the audit
trail.

### G2: Whole-Schema Client Codegen

Considered generating a typed client from the full introspected schema
(graphql_client derive or similar). Ruled out for v0.1: the schema is
not yet vendorable (introspection needs credentials), compile cost on
a schema this size is real, and the CLI's stream contract only needs
JSON. Curated documents + runtime JSON give the same safety through
`check-ops` at a fraction of the cost. Reopen if typed responses earn
their weight in the SDK.

---

## Session Log

| Session | Date | Outcomes |
|---------|------|----------|
| Scaffold | 2026-08-05 | Repo created at ArcavenAE/stave by porting bloomctl wholesale and re-instantiating the vendor-specific 30% for the Wiz GraphQL API. Shipped: curated operation library (12 list operations, provisional field selections), SDK (OAuth client-credentials mint + XDG token cache + dc-claim endpoint derivation, client-id/secret/endpoint/registry chains, keychain custody, write-guard on mutations incl. parsed ad-hoc documents, audit v2 with api_url_source + graphql_error outcome, Wiz kind table ×12, CEL camelCase timestamp promotion), CLI (auth/registry/config/ops/api + 6 primitives + mcp family), xtask (sync-spec introspection + check-ops schema validation), synthetic Wiz fixtures + asserts + recipes, tenant-data-hygiene rule adapted to cloud-posture data, CI/signing workflows renamed. B1–B5 set, F1–F5 opened, G1–G2 ruled. |
