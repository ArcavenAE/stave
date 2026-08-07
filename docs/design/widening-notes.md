# Widening notes: what was added to the curated documents, and what still needs a tenant

bd `aae-orc-qijl` (widen the selections) and `aae-orc-x1rg` (select
`severity` on vulnerability findings). Written 2026-08-07, offline, with
no tenant contact and no `stave` invocation.

Specification: `docs/design/field-surface-audit.md`. Consumer:
`docs/runbooks/attemptability.md`, whose SURFACE-blocked column is what
this change is trying to move.

**Status of everything below: PROVISIONAL.** Charter B2 and F1 say
selections are provisional until live-validated. The twelve documents
were validated against the vendored schema (`cargo xtask check-ops`, 12
of 12, zero warnings) and against nothing else. These are now the
newest and least-tested selections in the repo.

---

## What landed

`cargo xtask check-ops`: 12 documents validated, no errors, no warnings.
`cargo test --workspace`: 17 test binaries, all pass.

Nine of the twelve kinds were widened. `cloud_resource`,
`cloud_configuration_rule`, and `report` were not: the audit puts
`cloud_resource` under class 2 (the type has eight fields and stave
selects six; the answer is binding `cloudResourcesV2`, tracked
separately) and the other two were not on this ticket.

### issue

Added `statusChangedAt`, `reopenedAt`, `openReason`, `resolutionReason`,
`rejectionExpiredAt`, `resolvedBy { type, user { id name },
serviceAccount { id name } }`, `assignee { id name }`,
`projects { id name slug }`, `serviceTickets { id name externalId }`,
and `sourceRules { __typename id name }`.

`assignee` is an `Identity`, which carries `emails` and `primaryEmail`.
Held to id and name: A1 step 4 asks who owns the issue, not how to mail
them.

`serviceTickets` has a `url: String!` that was left out. B12 asks
whether a ticket exists and which one; `externalId` answers that without
putting the ITSM hostname in every issue record.

### vulnerability_finding

The `aae-orc-x1rg` correction: `severity` is now selected, first, with
`vendorSeverity` kept beside it. `severity` is
`VulnerabilitySeverity!`, non-null and canonical; `vendorSeverity` is
nullable and vendor-reported. Keeping both means the gap between them
stays visible, which is itself a signal about a vendor feed.

Also added `hasFix`, `fixedVersion`, `hasExploit`, `cisaKevDueDate`,
`projects { id name slug }`, `layerMetadata { id layerHash isBaseLayer }`,
`rootComponent { name }`, plus `vulnerableAsset` and
`selectedIgnoreRules` (both described under type surprises below).

`layerMetadata.details` was left out: it is free text of unknown size on
a connection that is already the heaviest in the registry.

### service_account and audit_log_entry

These two are runbook A4, which the audit reports as blocked at every
step past bucketing by age. They were done first for that reason.

service_account gained `lastLoginAt`, `lastRotatedAt`, `expiresAt`,
`enabled`, and `scopes`. `clientSecret` sits on this same type and is a
live credential on the xtask deny-list. It is not selected, the deny
rule is exercised by an existing xtask test, and a companion test proves
the rule does not block `scopes` next door.

audit_log_entry gained `actionType`, `actionParameters`, `sourceIP`, and
`performer { __typename id name }`.

`sourceIP` is a real address. The leak-pattern tier scrubs IPs, so it
does not block a commit, but it is tenant data in a transcript.

### cloud_account

Added `firstScannedAt`, `criticalSystemHealthIssueCount`,
`highSystemHealthIssueCount`, `linkedProjects { id name slug }`, and
`sourceDeployments { id name type status lastSeenAt }`.

Only the critical and high system-health counts were taken. The
medium, low, and informational three exist and are noise at this
altitude.

### project

Added `businessUnit`, `projectOwners { id name }`,
`securityChampions { id name }`, and `tags { key value }`. Both owner
fields are `[User!]!` and are held to id and name for the same reason as
`Issue.assignee`.

### control

Added `lastRunAt`, `lastSuccessfulRunAt`,
`serviceTickets { id name externalId }`, and
`securitySubCategories { id externalId title }`.

`lastRunAt` and `lastSuccessfulRunAt` are a pair on purpose. B9 asks
whether a control is actually running, and a control that is enabled and
failing every run is indistinguishable from a healthy one on `enabled`
alone.

### security_framework

Added `controls` and `cloudConfigurationRules`, each as
`(first: 100) { totalCount nodes { id name enabled severity } pageInfo {
hasNextPage endCursor } }`.

This is the one document whose cost changed shape rather than degree.
See "The nested-connection hazard" below before running it.

### user

Added `lastLoginAt`, `enabled`, `isSuspended`, and
`effectiveRole { id name }`. `UserRole.scopes` is the interesting field
for a privilege review and was left out: it is a separate ask, not a
free rider on the roster query.

---

## What could not be added, and why

Ten fields on the widening list are deprecated in the vendored schema.
None were added. In each case the schema's own deprecation text names a
successor, and the successor was selected instead. Selecting a
deprecated field is not blocked (check-ops warns rather than fails), so
this was a judgment: a permanent warning in the build plus a field the
vendor has announced it is removing is a poor trade for data the
successor already carries.

| Wanted | Schema says | Selected instead |
|---|---|---|
| `Issue.control` | deprecated, "use sourceRules instead" | `sourceRules { __typename id name }` |
| `VulnerabilityFinding.ignoreRules` | deprecated, "use selected ignore rules instead" | `selectedIgnoreRules` |
| `AuditLogEntry.user` | deprecated, "Use `performer` instead" | `performer` |
| `AuditLogEntry.serviceAccount` | deprecated, "Use `performer` instead" | `performer` |
| `CloudAccount.status` | deprecated, "refer to Deployments and System Health Issues instead" | `criticalSystemHealthIssueCount`, `highSystemHealthIssueCount`, `sourceDeployments` |
| `CloudAccount.connector` | deprecated, "use sourceConnectors instead", and `sourceConnectors` is itself deprecated in favour of `sourceDeployments` | `sourceDeployments { id name type status lastSeenAt }` |
| `User.role` | deprecated, "No longer supported" | `effectiveRole { id name }` |

`CloudAccount.status` deserves its own line. It is not merely
deprecated: the 2026-08-06 F1 live-validation pass removed it from this
document on purpose, and the removal is recorded in the document's own
header comment. Re-adding it would revert a validated correction on the
strength of an offline reading. It was not re-added.

Two substitutions are not clean swaps and should be read as such:

- `sourceRules` is `[IssueSourceRule!]`, an interface implemented by
  `Control`, `CloudConfigurationRule`, and `CloudEventRule`. `control`
  returned a `Control` specifically. `__typename` recovers which kind of
  rule raised the issue, so nothing is lost for A5 or B9, but a consumer
  keying on "this is a control" must now check `__typename` rather than
  assume it.
- `performer` is `SystemPrincipalSnapshot!`, an interface carrying only
  `id` and `name`. `user` and `serviceAccount` returned the full records.
  `__typename` distinguishes `UserSnapshot` from
  `ServiceAccountSnapshot`, which is what A4 step 3 and B10 step 1 need.
  The snapshot types each carry a link back to the full `User` or
  `ServiceAccount`; those links were deliberately not followed, because
  doing so would pull employee email into every audit line for no
  runbook step that asks for it.

---

## Types that surprised me

**`vulnerableAsset` is a union of fourteen members, and all fourteen
implement one interface.** `VulnerableAsset` unions
`VulnerableAssetArtifact`, `...Common`, `...Container`,
`...ContainerImage`, `...Device`, `...Endpoint`, `...Ide`,
`...NetworkAddress`, `...PaaSResource`, `...Repository`,
`...RepositoryBranch`, `...Serverless`, `...VirtualMachine`, and
`...VirtualMachineImage`. Every one declares `implements
VulnerableAssetBase`. So a single inline fragment on the interface
serves all fourteen, instead of fourteen fragments. GraphQL permits a
fragment on an interface inside a union when the intersection is
non-empty, and here the intersection is the whole union. **This is the
single riskiest construct in the change** and the first thing to check
against a live response.

**`VulnerableAssetBase` carries internet-exposure booleans.** It has
`hasWideInternetExposure` and `hasLimitedInternetExposure`, both
selected. The audit classifies A2.3 and A3.2 ("internet-reachable",
"internet-exposed") as OURS/binding on the grounds that
`isAccessibleFromInternet` exists only on `CloudResourceV2`. That is
correct for the resource path. It appears not to be the whole story for
the finding path, which has its own exposure fields. I am **not**
claiming these are equivalent: they are differently named fields on a
different type and their semantics are unverified. If they turn out to
answer the same question, part of A2.3 and A3.2 may be reachable without
the V2 binding, and the attemptability table would need revising. This
is a claim to test, not a result.

**`SecurityFramework.controls` and `.cloudConfigurationRules` are
connections, not lists.** Both return `...Connection!` with
`nodes`/`pageInfo`/`totalCount`. This is the one place the audit's
per-kind table reads like a plain field list and the schema disagrees.

**`ControlConnection` carries `exportUrl`.** It is on the xtask
deny-list (it mints a server-side export artifact) and is not selected.
Worth knowing that widening toward a connection type walks straight past
a denied field.

**`SelectedIgnoreRule` is a union of `ExternalIgnoreRuleDetails |
IgnoreRule`,** with no shared interface, so it needs two inline
fragments. `expiredAt` (C19 step 3, exceptions past expiry) exists only
on the `IgnoreRule` arm. An externally-sourced ignore rule reports id and
name and no expiry at all, which is itself an answer C19 should record
rather than treat as missing data.

**`AuditLogEntry.actionParameters` is `JSON!`,** an opaque custom
scalar. I have assumed it deserializes to an object. It could be an
array, a string, or null-in-practice despite the non-null marker. B10
step 1 ("what changed") depends entirely on its shape and nothing
offline can tell me what that shape is.

**`Control.serviceTickets` takes an optional `selection` argument.** It
is omitted, so the server default applies, and what that default selects
is not stated in the schema.

**Nullability is inconsistent across the joins.** `Issue.projects` is
`[Project]` (nullable elements), `Project.projectOwners` is `[User!]!`
(non-null throughout), `VulnerabilityFinding.projects` is `[Project]`.
Consumers should not assume a uniform shape across the three.

---

## The nested-connection hazard

`list_security_frameworks` now fans out multiplicatively where the other
eleven documents do not. Two nested connections of up to 100 ride on
every framework in the outer page, and stave's pager
(`crates/stave-cli/src/main.rs`) walks the outer connection only:
`next_cursor` reads the root field's `pageInfo` and nothing else.

Two consequences, both deliberate and both imperfect:

1. **`first: 100` is a literal.** The SDK binds only `$first` and
   `$after`, and both apply to the outer connection, so a nested page
   size cannot be a variable without changing the SDK. 100 is a
   judgment, not a measurement.
2. **A framework with more than 100 controls returns a partial roster.**
   `totalCount` and the inner `pageInfo.hasNextPage` are selected so
   truncation is visible in the record instead of looking complete.
   Nothing in stave follows the inner cursor today, and nothing should
   be built on the assumption that it does.

Run this one at a low `--limit` against the live tenant. At
`--limit 500` the worst case is 500 frameworks times 200 nested nodes in
one response.

---

## Registry changes, and the scope rule I used

Descriptions were rewritten to name what each document now returns.
`cost_hint` moved Light to Heavy for `cloud_account`, `project`,
`control`, and `security_framework`, each of which gained joins. These
are judgments about relative cost, not measurements; nothing here has
been timed against the tenant.

`sensitivity` on `project` moved Normal to Identity. `projectOwners` and
`securityChampions` name real employees, and the field is documented as
saying what an operation exposes, so Normal had become wrong.

`sensitivity` on `audit_log_entry` was left at Posture even though it now
names principals. `Sensitivity` holds one value rather than a set, so
switching to Identity would drop the posture signal without adding one.
The single-value limitation is the actual defect and is worth a ticket.

`required_scopes` gained joined scopes under one rule, stated so it can
be audited: **a joined type earns a scope only when another operation in
this registry lists exactly that type as its kind.** `Issue.projects`
reads `Project` and `list_projects` declares `read:projects`, so
`read:projects` was added. Joins to `Identity`, `VulnerableAsset`,
`Deployment`, `ServiceTicket`, `SecuritySubCategory`, `UserRole`, and
`IgnoreRule` added nothing, because no registry operation names those
types and inventing a scope name would put a guess in front of an
operator as a provisioning instruction.

Result:

| Operation | Scopes now |
|---|---|
| `list_issues` | `read:issues`, `read:projects`, `read:users`, `read:service_accounts` |
| `list_vulnerability_findings` | `read:vulnerabilities`, `read:projects` |
| `list_cloud_accounts` | `read:cloud_accounts`, `read:projects` |
| `list_projects` | `read:projects`, `read:users` |
| `list_security_frameworks` | `read:security_frameworks`, `read:controls`, `read:cloud_configuration` |
| others | unchanged |

**The rule assumes a permission model nobody has verified.** It assumes
Wiz enforces scopes at nested-field granularity, so that reading
`Issue.projects` requires the project scope. That is an assumption about
the model, not just about the names, and F1 already records that scope
qualification did not manifest as expected on the development service
account. If the assumption is wrong in the permissive direction, `auth
plan` now over-provisions, which `auth plan --check` reports as EXCESS
and is trimmable. If it is wrong in the other direction, nothing here
helps. Under-declaring was the worse failure to pick, because it makes
`can-i` say yes to a call the server then refuses.

### Three permission tests changed

`crates/stave-cli/tests/permissions.rs` had three tests that hardcoded
`list_issues` as a one-scope fixture. They broke on a change that had
nothing to say about permission logic. They now read the scope set out of
the registry via `stave_sdk::ops::find`, so the next widening breaks them
only if it breaks the logic. The excess-drift arm needed a scope outside
the requirement and uses `read:reports`, with an assertion that it stays
outside.

---

## What needs live validation

Everything above. In rough order of how loudly a wrong guess will fail
and how much rides on it:

1. **The `vulnerableAsset` interface fragment.** One inline fragment on
   `VulnerableAssetBase` standing in for fourteen union members. Valid
   per the spec and unexercised against this server. If it is refused,
   the fallback is fourteen fragments or a narrower asset selection.
2. **`actionParameters`.** Is `JSON!` an object in practice, and does it
   carry the before/after detail B10 step 1 needs, or only an action
   name.
3. **The nested connections on `list_security_frameworks`.** Does
   `first: 100` apply, does `totalCount` return, and what does the
   response actually cost. Check at `--limit 1` first.
4. **The seven deprecated-field substitutions.** Each was chosen from
   deprecation text, not from observed data. The specific question per
   row is whether the successor is populated where the deprecated field
   would have been. `sourceDeployments` and the system-health counts are
   the least certain, because they replace `CloudAccount.status` with a
   differently shaped signal rather than the same one under a new name.
5. **The joined scopes.** Whether the nested-field permission model
   exists at all, before whether the names are right.
6. **The exposure-boolean question** under type surprises. If
   `hasWideInternetExposure` answers A2.3, `docs/runbooks/attemptability.md`
   and the audit's class-2 count both need revising.
7. **The four cost-hint flips.** Unmeasured judgments.
8. **`sourceRules` and `performer` `__typename` values.** The schema
   names three and two implementers respectively. Whether the tenant
   returns others is a schema-versus-runtime question.

Do all of this under the safety-coach gate, at low `--limit`, through
`scripts/scrub.sh`.

---

## One thing this widening does not finish

**The scrubber redacts almost every field added here.**
`scripts/scrub.sh` runs a default-deny field allowlist, and the widening
did not touch it. Measured on a synthetic record:

```
$ printf '%s\n' '{"_kind":"service_account","id":"sa-1","name":"example",
  "enabled":true,"lastLoginAt":"...","lastRotatedAt":"...","scopes":[...]}' \
  | ./scripts/scrub.sh
{"_kind":"service_account","id":"<redacted:id>","name":"<redacted:name>",
 "enabled":true,"lastLoginAt":"<redacted:lastLoginAt>",
 "lastRotatedAt":"<redacted:lastRotatedAt>","scopes":"<redacted:scopes>"}
```

`lastLoginAt` and `lastRotatedAt` are runbook A4's whole question, and
they are timestamps carrying no tenant identity, in the same class as
`createdAt`, which the allowlist does permit. The tenant-data-hygiene
rule requires the scrubber in the pipe, so A4 becomes reachable at the
API and unreadable at the terminal in the same change.

The same applies to `statusChangedAt`, `reopenedAt`, `openReason`,
`resolutionReason`, `rejectionExpiredAt`, `hasFix`, `hasExploit`,
`fixedVersion`, `cisaKevDueDate`, `isBaseLayer`, `expiresAt`,
`isSuspended`, `firstScannedAt`, `lastRunAt`, `lastSuccessfulRunAt`,
`lastSeenAt`, `expiredAt`, `totalCount`,
`criticalSystemHealthIssueCount`, `highSystemHealthIssueCount`,
`hasWideInternetExposure`, `hasLimitedInternetExposure`, `nativeType`,
and `region`.

**`scripts/scrub.sh` was deliberately not changed.** It is a security
control, default-deny is the correct posture, and failing closed is the
right direction to fail. Widening a redaction allowlist is a decision
for a human who can weigh each field, not a side effect of a field
selection ticket. Filing it as its own piece of work is the ask.
