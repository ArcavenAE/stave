# stave Charter

> Re-introduction document for stave — Rust CLI for the iru (Kandji)
> Endpoint Management API. Restores context for a collaborator who was
> present but does not persist. Follows the kos process:
> Orient → Ideate → Question → Probe → Harvest → Promote.

Last updated: 2026-07-15 (scaffold session, same-day follow-up — live
validation PASSED after the user granted token permissions (F1 → B8);
repo public; signing enabled (release environment + 7 org secrets +
SIGNING_ENABLED=true))

---

## The Problem Statement

Agents working on Apple-fleet operations need programmatic access to
iru's Endpoint Management API. The vendor publishes an OpenAPI spec at
`https://docs.iru.com/openapi/iru-endpoint-openapi.json` (94 paths, 121
operations covering devices, blueprints, library items, users, tags,
audit events, threat + behavioral detections, vulnerability management,
prism reports, ADE integrations) and an MCP server exposing the same
surface as 131 tools. There is a first-party MCP story but no
first-party CLI — agents either write ad-hoc curl wrappers per session
or hand the whole conversation to MCP without a durable usage record.

A CLI alone is not enough. To eventually curate *composite verbs* —
the workflows iru operators actually run (fleet inventory, staleness
sweeps, vulnerability triage, enrollment audits) — we need a durable
record of how the API is used in practice. That record has to be
structured enough for a future LLM session to mine for patterns, and it
has to capture every call: REST and MCP alike.

One more constraint sidestep didn't have: **the only tenant we can
develop against is in-use production.** Safety posture is a design
value, not an afterthought.

---

## Design Values

1. **Spec is the contract.** The vendored OpenAPI spec is the canonical
   surface. The generated `stave-api` crate is its faithful Rust
   projection. The CLI exposes the spec via `stave api <operationId>`
   and layers primitives on top. Regen is one command; keeping up with
   the vendor is cheap by construction.
2. **SDK-first.** All shared logic — auth chains, audit emission,
   redaction, the write-guard, the MCP client — lives in
   `stave-sdk`. The CLI is presentation; future MCP surfaces are
   sibling consumers.
3. **Audit trail is a feature, not a log.** Every REST call and MCP
   call writes a structured JSONL line locally. The format is designed
   for future LLM analysis to propose composite verbs. See
   `docs/audit-trail-format.md`.
4. **Production-safe by default.** Mutating operations refuse without
   explicit opt-in (`--allow-write` / env / config). The guard applies
   uniformly to REST (non-GET) and MCP (non-`get-*`/`list-*` tools).
5. **Agent-first ergonomics.** JSONL for non-TTY, predictable verb
   shape, stable exit codes, chain-naming errors.
6. **User sovereignty.** Local-first audit trail, local config, no
   phone-home, no telemetry. Aligns with the orc platform's SOUL §1.

---

## Non-Goals

- **Not a curl replacement.** No `raw` HTTP escape hatch (inherited
  ruling — sidestep G1). The spec is the contract; `stave api
  <operationId>` is the spec-aware escape hatch.
- **Not a multi-tenant service.** stave is a local CLI.
  Authentication is per-user, per-tenant.
- **Not an MDM console.** Destructive device actions (erase, lock,
  passcode ops) stay behind the write-guard; stave is not trying to
  make them frictionless.

---

## Bedrock

*Established. Evidence-based or decided with rationale.*

### B1: The Sidestep Pattern Transfers

stave was scaffolded by porting sidestep's entire workspace
(4 crates + xtask + tests + CI + rules) and re-instantiating the
vendor-specific ~30%: spec module, auth chains, kind table, enrich
recipes, fixtures. The vendor-agnostic ~70% — stream contract, CEL
adapter, audit machinery, redaction, keyring handling, wiremock
harness, distribution workflows — transferred without redesign. One
session took stave from zero to 147 green tests (sidestep parity:
127) at all gates.

Evidence: this scaffold session; orc finding (stave-scaffold) for
the pattern-vs-implementation split. Bears on orc F16 (assumption
provenance) and orc F17 (lifecycle infrastructure).

### B2: OpenAPI Codegen via progenitor, iru Pre-Passes

`stave-api` is regenerated from `spec/iru-endpoint-openapi.json`
via `cargo xtask regen` (progenitor 0.14). The iru spec needs three
pre-passes, two inherited and one new:

1. `fill_missing_operation_ids` — **all 121** operations lack
   `operationId` (StepSecurity: 78/93). Synthesized as
   `{method}_{path}` with the constant `/api/vN` prefix stripped, so
   ids read `get_devices`, not `get_api_v1_devices`. Zero collisions.
   Applied at `sync-spec` time so the vendored spec is self-contained.
2. `strip_multipart_request_bodies` — **new for iru.** progenitor
   rejects `multipart/form-data` outright; iru's 3 library-upload
   endpoints use it. The pre-pass drops those request bodies from the
   in-memory model only; the vendored spec keeps them, and the SDK's
   spec-driven dispatch still sees `has_body`. Real multipart upload
   is a curated-verb question for later.
3. `collapse_multi_error_responses` — 226 collapsed (progenitor
   allows one bodied response per class). Multi-success: 0, unlike
   StepSecurity.

Output: ~500KB generated client, 121 operations. Committed at the
crate level.

### B3: Vendored Spec, Live Source

Spec vendored under `spec/` with a `.sha256` pin. `cargo xtask
sync-spec` fetches from `https://docs.iru.com/openapi/iru-endpoint-openapi.json`,
sanitizes secret-shaped examples, normalizes operationIds, and updates
both files. `diff-spec` remains a stub (inherited follow-on).

### B4: Subdomain Chain — Third val-resolution-chain Instantiation

iru's server URL is templated: `https://{subdomain}.api.kandji.io`
(US) / `.api.eu.kandji.io` (EU). The tenant subdomain is constant for
the life of a token — the abusive-argument test
(`.claude/rules/cli-philosophy.md`) applies, but unlike sidestep's
owner/customer it is a *client-construction* concern, not a path
param. Resolution: `--subdomain` flag → `STAVE_SUBDOMAIN` →
`[default] subdomain` in config → chain-naming error. Region (`us` /
`eu`) rides the same shape. The audit trail records the provenance as
`path_params_source._subdomain` (reserved name), keeping the mining
signal uniform with sidestep's.

Token chain (env → keyring → config) inherited verbatim from
sidestep B5.

### B5: Write-Guard — Read-Only Against Production by Default

Any non-GET operation errors with a repair-friendly `WriteGuard`
message unless allowed by: `--allow-write` (per call) →
`STAVE_ALLOW_WRITE` env → `[default] allow_writes = true` config.
The same posture gates MCP `tools/call` for any tool not named
`get-*`/`list-*`. Enforced in `Client::call_op` before any request is
built (wiremock `expect(0)` test pins this), and reported loudly by
`auth status`.

Rationale: the only live tenant is in-use production; the vendor's own
MCP docs recommend explicit approval for destructive operations.

### B6: MCP Client Surface

iru publishes an MCP server per tenant
(`https://<subdomain>.connect.iru.com/mcp-server/connector/kandji/tools`,
streamable HTTP, `X-API-Key` `sk_live:` + `X-MCP-Profile` headers).
`stave mcp` operates it: `login` (key → keyring, profile/url →
config), `status`, `config [--reveal]` (client-config JSON), `tools`
(JSONL, `_kind: mcp_tool`), `call` (write-gated), `map` (tool-name ↔
operationId crosswalk; unmatched tools emit `operation_id: null` so
the gap itself is data). MCP calls emit audit lines with `verb_phase:
"mcp"`, `mcp_tool`, args shape hash, and outcome.

Verified live 2026-07-15: server `kandji-mcp` v3.4.4, 131 tools,
SSE-framed JSON-RPC responses. Tool *calls* were permission-blocked
(see F1) but transport, handshake, and tools/list are confirmed real.

### B8: Live-Validated Against the Production Tenant

Same-day follow-up to the scaffold, after the user granted the token
read permissions. Auth mechanism first verified by differential probe
(garbage token → 401, no header → 401, real token → 403), confirming
the token IS the bearer — no exchange step exists. Then the smoke:

- `api get_settings_licensing` — real payload (counts/limits).
- `list` live for device, blueprint, user, tag, audit_event,
  vulnerability, custom_app, custom_profile, custom_script,
  ade_device. **Every kind-table field guess confirmed** — device_id/
  device_name/last_check_in/platform/blueprint_id, blueprint id+name+
  enrollment_code+computers_count, audit_event id/action/occurred_at,
  vulnerability cve_id/severity/first_detection_date, library-item
  id/name. threat + behavioral_detection 403 (permission not granted
  or SKU-gated) — the only unverified pair (F2).
- Live pipeline: `list device | filter --where 'platform == "Mac" &&
  os_version.startsWith("15.")' | emit --format md` — CEL promotion
  and the stream contract work on real payloads.
- MCP end-to-end: `mcp status`, `mcp tools --filter`, `mcp call
  get-settings-licensing` (connector `outcome: success`), write-guard
  refuses `create-tag` without opt-in, `mcp map` links 35/131 tools to
  operationIds (the 96 unmatched are curated names — F3 mining data).
- Audit trail flowing: 37 JSONL lines day one at `~/.stave/audit/`
  (macOS falls back to `~/.stave` — `dirs::state_dir()` is None on
  macOS; XDG state path applies on Linux).

Evidence: bd aae-orc-b6dg.11 (closed); orc finding-077 addendum.

### B7: v0.1 Primitive Layer (Ported)

Six primitives (`list`, `get`, `search`, `filter`, `enrich`, `emit`) +
`api` escape hatch, over a `_kind`-tagged JSONL stream. 12 kinds:
device, blueprint, user, tag, audit_event, threat,
behavioral_detection, vulnerability, custom_app, custom_profile,
custom_script, ade_device. CEL predicates via the canonical adapter,
with iru-specific timestamp promotion (`*_at`, `*_date`,
`last_check_in`, `ts`). Three recipes re-instantiate sidestep's three
recipe *shapes*: `blueprint-context` (join), `severity-roll-up`
(roll-up), `device-platform` (hoist). Audit schema_version 2
throughout, plus the additive `invocation.subdomain_source` and the
MCP verb fields.

Kind-table field metadata (id/timestamp/search fields) is
**provisional until live-validated** — see F1.

---

## Frontier

*Actively open. Expected to resolve through design work or probes.*

### F1: Live Validation [RESOLVED → B8]

User granted read permissions same day; full smoke passed. See B8.

### F2: Response-Shape Knowledge Gap [largely resolved]

Field metadata confirmed against live payloads for 10 of 12 kinds
(see B8) — no kind-table changes were needed. Remaining: `threat`
and `behavioral_detection` field guesses are unverified (their
endpoints 403 — the threat/EDR permission wasn't in the grant, or
the tenant SKU lacks it). Re-verify when that permission lands.

### F3: Audit-Trail Mining → v0.2 Composite Verbs

Same question as sidestep F3, plus the MCP dimension: the trail now
records REST calls and MCP calls in one stream. Which composite verbs
do iru workflows want (`fleet-health`, `enroll-audit`,
`vuln-exposure`)? Deferred until real usage accumulates — the
explicit purpose of shipping the audit trail first. The `mcp map`
crosswalk also feeds this: tools with no REST twin mark surface the
spec doesn't cover.

### F4: Uploads

The three multipart upload endpoints (custom apps, in-house apps) are
stripped from the generated client (B2) and not invocable via
`stave api` (JSON body only). If uploads matter, they arrive as a
curated verb wrapping the documented S3 upload flow. No demand signal
yet.

### F5: Distribution

CI workflows (ci/alpha/release, harden-runner, gated signing) ported
from sidestep and renamed. Signing is gated on `vars.SIGNING_ENABLED`
— enabling requires adding stave to the org-level signing/notary/
tap secret allowlists (same 7 secrets sidestep uses). First alpha
publishes the `Formula/stave.rb` to `ArcavenAE/homebrew-tap`.
bd: aae-orc-b6dg.8.

### F6: Prism as a Kind Family

The 16 `get_prism_*` report endpoints (apps, filevault, certificates,
kernel extensions, …) share a uniform query shape (blueprint_ids,
device_families, filter, sort_by, limit/offset). They'd fit a
parameterized kind (`stave list prism --category apps`) better
than 16 separate kinds. Deferred to v0.2 with F3 evidence; today they
are reachable via `stave api get_prism_<category>`.

---

## Graveyard

*Ruled out. Kept for the reasoning.*

### G1: `raw` HTTP Escape Hatch (inherited)

Same ruling as sidestep G1: the spec is the canonical surface; `raw`
would let drift hide. Update the spec and regenerate instead.

### G2: The Name "bloom"

`bloom` collides with ros-infrastructure/bloom (major release-automation
CLI), DCSO/bloom (Go bloom-filter CLI), safety-research/bloom, and the
bloom-filter namespace generally; `bloom-cli` is an existing npm
package. `stave` had zero collisions in search at scaffold time.

---

## Session Log

| Session | Date | Outcomes |
|---------|------|----------|
| Scaffold | 2026-07-15 | Repo created at ArcavenAE/stave by porting sidestep wholesale and re-instantiating the vendor-specific 30%. Shipped: vendored spec (121 ops, all operationIds synthesized, `/api/vN` prefix stripped), progenitor codegen with new multipart pre-pass, SDK (token + subdomain + region + MCP credential chains, write-guard, audit v2 + subdomain_source + MCP fields, iru kind table ×12, 3 recipes, CEL `*_date`/`last_check_in` promotion), CLI (auth/config/ops/api + 6 primitives + full `mcp` subcommand family), iru fixtures + 3 jq asserts + 3 recipes, 147 tests green, clippy/deny/nightly-fmt clean. Live MCP verified (kandji-mcp 3.4.4, 131 tools); all data calls 403 — token has no API permissions (F1, bd aae-orc-b6dg.11). B1–B7 set, F1–F6 opened, G1–G2 ruled. orc bd: aae-orc-b6dg epic. |
