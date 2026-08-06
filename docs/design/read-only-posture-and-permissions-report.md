# Read-Only Posture and the Permissions Report

Design brief. Drafted 2026-08-05 from a party-mode design review (architect,
test architect, platform engineer, systems/safety reviewers, two LLM-behavior
reviewers), revised the same day after an operator review that verified the
code citations and ruled on the open options.

**Status of this brief:** the decisions below are agreed. They are
**design-ready**, not all **build-ready** — several depend on facts that F1
(live validation) must confirm, or on a contract (session identity) that must
be specified first. The split is called out per item and summarized in
"Decided vs. buildable" at the end. Nothing here is implemented yet.

## Context and constraints

- stave's only tenant is in-use production. There is no development instance.
- Therefore any client-side write/delete gate is untestable end to end: to
  prove a delete block works, a delete would have to be possible somewhere.
  We err on the side of "not tested" rather than validating gates against
  the live tenant.
- This is not a security feature. The server (Wiz scope enforcement) owns
  security. This design reduces the risk of an experimenting, high-temperature
  LLM caller performing impromptu tests against production, and it makes the
  one server-enforced barrier (the service account's scopes) easy to provision
  correctly. It is an operational-safety feature.
- Research grounding: leading models are measurably compliant with harmful
  agentic requests at baseline (AgentHarm, arXiv 2410.09024); refusal
  robustness varies unpredictably with context length (arXiv 2512.02445);
  prompt-level rules do not prevent tool calls, so enforcement must sit
  between reasoning and execution (the SDK, not the prompt). The narrower
  claim that a refusal *string* steers later behavior is NOT established by
  the long-context paper; byte-stable refusals (D2) are engineering judgment
  informed by reformulation results elsewhere (AgentDojo), not a finding of
  that paper.

## The four-layer model (agreed)

Ordered by how much each layer can actually be trusted:

1. **Credential scopes** (server-enforced, vendor-tested). The service
   account is the sandbox. The only layer we can rely on.
2. **Provisioning report** (`stave auth plan`). Makes layer 1 correct at
   creation time. The primary operational-safety feature of this design.
3. **Unconditional client-side refusal of mutations**. Friction, honestly
   labeled as untested. Not a fence.
4. **Audit of refusals**. Detection. Repeated, reformulated refusals within
   one operator session are the signature of an experimenting agent
   (requires the session-identity contract in D6).

## Decisions (agreed in review)

### D1. No write/delete override surface in v0

Mutations (and subscriptions) are refused unconditionally. No
`--allow-write`, no `--allow-delete`, no env override. Rationale: a gate that
can never be commissioned against a live mutation is scenery; an override
surface invites the caller to find it; capability absence beats capability
gating. The tiered override design is deferred, not rejected. Reopening
condition: a development tenant exists to commission the gate against.

### D2. Refusal text is terminal and byte-stable

The refusal message names the posture and stops: read-only against live
tenants, not configurable in this session. It never names a flag, env var,
config key, or doc section that would lift it. It is byte-identical on every
firing. Correlation identity for operators lives in the audit line, never in
the message.

Basis: terminal refusals follow from D1 (there is nothing to point at).
Byte-stability is a conservative engineering choice against a persistent
caller mining message variation for signal; it is not attributed to a
specific paper.

This splits the repair-error rule in `cli-philosophy.md` into two cases:
resolution-chain errors get maps (name every layer and the fix); guard
refusals get walls (terminal, no breadcrumbs). The rule file needs this
amendment.

### D3. Required scopes are mandatory registry metadata

Every curated operation declares `required_scopes` in the `stave-api`
registry. `cargo xtask check-ops` fails when an operation omits them: no verb
can be added without declaring its permission cost. Scope names follow the
vendor's `verb:resource` grammar (`read:issues`, `create:reports`,
`admin:audit`), corroborated across integration-vendor docs.

**Provisional until F1:** the canonical scope list, the exact names, and the
scope-claim format all sit behind tenant-authenticated docs. The plumbing may
be built now, but `auth plan` and `can-i` must not present their output as
authoritative until F1 validates scope names, the JWT scope-claim format, and
any scope-implication rules (e.g. whether `read:all` subsumes the enumerated
set). Until then their output carries a provisional marker.

### D4. Effects metadata, and the tier as a conservative join

The scope prefix is the vendor's authorization claim. It is not a consequence
model. The registry therefore also carries an authored effects block per
operation. **The tier describes mutation consequences only** — it is not a
general sensitivity rating (see D4a for reads).

- `reversibility`: reversible | irreversible | unknown
- `side_effects`: none | notifies | triggers-integrations | unknown
- `egress`: none | produces-egress-artifact | unknown

The mutation tier is computed as the stricter of (scope-prefix tier,
authored-effects tier); `unknown` or missing resolves to the strictest tier.
Omission fails safe.

Worked example that settled the design: `create:reports` is reversible and
low side-effect but high egress; a Wiz report mints a pre-signed download URL
that packages tenant posture into a portable artifact. Deleting the report
does not un-mint the URL. Reversibility alone under-scores it; the prefix
alone mis-describes why it is dangerous. Both axes are needed. Do not
re-litigate the one-axis model.

In v0 this is schema and vocabulary only (all mutations refuse regardless);
it is bought now so the day mutations arrive does not start with a taxonomy
argument.

### D4a. Read-side sensitivity and cost metadata

The effects model above is mutation-shaped, but v0 performs only reads, and
reads carry their own consequences: identity data (`users`), security posture
(`issues`, `vulnerabilities`), and query cost (broad `cloudResources`). Each
curated operation therefore also declares:

- `sensitivity`: normal | identity | posture | unknown  (what class of data
  the read returns; feeds tenant-hygiene and the `auth plan` summary)
- `cost_hint`: light | heavy | unknown  (advisory; informs future depth/size
  limits, not a gate)

`sensitivity` is distinct from the mutation tier and is never used to refuse
a read — the credential's scopes decide that. It exists so the tool can
describe what an operation exposes, not to block it.

### D5. Four permission verbs

All static or JWT-local; all testable with zero tenant contact. Output
follows the stream contract (JSONL non-TTY, table on TTY, `--output md`).
Scope-dependent output (verbs 2-4) carries the D3 provisional marker until F1.

1. `stave ops permissions`: the static report. Per operation: required
   scopes, computed mutation tier, effects, read sensitivity, cost hint.
   Offline; pure registry metadata.
2. `stave auth scopes`: scopes the current token actually carries, decoded
   from the JWT claim (same decoder as the `dc` claim; no extra permission,
   no API call).
3. `stave auth can-i <op>`: required subset-of granted, instant, local.
   Exit 0 yes / 1 no. The kubectl borrow.
4. `stave auth plan [--ops ...]`: the provisioning artifact. Sections:
   - GRANT: the least-privilege union of scopes for the selected (default:
     all curated) verbs, as a checklist for the Wiz portal service-account
     dialog. Discourage `read:all` in favor of the enumerated set.
   - DO NOT GRANT: scopes that appear in the registry only for future or
     write-tier verbs, each with one line of why withholding it is the real
     delete-block.
   - `--check`: compares granted (JWT) vs required (registry). Exits nonzero
     on any drift, and **distinguishes the two drift directions** in the
     report: MISSING scopes (credential is unusable for some enabled verb)
     versus EXCESS scopes (credential is over-privileged beyond what any
     enabled verb needs), or both. CI-able.

### D6. Refusals are first-class audit outcomes

Guard refusals emit an audit line carrying the operation, classification, and
tier. Three concrete corrections from the code review, which raise this from
"S" to "M":

- **Field name:** the audit contract's outcome field is `result`, not
  `outcome`. The refused value is `result: "refused"`
  (`docs/audit-trail-format.md`, `crates/stave-sdk/src/audit.rs`).
- **Schema bump:** adding a new `result` value is a schema change; bump
  `schema_version` (currently 2) per the audit contract, and update the
  format doc.
- **Ordering:** refusals currently return before the audit span is
  constructed (`crates/stave-sdk/src/client.rs`, the WriteGuard early
  return). Emitting a refused line requires building (a minimal) span on the
  refusal path. This is a control-flow change, not a field addition.

**Session identity (new contract, gates the per-session detector).** The
detector D6 exists for — "count refusals per operator session" — is not
buildable on today's audit trace. `CallOptions.trace_id` groups one logical
invocation or pagination run and otherwise resets to a fresh UUIDv7
(`crates/stave-sdk/src/client.rs`); a run of reformulated attacks is N
distinct trace_ids, not one session. A durable `session_id`, supplied by the
invoking agent environment (env var, threaded into the audit span alongside
trace_id), is required. Specify this contract before coding the detector;
the refused-audit-line itself (above) ships independently of it.

### D7. Two-account doctrine

When a write verb is eventually needed, the answer is a second, separately
named, write-scoped service account, provisioned then and stored apart. The
default credential in every shell and agent session stays read-only. Never
widen the read account. `auth plan` prints this doctrine in its DO NOT GRANT
section. (Pattern precedent: the two-service-account CI arrangement from
kubernetes#95449.)

### D8. Honest labeling

SECURITY.md states: the client-side mutation refusal exists but has never
been commissioned against a live mutation; do not lean on it; lean on scopes.
Docs and help text must not describe the guard as a security control.

### D9. Advertised surface stays read-only

`ops list` shows only read operations (v0.1 curates nothing else; keep that
deliberate). If write verbs later enter the registry for planning purposes
(so `auth plan` can name their scopes), they are registry-visible but not
listed by default. The tool's visible surface teaches the caller what kind of
tool it is.

### D10. Remove `allow_writes` from the typed configuration model (O1(a))

Ruling: remove, do not merely ignore. Delete the flag, environment variable,
config command/key, status reporting, and SDK opt-in field. The compat
argument for keeping it is void: the config struct is `#[serde(default)]`
(`crates/stave-sdk/src/auth.rs`), so a stale `allow_writes` key already parses
harmlessly with no typed field to receive it. A parsed-but-inert switch is
misleading configuration and an inviting reconnection point for a future
maintainer.

A stale key may draw an "obsolete and ignored" migration warning, but it is
not part of the typed model. If write support ever returns, a newly designed
posture is preferable to reviving a mechanism whose semantics were
deliberately repudiated.

### D11. Curated-only default with a persistent exploratory-read posture (O2(b))

Ruling: default to a named curated posture; permit ad-hoc read documents only
under an explicit, persistent exploratory-read posture the operator
deliberately enters. Rationale: ad-hoc reads do not threaten mutation safety
under a correctly scoped credential, but they bypass the new safety structure
— no registry effects/sensitivity metadata, scope needs that `auth plan`
cannot enumerate, field selections omitted from curated documents on purpose,
and unbounded query cost. The value of the posture is that arbitrary query
construction happens only after the operator has opened that door on purpose,
not opportunistically mid-workflow.

Constraints on the exploratory posture:

- It is persistent (a posture, not a per-call flag). **No per-call override**
  — a per-call flag would be exactly the breadcrumb D2 forbids.
- Mutation/subscription refusal stays unconditional in BOTH postures.
- Audit records the active posture and a document hash.
- Consider query depth/complexity and response-size limits (advisory,
  informed by D4a `cost_hint`).
- Ad-hoc documents remain syntax-parsed and classified but are still not
  locally schema-validated until the vendored schema exists
  (`crates/stave-sdk/src/ops.rs`, F2); the server is the validator until then.
- This is not a security boundary — an agent that can edit config could
  change the posture. Its value is behavioral, not enforcing.

Explicitly separate concern: curated-only does NOT solve cursor/variable
leakage into audit lines (`docs/audit-trail-format.md` already warns that
variables and cursors carry tenant data). That is a distinct audit-privacy
problem, tracked separately, not laundered through this posture.

## Provisional pending F1 (live validation)

- The JWT scope claim's exact field name (scopes-in-token corroborated by
  integration vendors via RegScale's `wizScope`; field name unverified).
- The canonical scope vocabulary and exact names (official docs are behind
  tenant auth; current list is assembled from integration-vendor docs).
- Scope-implication rules (e.g. whether a `read:all` bundle subsumes the
  enumerated scopes for `can-i`/`--check` math).
- Whether `delete:`-prefixed scopes exist in the vendor grammar.

Until F1: build the plumbing, but `auth plan` / `auth scopes` / `can-i` mark
their output provisional and are not presented as authoritative.

## Decided vs. buildable

- **Build now, tenant-free:** D3 registry `required_scopes` + D4/D4a effects
  and sensitivity schema + check-ops gate; D10 removal of `allow_writes`; D2
  refusal text + cli-philosophy amendment; D8 SECURITY.md labeling; D9
  surface rule; `ops permissions` (static). All unit/wiremock testable.
- **Build now, mark provisional until F1:** `auth scopes`, `auth can-i`,
  `auth plan` (+`--check` with missing/excess split). Logic is static
  metadata + the existing JWT decoder; correctness of scope names is what F1
  confirms.
- **Specify first, then build:** D6 — the refused audit line (field `result`,
  schema bump, refuse-before-span reorder) can ship on its own; the
  per-session detector needs the `session_id` contract defined before coding.
- **Deferred with reopening condition:** the tiered write/delete override
  surface (needs a dev tenant); session-level refusal escalation (needs audit
  evidence first).

## Effort sizing (test architect, revised)

Small to medium overall. D3 + check-ops: S. D4/D4a schema: S (net-new). Four
verbs: M (presentation over static metadata + JWT decoder). D10 removal: S.
D11 posture: S-M (new posture state + audit fields). D6: **M** (revised up
from S — schema bump + control-flow reorder + a new session-identity
contract, not a field addition). D2/D8 prose + rule amendment: S. Everything
is unit/wiremock testable; nothing requires tenant contact.

## Sources

- AgentHarm: arXiv 2410.09024
- Refusal instability in long-context agents: arXiv 2512.02445 (supports
  context-dependent instability; does NOT support refusal-text-as-steering)
- Magentic-UI reversibility heuristics: arXiv 2507.22358
- Approval fatigue / oversight capacity: arXiv 2606.08919
- Guardrail placement (enforcement between reasoning and execution):
  Towards Data Science, "How to Build Guardrails for Effective Agents"
- Wiz scope vocabulary (integration-vendor docs): Illumio, Port, Phoenix
  Security, Elastic, RegScale integration guides
- Two-account dry-run pattern: kubernetes/kubernetes#95449
- `kubectl auth can-i` as pre-flight convention: Kubernetes docs and field
  guides

## Cross-references

- charter.md B4 (write-guard as shipped today), F1 (live validation), F2
  (schema introspection)
- `.claude/rules/cli-philosophy.md` (repair-error rule; D2 amends it)
- `.claude/rules/tenant-data-hygiene.md` (pre-signed report URLs as secrets,
  the D4 egress axis; cursor/variable leakage, the D11 separate concern)
- `docs/audit-trail-format.md` (D6 adds the `result: refused` value and bumps
  schema_version; D11 adds posture + document-hash fields)
