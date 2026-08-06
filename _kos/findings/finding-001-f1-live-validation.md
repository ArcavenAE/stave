# finding-001: F1 live validation against the production tenant

Date: 2026-08-06. Scope: charter F1. Method: read-only passive only —
schema introspection, one `list project --limit 1`, and offline JWT
inspection. No active, change, remediation, or destructive action was
attempted against the tenant.

## What was confirmed

- **Auth flow works end to end.** The service account mints a token via
  `grant_type=client_credentials`, `audience=wiz-api`, against
  `https://auth.app.wiz.io/oauth/token`. The `TOKEN_AUDIENCE` and
  default token URL were correct guesses.
- **Endpoint derivation works.** The minted token carries the `dc`
  claim; `api_url_from_dc` reconstructs the tenant endpoint. Confirmed
  present in the real token.
- **Schema introspection works.** `cargo xtask sync-spec` vendored
  10,060 types (2.7 MB SDL) with a sha256 pin. The schema contains no
  tenant-identifying literals (leak-scanned + grepped).
- **The read pipeline works.** `list project --limit 1` returned a
  well-shaped record (token mint→cache→endpoint→POST→relay
  connection→kind extraction→JSONL). `list_projects` and the project
  kind table are validated.
- **11 of 12 curated documents validated as written.**

## What was corrected (provisional guesses that were wrong)

- **`CloudResource` has no `cloudPlatform` field.** Real fields:
  `nativeType`, `subscriptionName`, `subscriptionExternalId`.
  `list_cloud_resources.graphql` fixed.
- **`CloudAccount.status` is deprecated** ("refer to Deployments and
  System Health Issues instead") with no drop-in scalar. Replaced with
  `lastScannedAt` + `resourceCount`. `list_cloud_accounts.graphql`
  fixed.
- **sync-spec emitted invalid SDL for empty types.** Wiz's schema has
  empty input objects; introspection rendered them as `input Foo {}`,
  which is a parse error (a fields block needs at least one member).
  Fixed the SDL emitter to render bodyless (`input Foo`) when empty,
  per the GraphQL grammar. Without this, `check-ops` could not parse
  the schema at all.

## The load-bearing finding: scopes are an opaque bitmask

The design assumed the token would carry granted scopes as readable
strings (`scope` / `scp` / `permissions`). It does not. The Wiz
service-account token carries **`encodedScopes`** — a base64 bitmask
against an internal ordering stave does not have. There is no readable
current-identity scope query in the schema (`serviceAccountScopes`
appears only on a mutation *payload*, i.e. the echo when an SA is
created).

Consequence for the D5 verbs:

- `auth scopes` reports the scopes are opaque and non-enumerable — it
  does NOT claim "no scopes."
- `auth can-i` and `auth plan --check` **refuse to answer** rather than
  emit a false verdict. This is the paramount honesty property: with an
  opaque bitmask, a naive reader would have reported every scope as
  "missing" and every operation as "not allowed," which is worse than
  useless — it is confidently wrong.
- `ops permissions` and `auth plan` (GRANT / DO NOT GRANT provisioning
  checklist) are unaffected — they are static registry metadata and do
  not need the token. This is the higher-value half of the feature, and
  it stands.

`token_scopes` now returns `Readable | Opaque | Absent`; the CLI handles
`Opaque` honestly on every scope-dependent path. Tests cover all three.

## Still open (design decision for the operator)

Enumerating/checking granted scopes for a Wiz tenant would require
either Wiz's `encodedScopes` bit-ordering table (not published) or a
readable-scopes API query (none found for the current identity). Until
one exists, client-side `can-i`/`--check` cannot function against Wiz
tokens. Options, none urgent: (a) leave them honest-refusing as now;
(b) drop them for Wiz and lean entirely on `auth plan` provisioning;
(c) revisit if Wiz exposes a readable-scopes query. Recorded, not
decided.

## Provisional markers

Scope *names* in the registry are now schema-consistent (the documents
validate) but the *grant vocabulary* a tenant actually issues is still
only corroborated from integration-vendor docs, not enumerable from the
token. `SCOPE_METADATA_PROVISIONAL` stays `true` until the grant
vocabulary is confirmed by another route.
