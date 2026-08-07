# Binding `cloudResourcesV2`

bd `aae-orc-rsh6`, offline half. Written 2026-08-07 against the vendored
schema `spec/wiz-schema.graphql`. No tenant contact, no stave
invocation. Specification: `docs/design/field-surface-audit.md` class 2.

## What shipped

- `crates/stave-api/ops/list_cloud_resources_v2.graphql`, a paginated
  query over `cloudResourcesV2`.
- Registry entry `list_cloud_resources_v2` (root field
  `cloudResourcesV2`, `read:resources`, `Posture`, `Heavy`).
- Kind `cloud_resource_v2` in `crates/stave-sdk/src/kinds.rs`, id field
  `id`, no severity field, primary timestamp `firstSeen`, search field
  `name`.
- Allowlist additions in `scripts/scrub.sh` plus eight selftest cases.
  Read the section on that below; it is the one change here that touches
  a security control.

No CLI code changed. The CLI derives its kind vocabulary, its clap
`PossibleValuesParser`, its `explain` schema dump, and its
kind-to-operation checks from `kinds::ALL_KINDS` and
`kinds::all_kinds()`, so a table entry is the whole wiring.

## Beside, not instead

The ticket left open whether `cloudResourcesV2` replaces the v1 binding
or ships beside it. It ships beside it, as a second kind.

Re-pointing `cloud_resource` at `cloudResourcesV2` would change the
field set of a stream that already has consumers: `examples/fixtures/`,
`examples/recipes/resource-inventory.sh`, the `account-context` enrich
recipe, and the assert scripts all read the v1 shape. That is a
stream-contract break, and per `.claude/rules/cli-philosophy.md` the
JSONL shape is public API. Additive is reversible; a rename is not.

## Naming

`cloud_resource_v2`, not a semantically fresh name like `asset` or
`resource_detail`.

The two kinds are the same noun at two levels of detail. A distinct
name would assert a distinction that does not exist, and an operator
who learned `asset` would then have to unlearn it if v1 is ever retired
and the survivor becomes plain `cloud_resource`. The `v2` suffix is
also the vendor's own word for the root field, so the audit trail maps
`cloud_resource_v2` to `cloudResourcesV2` with nothing to infer. The
paper-pipelines document already writes "`list cloud_resource` (V2)",
so V2 is the vocabulary in circulation.

## What was selected

`CloudResourceV2` has 55 fields. This document selects 27 of them (plus
sub-selections). Every one traces to a runbook step named in the field
surface audit:

| Selected | Why |
|---|---|
| `id`, `name`, `type`, `nativeType` | identity; the v1 selection minus the subscription pair |
| `externalId`, `providerUniqueId`, `region` | B6, B7 join keys |
| `cloudPlatform`, `status` | inventory basics; `status` is B13/C18 |
| `firstSeen`, `lastSeen`, `createdAt`, `deletedAt` | B13.2, C18.2 |
| `isAccessibleFromInternet`, `isOpenToAllInternet` | A2.3, A3.2 |
| `hasSensitiveData`, `hasAccessToSensitiveData` | A3.3 |
| `owners { type, graphEntity { id name type } }` | A1.4, A2.4, C20 |
| `projects { id name slug }` | A1.4, A2.4, B8, C20 |
| `tags { key value }` | B7, B8, C20 |
| `cloudAccount { id name externalId cloudProvider }` | account attribution without a second call |
| `iacDetails { iacStatus iacPlatform iacDetectionMethod iacDriftDetectionMethod }` | C15 |
| `iacDeployment`, `iacModuleSource`, `codeRepository` (id/name/type) | C15.1, C15.2 |
| `issueAnalytics`, `vulnerabilityAnalytics` (six counts each) | A3.4 ranking |

`owners.type` deserves a note. C20.2 asks for the attribution *basis*,
and `CloudResourceOwnerType` is exactly that: `DECLARED_OWNER`,
`TAG`, `ACTIVITY_CREATOR`, `CODE_OWNER`, and so on. It answers the
question without the owner's identity, which matters because the
identity is redacted by the scrubber and the basis is not.

## What was deliberately left out

- **`owners.evidence`.** A six-member union
  (`CloudResourceOwnerEvidence`). Selecting it needs inline fragments on
  every member, and several members carry login records and tag values.
  `owners.type` gives the basis at a fraction of the cost and none of
  the exposure.
- **`graphEntity`, `properties`, `originalObject`, `providerData`,
  `peripheralData`, `typedProperties`.** Unbounded `JSON` blobs. Every
  one would arrive as an unclassified object, which is the worst
  possible input to a default-deny scrubber and an unbounded cost to the
  tenant.
- **`revisions`.** A nested connection with its own paging. Paging
  inside a paged read is a design decision, not a field selection; it
  belongs with the history work (audit class 3).
- **`totalCount` on the connection.** Useful for B6.2 and cheap to add
  later, but on a large connection the server may compute it per page.
  Left out until someone can measure it.
- **`hasAdminPrivileges`, `hasHighPrivileges`, `secretAnalytics`,
  `technology`, `versionDetails`, `resourceGroup`, the encryption and
  PQC fields, `applicationServices`, the three
  `containerImageExecutionContextAnalytics*` variants.** Real fields, no
  runbook asks for them. Every one is work the tenant does.
- **`filterBy` and `orderBy`.** They exist on this root field and are
  the correct fix for the full-connection walk (charter F2, bd
  `aae-orc-j1xi`). Shipping a new binding and a new filter surface in
  one document would make a live failure ambiguous about which half
  broke.
- **`exportUrl`.** `CloudResourceV2Connection` does not carry it, so the
  denied-selection check was never in play here. Checked rather than
  assumed.

## The scrubber change

`scripts/scrub.sh` runs a default-deny field allowlist, so a new kind is
safe on arrival: every field it introduces is redacted until classified.
That is correct and it is also why the change could not stop there. Left
alone, `cloud_resource_v2` output after scrubbing is `_kind`, `_source`,
`type`, `cloudPlatform`, `createdAt`, `status`, and forty
`<redacted:...>` markers. The binding exists to serve runbooks that are
required to be run through the scrubber, so an unclassified selection is
a binding that cannot be used for its purpose.

Added: the booleans, the twelve analytics counts, `firstSeen`,
`lastSeen`, `deletedAt`, the four IaC enums, and three containers
(`owners`, `issueAnalytics`, `vulnerabilityAnalytics`). These are the
three classes the script's own header already names as safe (enums,
booleans, counts, timestamps); no new class was invented.

Still redacted, and covered by four new negative selftests: `tags`
(tenant-authored key and value), `owners.graphEntity` (a person),
`region`, and `codeRepository` (an org-named repo). Also still redacted
without needing a test, because their containers are not allowed:
`name`, `externalId`, `providerUniqueId`, `projects`, `cloudAccount`,
`iacDeployment`, `iacModuleSource`.

Two positive controls run: the existing one, and a new one asserting
`isAccessibleFromInternet`, an owner `type`, and a severity count all
survive.

This is the one hunk here that widens a security control. It is
separable from the rest of the change if a reviewer wants it argued on
its own.

## Should v1 be retired

Not yet, and not on the evidence available offline. Three things have to
be true first, and none can be established without the tenant:

1. **`cloudResourcesV2` returns the same population as
   `cloudResources`.** The names suggest it, the schema does not say it.
   `CloudResourceV2` carries `isRepresentativeResource` and
   `isAvailableOnGraph`, which hint that the V2 connection may apply its
   own inclusion rules. If the two roots return different row counts for
   the same tenant, they are not versions of each other and both stay.
2. **The richer selection does not time out or throttle at inventory
   scale.** The runbooks read up to 50,000 records. Fifty fields per
   record with two rollups and four entity references is a different
   query cost from six scalars, and the failure mode of the heavier one
   is unknown.
3. **The scopes are actually the same.** `read:resources` on the v2
   entry is copied from v1, not derived. If Wiz gates the richer root
   differently, a credential that reads v1 may not read v2, and
   retiring v1 would break a working read.

If all three hold, retirement is right and the shape is: rename the kind
to `cloud_resource`, keep `cloud_resource_v2` as an alias for one
release, update the fixtures and the `account-context` recipe, and file
the stream-contract break as a release event. A thin binding kept
alongside a strictly better one is drift, not caution. But that is an
argument to make after the measurement, not before it.

## Surprises

- **`cloudResources` is not simply the old version of
  `cloudResourcesV2`.** V1 carries `subscriptionName` and
  `subscriptionExternalId` as flat scalars; V2 has neither, and reaches
  the account through `cloudAccount` instead. So the `account-context`
  enrich recipe, which joins `cloud_resource.subscriptionExternalId`
  against `cloud_account.externalId`, does not apply to the new kind.
  It does not break either: `enrich` checks `record.kind` and passes
  non-matching records through untouched. The v2 records simply arrive
  with the account already attached.
- **`Sensitivity` cannot describe this operation honestly.** It is a
  single value, and this selection carries both posture data (exposure,
  sensitive-data flags, per-severity counts) and identity data (owner
  entities). `Posture` is the closer of the two available answers. Worth
  a follow-up on whether `Sensitivity` should be a set.
- **The connection has no `exportUrl`.** Twenty-six connection types in
  the schema carry it and this is not one of them.
- **`CloudResourceOwnerType` includes `CODE_OWNER`**, which means C15's
  code attribution and C20's team attribution can meet on the same
  field. Not exploited here; noted for the verb exercise.

## Needs live validation

Nothing below has been through the safety gate. Charter B2 and F1 treat
every selection as provisional until it has.

1. **The document validates server-side.** `check-ops` confirms the
   field names against the vendored schema; it cannot confirm the server
   accepts the query. First run should be the smallest possible read.
2. **`read:resources` grants `cloudResourcesV2`.** Copied from v1, not
   derived. A scope failure here is the most likely first failure.
3. **Cost at page size.** Whether the default page size holds for a
   selection this wide, or whether the server throttles, truncates, or
   times out.
4. **Population parity with v1.** Row counts for the same tenant from
   both roots. This is the measurement that decides the retirement
   question, and it is one comparison.
5. **`firstSeen` is the right `--since` field.** It is non-null in the
   schema. Whether it is populated and monotonic in practice is a
   separate question, and `--since` silently misbehaves if it is not.
6. **The nullable composites are populated.** `owners`, `projects`,
   `tags`, `iacDetails`, `codeRepository`, `issueAnalytics`, and
   `vulnerabilityAnalytics` are all nullable. A field that validates and
   always returns null looks identical to a field that works, which is
   the failure mode this list exists to catch.
7. **The scrubber survives real output.** The selftest uses synthetic
   records shaped by hand. Run a scrubbed read and confirm nothing
   identifying reaches the terminal before any of it is quoted anywhere.
