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

## Scope-qualification did not manifest as expected — needs more study

With the service account used during development, the scope-reading
verbs (`auth scopes`, `auth can-i`, `auth plan --check`) did not
function as designed. The token did not expose granted scopes as the
readable claim the design assumed; the observed carrier was
`encodedScopes`, which did not decode to a readable scope list through
the paths tried. The root cause and the correct remedy are not settled
and require more study (against this and other service accounts).

Interim behavior (ratified: keep as-is): the verbs **refuse to answer**
rather than emit a verdict they cannot support. `auth scopes` reports
the scopes as non-enumerable; `auth can-i` and `auth plan --check` exit
nonzero with "cannot determine" instead of a possibly-false
"not allowed." `ops permissions` and `auth plan` (the GRANT / DO NOT
GRANT provisioning checklist) are unaffected — they are static registry
metadata and do not read the token.

`token_scopes` returns `Readable | Opaque | Absent`; the CLI handles the
non-readable case honestly on every scope-dependent path. Tests cover
all three.

## Still open

Whether scope qualification can work against Wiz at all is unresolved
and needs more study — the encodedScopes format, whether a readable
scope source exists elsewhere, and how the grant vocabulary is actually
issued. Decision on the verbs: option (a), left honest-refusing.

## Provisional markers

Scope *names* in the registry are now schema-consistent (the documents
validate) but the *grant vocabulary* a tenant actually issues is still
only corroborated from integration-vendor docs, not enumerable from the
token. `SCOPE_METADATA_PROVISIONAL` stays `true` until the grant
vocabulary is confirmed by another route.
