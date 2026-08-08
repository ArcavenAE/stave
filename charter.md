# stave Charter

> Re-introduction document for stave, an unofficial Rust CLI for the
> Wiz API (not affiliated with or endorsed by Wiz, Inc.). Restores
> context for a collaborator who was present but does not persist.
> Follows the kos process: Orient → Ideate → Question → Probe →
> Harvest → Promote.

Last updated: 2026-08-08 (field sweep, the directory scope route, connection metadata)

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

Qualified 2026-08-07: selection width was the wrong thing to worry
about first. The field-surface audit found the binding matters more —
`cloudResources` returns an eight-field type of which stave already
selects six, so no widening reaches those runbooks and only binding
`cloudResourcesV2` does. See `docs/design/field-surface-audit.md` and bd
`aae-orc-rsh6`.

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

### B4: Read-Only Posture — Mutations Refuse Unconditionally, Layered Permission Model

stave refuses every GraphQL mutation and subscription with a terminal,
byte-stable `WriteGuard` message: no `--allow-write`, no env var, no
config opt-in lifts it (the override surface was removed — a gate that
can never be commissioned against a live mutation is scenery, and an
override breadcrumb reads as an instruction to a high-temperature
agent). Ad-hoc documents (`stave api --query`) run only under the
exploratory read posture (`config set posture exploratory`); the
default curated posture refuses them. Unparseable documents are refused
outright; non-read MCP tools are refused the same way. Enforced in the
SDK so every consumer inherits it.

Around that boundary sit three more layers, none of them a security
control (the server's scope enforcement is): a registry that declares
`required_scopes` per operation (`check-ops` fails a verb that omits
them); the permission verbs `ops permissions` / `auth scopes` /
`auth can-i` / `auth plan [--check]` that report and provision
least-privilege credentials offline from the token's own scope claim;
and refusals audited as a first-class `result: "refused"` outcome so a
run of reformulated attempts in one session is visible. The real
boundary is a read-only service account; the client-side guard is
honest friction on top of it, documented as untested against a live
mutation.

Full design and the mandatory testing-safety rules:
`docs/design/read-only-posture-and-permissions-report.md` and
`docs/design/read-only-permissions-implementation-plan.md`. Scope names
are provisional until F1.

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

### F1: Live Validation [largely resolved 2026-08-06]

Validated against the production tenant, read-only (finding-001).
Confirmed: the token mint (audience `wiz-api`), the data-center claim
derivation, schema introspection (10,060 types vendored + sha256
pinned), the read pipeline end to end (`list project`), and 11 of 12
curated documents. Corrected: `CloudResource.cloudPlatform` (does not
exist → `nativeType`), `CloudAccount.status` (deprecated →
`lastScannedAt`/`resourceCount`), and a sync-spec SDL bug (empty types
emitted invalid `{}`).

Open finding, **resolved 2026-08-08 by finding-007**: with the service
account used during development, scope qualification did not manifest as
expected — the token did not expose readable granted scopes, so
`auth can-i` and `auth plan --check` refused to answer rather than emit
a false verdict (option (a), ratified). `ops permissions` and
`auth plan` provisioning were unaffected. See finding-001.

The token claim is still opaque and always will be. A second route now
answers the question: `--from-directory` on `auth scopes`, `auth can-i`
and `auth plan --check` reads the caller's own `ServiceAccount.scopes`
from the tenant directory. Validated live — 79 scopes readable, and
`missing: 0` against the twelve the registry declares, so those twelve
names are real. Opt-in rather than a silent fallback, because the
default must stay offline and deterministic.

`SCOPE_METADATA_PROVISIONAL` stays `true`. Twelve scope NAMES are
validated; the per-operation ASSIGNMENT is not, and a credential
holding 79 scopes cannot distinguish a correct mapping from a generous
one. Settling that needs a least-privilege credential.

Scope qualifier added 2026-08-06: "largely resolved" covers field
selections, not the read path. Every live probe so far ran at
`--limit 2`, so each returned one page, and two paging defects lived
below that depth until a pre-run review found them. See finding-002.

Session 2 of 2026-08-07, live: queue item 1 of the widening validation
is closed. The single inline fragment on `VulnerableAssetBase`, standing
in for fourteen union members, is accepted and returning (populated
20/20), so the fourteen-fragment fallback is dead. `x1rg` validated
live: `severity` is populated 20/20 and equals `vendorSeverity` on every
record, so the gap those two fields exist to expose is zero on this
sample. Seven queue items remain, several with a sampling floor they
cannot cross until `filterBy` lands (bd `aae-orc-j1xi`). Detail in
`docs/design/widening-notes.md` and bd `aae-orc-qijl`.

The scope-qualification item now has a concrete route to try rather than
an open question: `ServiceAccount.scopes`, tracked as bd `aae-orc-8af5`.
The study itself is bd `aae-orc-cw9y`.

Remaining: MCP transport (F3) and the composite-verb question (F4). The
scope-qualification study (`aae-orc-cw9y`) narrowed rather than closed:
the readable-scope route exists, so what is left is the per-operation
assignment, which needs a least-privilege credential.

### F2: Schema Introspection, get-by-id, and Server-Side Filters

`cargo xtask sync-spec` introspects the schema once credentials exist;
`check-ops` then validates the curated documents. This section was
posed as three open questions: which kinds get `stave get <kind> <id>`,
which list operations grow `filterBy` variables, and whether
`graphSearch` becomes a first-class verb. **All three were questions
about the vendor and none of them should have been.** The 2026-08-07
audit found singular root fields (`issue(id:)`, `cloudResource(id:)`
and the rest) and `graphSearch` with its own filters already present in
the schema. What is open is which to expose and how, not whether they
exist.

Evidence for the server-side filter, added 2026-08-06: while `search`
and `--since` filter client-side they are full-connection walks by
construction. `820a8b2` cut the request count for one such walk by
roughly twelve times; it did not remove the walk, and no page-size
tuning can. Only a server-side filter does. See finding-002.

**Premise corrected 2026-08-07.** This question was posed as whether
list operations should *grow* `filterBy` variables. They exist already:
`issuesV2` and `cloudResourcesV2` both accept `filterBy` and `orderBy`,
and `IssueFilters` carries 60 input fields including `status`,
`severity`, `createdAt`, `assignee`, and `hasServiceTicket`. The curated
documents declare only `$first` and `$after`, so the full-connection
walk is a property of our documents and not of the Wiz API. `get`-by-id
and `graphSearch` are likewise present as root fields rather than open
questions. What remains open is which filters to expose and how, not
whether they exist. See `docs/design/field-surface-audit.md` and bd
`aae-orc-j1xi`.

### F3: MCP Live Validation

The MCP client follows the streamable-HTTP shape verified against
another vendor's server (bloomctl B6/B8). Wiz's hosted server at
`https://mcp.app.wiz.io` needs live verification: auth handshake
(bearer from the same OAuth flow is the working assumption), tool
vocabulary (feeds the read-only heuristic), and the `mcp map`
crosswalk to curated operations.

### F4: Audit-Trail Mining → v0.2 Composite Verbs

Same question as the siblings: which composite verbs do Wiz workflows
want (`issue-triage`, `vuln-exposure`, `posture-report`)? Originally
deferred until real usage accumulates, the explicit purpose of shipping
the audit trail first.

No longer deferred, and the answer is narrower than the question was.
The 20-runbook elicitation was run against a sealed vendor-surface
control arm under a pre-registered scoring predicate, and the gate
returned Outcome 3, continue, on a narrower certified finding than the
exercise set out to test. Ruling, the six pre-registration defects it
exposed, and what unblocks in what order:
`docs/design/verb-comparison-gate.md`. The elicitation corpus is
`docs/runbooks/catalogue.md`; bd `aae-orc-e4jo` is the umbrella.

One correction the gate work produced that bears on this question
directly: glue that exists because our own documents are thin is
document debt, not verb demand. Sixteen of 106 paper glue stages
disappear once `rsh6`, `j1xi`, `qijl` and `gs23` land, ten of them in
class A alone, and the run harness makes that answer mandatory per
recorded stage rather than optional.

### F5: Distribution [largely resolved]

Signed distribution is live as of 2026-08-05: the 7 org secrets are
granted, `SIGNING_ENABLED=true`, and alpha run 31056265168 went
all-green through Sign & Notarize, provenance attestation, and the
tap update. Verified end to end: `brew install ArcavenAE/tap/stave`
installs a Developer ID-signed, notarized binary reporting its channel
tag in `--version`. Remaining: the stable channel (`v*` tag →
release.yml) is untested until the first stable release is cut.

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
| Field sweep, a second route to our own scopes, and two silent truncations | 2026-08-08 | **Two tickets closed, four filed, three findings.** The twelve-kind field sweep (`aae-orc-k5o7`) finished what finding-006 opened: ten of twelve curated kinds return every field their document selects, and the gap is confined to `audit_log` (4 of 8) and `cloud_account` (6 of 11). That bounded a caveat that had to be read as applying everywhere, which is why it was worth running ahead of the mechanism (`aae-orc-i8cj`, P1, still open on the mechanism). **finding-007** closed F1's last open item: the token's `encodedScopes` bitmask will never decode, but `ServiceAccount.scopes` answers the same question, and `--from-directory` on `auth scopes` / `can-i` / `plan --check` reads it. Live: 79 scopes readable, `missing: 0` against the twelve the registry declares, so those names are real. Opt-in rather than the silent fallback the ticket asked for, because `token_scopes` is pure and `auth scopes` has always been offline. **`SCOPE_METADATA_PROVISIONAL` stays true**: names are validated, the per-operation assignment is not, and a credential holding 79 scopes cannot tell a correct mapping from a generous one. **`aae-orc-x7iv`**: a redacted GraphQL connection now keeps `totalCount` and `hasNextPage` by type guard (only a number and a boolean pass), because a truncated nested connection was otherwise invisible. Confirmed against a real response rather than fixtures, and the first record it met was already truncated: 139 controls fetched at `first: 100`, 39 dropped silently, filed as `aae-orc-yet2`. Queue item 3 settled; queue item 4 at four of seven, three blocked on the two gap kinds. Also filed: `aae-orc-2gk8`, the run harness cannot execute any non-stream verb because `scrub.sh` fails closed without `_kind`, so every `auth` verb's coach gate is remembered rather than enforced. Two errors of mine, both caught and both recorded: an alarm that the run had measured pre-widening documents (disproved arithmetically) and an empty `build.rs` created where none existed. The skew check refused to start a run because `stave` on PATH was the brew binary, which is exactly the trap it was built for. |
| Instrument-building, and four controls that were wrong | 2026-08-07 | The verb-mining apparatus shipped and then spent the day failing usefully. Built: the build-time `DENIED_SELECTIONS` guard (`exportUrl` on any type, `ReportRun.url`, `ServiceAccount.clientSecret`); nine widened curated kinds (`aae-orc-qijl`, `x1rg` closed) with ten deprecated fields replaced by the successors the schema's own text names; the field-surface audit, which found **zero** runbook steps blocked by the Wiz API and corrected F2's premise (`filterBy` exists and is unused); the run harness (`scripts/runlog.sh`, coach-gated, scrubbed by construction); four synthetic external join fixtures with their answers measured; and nine independent runbook judges with a default-deny packet allowlist. Four controls turned out to range over the wrong thing, each found by probing rather than reading, and each is a finding rather than a commit message. **finding-004**: the harness's scrub exemption keyed on `--in`, a flag the coach never sees, so a bypass existed with no bypass flag; second instance the same day in the judge packet, which keeps `command` and cannot police what an executor writes inside a predicate. **The version binding**: the harness ran the brew binary while recording the tree's commit, so a whole run measured the wrong document set. **The scrubber**: a populated object and a JSON null rendered identically, which blinded every item of a queue that asks whether fields are populated. **finding-005**: check 4 was rewritten under operator direction and the running coach never saw it, because an agent-file edit does not reach a coach spawned after it. First live validation banked queue item 1 (the fourteen-member `VulnerableAssetBase` fragment is accepted) before parking for a gate reload. |
| Live demo, hygiene, safety gate, paging fixes | 2026-08-06 | First live tenant work. 12/12 curated kinds validated read-only (`aae-orc-hzg0`). A tenant-data leak into a transcript produced the durable scrubber (`scripts/scrub.sh` with a default-deny field allowlist, shared pattern source with the detector, fail-closed on unrecognised input shapes) plus the `tenant-leak-scan` skill and a second trigger in the hygiene rule; lefthook installed and verified by planting an OCID. Added the `stave-safety-coach` subagent (Read/Grep/Glob only, CLEAR or HALT, uncertainty resolves to HALT) and its behavior-trigger rule, after confirming Wiz puts effectful operations under `type Query`. A BMAD casting call produced 20 operator runbooks (`docs/runbooks/catalogue.md`) for the F4 verb-mining exercise, tracked as bd `aae-orc-e4jo`; a pre-run review reframed that exercise around a sealed control arm and demoted execution to commissioning. Two paging defects fixed (`820a8b2`): zero-node pages ended reads, and page size was derived from `--limit` so filtering verbs walked whole connections at the limit. F1 scope-qualified, F2 gains evidence. See finding-002. |
| Scaffold | 2026-08-05 | Repo created at ArcavenAE/stave by porting bloomctl wholesale and re-instantiating the vendor-specific 30% for the Wiz GraphQL API. Shipped: curated operation library (12 list operations, provisional field selections), SDK (OAuth client-credentials mint + XDG token cache + dc-claim endpoint derivation, client-id/secret/endpoint/registry chains, keychain custody, write-guard on mutations incl. parsed ad-hoc documents, audit v2 with api_url_source + graphql_error outcome, Wiz kind table ×12, CEL camelCase timestamp promotion), CLI (auth/registry/config/ops/api + 6 primitives + mcp family), xtask (sync-spec introspection + check-ops schema validation), synthetic Wiz fixtures + asserts + recipes, tenant-data-hygiene rule adapted to cloud-posture data, CI/signing workflows renamed. B1–B5 set, F1–F5 opened, G1–G2 ruled. |
