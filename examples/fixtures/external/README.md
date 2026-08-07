# external fixtures: the inputs the security graph does not hold

Four synthetic exports standing in for the systems the class B join
runbooks reconcile against: a CMDB extract, a vended-account roster, a
ticket export, and an IaC inventory. Built for bd `aae-orc-e4jo.4`;
consumed by `.6`.

Everything here is invented. Nothing was copied from a live tenant, and
no `stave` invocation produced or verified any of it. Every count in
this file was measured with jq and python over the committed files, so a
runbook attempt can be checked against a stated answer rather than
against a plausible one.

```
external/
├── vended_account.jsonl   14 records   account vending pipeline   (B11, C16, B8, C20)
├── cmdb_record.jsonl      18 records   CMDB configuration items   (B7, B8, B13, C20)
├── ticket.jsonl           16 records   security ticket export     (B12, C14)
└── iac_component.jsonl    15 records   IaC / artifact inventory   (C15, B7, B8)
```

## Stream shape

Same JSONL contract as the graph-side fixtures: a `_kind` discriminator,
a `_source` provenance object, domain fields alongside. That is
deliberate. A join runbook concatenates external records with graph
records and filters by `_kind`, rather than carrying a second parser for
its second input.

```json
{"_kind":"vended_account",
 "_source":{"operation_id":"external.account_vending.export",
            "response_index":0,
            "fetched_at":"2026-08-06T00:00:00Z"},
 "sourceSystem":"account-vending",
 "accountKey":"123456789012", ...}
```

Three notes on the shape:

- **`_source` carries the three fields `SourceRef` requires**
  (`crates/stave-sdk/src/stream.rs`): `operation_id`, `response_index`,
  `fetched_at`. All 63 records conform, so `stave filter` and the CEL
  adapter read these files the same way they read a graph stream. The
  `operation_id` values are `external.*` prefixed and are not stave
  operations; nothing in `stave-api` will resolve them.
- **`sourceSystem` is a top-level domain field, not a `_source` key.**
  `SourceRef` serializes only its own four fields, so an extra key
  inside `_source` would be dropped the first time a record passed
  through a primitive. A domain field survives the round trip.
- **`scripts/scrub.sh` redacts all of it.** The field allowlist is
  default-deny and knows only graph fields, so a scrubbed external
  record is almost entirely `<redacted:...>`. That is the allowlist
  working as designed. These files are synthetic and need no scrubbing;
  the note is here so nobody reads a wall of redactions as a defect.

## The graph side, and the ceiling it sets

These fixtures join against the committed graph-side fixtures one
directory up: `cloud_account.jsonl` (2 records), `cloud_resource.jsonl`
(4), `issue.jsonl` (4), `vulnerability_finding.jsonl` (5).

That is a regression fixture, not an estate. Correspondence percentages
below are therefore low on the graph-facing pairs, and the ratio of
unscanned to scanned accounts in B11 is far worse than any real tenant
would show. The counts are exact; the ratios are an artifact of a
two-record account fixture. Read them as "this answer is checkable",
not as "this is what an estate looks like".

Two graph-side facts worth holding while reading the answers:

- `cloud_resource.jsonl` has a resource (`example-corp-legacy-share`) in
  subscription `10000000-0000-4000-8000-000000000009`, for which
  `cloud_account.jsonl` has no account record. The scanner returns
  resources from an account it has no account row for.
- `cloud_account.jsonl` predates the F1 correction: it still carries
  `status` and does not carry `lastScannedAt` or `resourceCount`, which
  the curated operation now selects. It was left alone because
  `asserts/02-cross-kind-enrich.sh` pins the account summary shape. The
  consequence for C16 is stated below.

## vended_account.jsonl

The authoritative roster from the account vending pipeline. The one
fixture whose consuming runbook (B11) has a graph half that today's
curated read surface already serves, which makes it the only one of the
four that yields an end-to-end attempt rather than a partial one.

| Field | Meaning |
|---|---|
| `accountKey` | cloud-native account identifier; joins `cloud_account.externalId`. Nullable. |
| `accountAlias` | vending-pipeline name; joins `cloud_account.name` weakly |
| `cloudProvider` | AWS / Azure / GCP |
| `environment` | production / staging / sandbox / lab |
| `vendedAt` | when the pipeline created the account |
| `status` | active / closed |
| `closedAt` | nullable |
| `ownerTeam`, `ownerTeamStatus` | requesting team, and whether it still exists (active / dissolved / unknown) |
| `businessUnit` | |
| `requestTicket` | the vending request, in an `OPS-` namespace |

`ownerTeamStatus` is where the dissolved-team fact lives. A real
deployment would source it from the identity provider or the HR system;
that system is not one of the four fixtures, so the roster carries the
verdict directly.

**Defect profile.** 14 records.

| Defect | Count | Which |
|---|---|---|
| Missing join key (`accountKey` null) | 2 | example-corp-legacy-billing, example-corp-acquired-northwind |
| Duplicate join key | 1 pair | two rows on `123456789012`: an active one and a closed re-vend artifact |
| Closed account, scanner still connected | 2 rows | example-corp-sandbox (unambiguous); example-corp-prod-legacy-entry (ambiguous, its key also has an active row) |
| Closed and correctly absent from the scanner | 1 | example-corp-sandbox-two, the control case |
| Owner team dissolved | 2 | example-corp-sandbox, example-corp-legacy-billing |
| Owner team unknown, `ownerTeam` null | 1 | example-corp-acquired-northwind |
| Key shape varies by provider | 6 of 11 distinct keys | 4 Azure GUIDs and 2 GCP project-style ids, so a join written for 12-digit AWS ids silently drops more than half the roster |

The duplicate key is not decoration. It is the reason B13's
closed-account answer below has to be given as two numbers.

## cmdb_record.jsonl

A configuration-item export. Joins to `cloud_resource` on `name`, which
is the only resource-level key pair available, for the reason in the
matrix below.

| Field | Meaning |
|---|---|
| `ciId` | CMDB primary key, no counterpart anywhere else |
| `name` | joins `cloud_resource.name` |
| `nativeId` | cloud-native resource id. Nullable. No graph counterpart. |
| `accountKey` | joins `cloud_resource.subscriptionExternalId` and `vended_account.accountKey`. Nullable. |
| `ciType` | compares against `cloud_resource.type` |
| `lifecycleState` | active / retired / decommissioned / planned |
| `retiredAt` | nullable |
| `owner`, `environment`, `criticality` | the three attributes B7 bucket three asks about. None has a graph counterpart. |
| `lastReviewedAt` | |

**Defect profile.** 18 records, 17 distinct names.

| Defect | Count | Which |
|---|---|---|
| In the cloud, absent from the CMDB | 1 | example-corp-build-agent |
| In the CMDB, absent from the cloud | 14 records, 14 names | |
| Duplicate CI records for one resource | 1 pair | CI-0001 and CI-0004, both example-corp-audit-logs, disagreeing on owner (team-payments vs team-atlas) and criticality (tier-1 vs tier-2) |
| Type disagrees with the graph | 1 | CI-0002 says DATABASE, the graph says VIRTUAL_MACHINE |
| Account disagrees with the graph | 1 | CI-0002 points at the Azure sandbox, the graph at `123456789012` |
| Retired record, resource still in the cloud | 1 | CI-0003 example-corp-legacy-share, retired 2026-01-31 |
| Retired and correctly absent | 3 | CI-0008, CI-0009, CI-0017, the control cases |
| Missing `accountKey` | 3 | CI-0007, CI-0010, CI-0011 |
| Missing `nativeId` | 4 | |
| Missing `owner` | 1 | CI-0007 |
| Owner is a dissolved team | 5 | CI-0003, CI-0004, CI-0009, CI-0017, CI-0018 |
| References an account in neither the roster nor the graph | 1 | CI-0018, account `123456789099` |
| Planned, never built | 1 | CI-0014 |

CI-0018 is the shadow-account direction. The roster is authoritative by
construction, so an account it never vended can only surface downstream,
which is where it surfaces here.

## ticket.jsonl

A security ticket export. Joins to `issue` on `issueKey`, with
`resourceName` as a fallback key that changes the answer.

| Field | Meaning |
|---|---|
| `key` | ticket id |
| `summary`, `assignee`, `team` | |
| `status` | open / in_progress / closed / cancelled |
| `resolution` | fixed / duplicate / wont_fix, null while open |
| `createdAt`, `closedAt` | |
| `issueKey` | joins `issue.id`. Nullable. |
| `resourceName` | fallback key; joins `issue.entitySnapshot.name` and `cloud_resource.name`. Nullable. |
| `rootCause` | CVE id or `base-image:<ref>`; the grouping key for duplicate counting |

**Defect profile.** 16 records.

| Defect | Count | Which |
|---|---|---|
| Missing `issueKey` | 8 | half the export |
| No key of any kind | 1 | SECOPS-1208 |
| Dangling `issueKey` | 1 | SECOPS-1206 references `issue_99`, which no graph issue matches |
| Closed ticket, issue not resolved | 3 | SECOPS-1202, SECOPS-1203, SECOPS-1210 |
| Open ticket, issue already resolved | 1 | SECOPS-1216 against the resolved `issue_04` |
| Open issue with no ticket by `issueKey` | 1 | `issue_03` |
| Duplicates against one issue | 3 and 2 and 2 | `issue_01` has 3 tickets, `issue_02` has 2, `issue_04` has 2 |
| Duplicates against one root cause | 4 | the base-image cluster: SECOPS-1211, 1212, 1213, 1214 |
| Root cause the IaC inventory contradicts | 1 | SECOPS-1212 blames the base image; the inventory says that host runs a different artifact |

## iac_component.jsonl

An infrastructure-as-code and artifact inventory. Joins to
`cloud_resource` on `managedResourceName`.

| Field | Meaning |
|---|---|
| `componentId` | |
| `repo`, `path`, `commitSha`, `moduleVersion` | where the resource comes from |
| `artifactRef` | image or artifact the component deploys. Nullable. No graph counterpart. |
| `managedResourceName` | joins `cloud_resource.name` and `cmdb_record.name`. Nullable. |
| `accountKey` | joins `vended_account.accountKey`. Nullable. |
| `owner`, `environment` | second opinion on the two attributes the CMDB also asserts |
| `lastAppliedAt` | null when the component has never been applied |
| `driftDetected` | |

**Defect profile.** 15 records across 6 repos.

| Defect | Count | Which |
|---|---|---|
| Missing `managedResourceName` | 3 | iac-0004, iac-0011 (module-level), iac-0015 (the base image source) |
| Missing `accountKey` | 3 | |
| Owner disagrees with the CMDB | 3 | iac-0001 vs CI-0001 and vs CI-0004; iac-0014 vs a CMDB record with no owner |
| Environment disagrees with the CMDB | 3 | iac-0002, iac-0008, iac-0014 against CI-0002, CI-0012, CI-0007 |
| Still managing a resource the CMDB retired | 1 | iac-0007, last applied 2024-05-11, drift detected |
| Never applied | 1 | iac-0010, matching the planned CI-0014 |
| Drift detected | 2 | iac-0002, iac-0007 |

## B6: join keys and their population

Join-key coverage is itself a runbook (B6), and every later cross-check
is supposed to state its ceiling up front. These are the ceilings.

**Population, per fixture.**

| Key | Populated | Rate |
|---|---|---|
| `vended_account.accountKey` | 12/14 | 85.7% |
| `vended_account.accountAlias` | 14/14 | 100% |
| `vended_account.ownerTeam` | 13/14 | 92.9% |
| `vended_account.requestTicket` | 10/14 | 71.4% |
| `cmdb_record.name` | 18/18 | 100% |
| `cmdb_record.nativeId` | 14/18 | 77.8% |
| `cmdb_record.accountKey` | 15/18 | 83.3% |
| `cmdb_record.owner` | 17/18 | 94.4% |
| `ticket.issueKey` | 8/16 | 50.0% |
| `ticket.resourceName` | 12/16 | 75.0% |
| `ticket.rootCause` | 14/16 | 87.5% |
| `iac_component.managedResourceName` | 12/15 | 80.0% |
| `iac_component.accountKey` | 12/15 | 80.0% |
| `iac_component.artifactRef` | 6/15 | 40.0% |

**Correspondence, distinct populated values that resolve on the other
side.**

| Key pair | Resolves | Rate |
|---|---|---|
| `vended_account.accountKey` -> `cloud_account.externalId` | 2/11 | 18.2% |
| `vended_account.accountKey` -> `cloud_resource.subscriptionExternalId` | 3/11 | 27.3% |
| `vended_account.accountAlias` -> `cloud_account.name` | 2/14 | 14.3% |
| `vended_account.requestTicket` -> nothing | 0/10 | 0% |
| `cmdb_record.name` -> `cloud_resource.name` | 3/17 | 17.6% |
| `cmdb_record.nativeId` -> nothing | 0/14 | 0% |
| `cmdb_record.accountKey` -> `cloud_account.externalId` | 2/10 | 20.0% |
| `cmdb_record.accountKey` -> `vended_account.accountKey` | 9/10 | 90.0% |
| `ticket.issueKey` -> `issue.id` | 3/4 | 75.0% |
| `ticket.resourceName` -> `cloud_resource.name` | 4/6 | 66.7% |
| `ticket.resourceName` -> `issue.entitySnapshot.name` | 3/6 | 50.0% |
| `iac_component.managedResourceName` -> `cloud_resource.name` | 4/12 | 33.3% |
| `iac_component.managedResourceName` -> `cmdb_record.name` | 11/12 | 91.7% |
| `iac_component.accountKey` -> `vended_account.accountKey` | 8/8 | 100% |
| `iac_component.artifactRef` -> nothing | 0/2 | 0% |

Three of these say something the fixture was built to say:

1. **A well-populated key can still be worthless.** `nativeId` is on 78%
   of CMDB records and `artifactRef` on 40% of IaC components, and both
   resolve against nothing, because stave's curated selections carry no
   native resource id and no artifact reference. That is a surface fact,
   not a data-quality fact. It is why B7's only resource-level key pair
   is `name`, and why C15's finding-to-artifact step has a ceiling of
   zero before the external input is even considered.
2. **The external systems agree with each other better than any of them
   agrees with the graph.** CMDB to roster is 90%, IaC to CMDB 91.7%,
   IaC to roster 100%, while every graph-facing pair sits under 30%. The
   graph-facing rates are depressed by the two-record account fixture;
   the shape of the gap is not.
3. **Some key pairs need a transform before they correspond at all.**
   `ticket.rootCause` matches `iac_component.artifactRef` on 0 of 6
   distinct values as written, and on 1 of 6 after stripping the
   `base-image:` prefix. A coverage number computed without the
   normalization is wrong in the safe-looking direction.

## Known-correct answers

Computed over the committed files, as of `2026-08-06T00:00:00Z`, which
is the `fetched_at` on every record. Where a runbook's answer depends on
a choice the runbook does not make for you, both answers are given, and
the choice is named.

### B11, scan coverage gap

Roster keys: 11 distinct populated, 2 resolve to a scanner account, 9 do
not. Two further rows carry no key and cannot be evaluated either way.

Of the 9 unmatched, one (`example-corp-sandbox-two`) is closed and
therefore correctly unscanned. **The answer is 8 unscanned active
accounts**, or 9 if closed accounts are counted, which they should not
be.

Age at the as-of date, from `vendedAt`:

| Account | Key | Days |
|---|---|---|
| example-corp-shared-services | `10000000-0000-4000-8000-000000000009` | 612 |
| example-corp-security-tooling | `123456789016` | 537 |
| example-corp-web | `example-corp-web-8820` | 340 |
| example-corp-staging | `10000000-0000-4000-8000-000000000003` | 330 |
| example-corp-data-platform | `123456789014` | 175 |
| example-corp-analytics | `example-corp-analytics-4471` | 109 |
| example-corp-prod-dr | `123456789013` | 34 |
| example-corp-ml-lab | `123456789015` | 4 |

The worst case is the interesting one. `example-corp-shared-services`
has been unscanned for 612 days, and it is the subscription
`example-corp-legacy-share` sits in, so the scanner has been returning a
resource from an account it has no account record for the whole time.

Coverage ceiling to state up front: 85.7% of roster rows carry the join
key, so 2 accounts are outside the answer in both directions.

### B7, CMDB three-bucket reconciliation

Joining on `name`, which the matrix above shows is the only available
resource-level pair.

- **Bucket one, in the cloud and not in the CMDB: 1.**
  `example-corp-build-agent`.
- **Bucket two, in the CMDB and not in the cloud: 14 records, 14
  distinct names.** Three of them are retired or decommissioned and
  correctly absent, so a bucket-two list that does not subtract
  lifecycle state overstates by 3.
- **Bucket three, present in both and disagreeing: 1 record, 2
  attributes.** CI-0002 disagrees on type and on account.

Bucket three cannot be answered for owner, environment, or criticality,
which is what the runbook actually asks about. Those three fields have
no counterpart in stave's current selection, so the comparison has no
right-hand side. The fixture seeds them across the external systems
instead, where the disagreement is at least measurable: 3 owner
disagreements and 3 environment disagreements between the CMDB and the
IaC inventory, 2 environment disagreements between the CMDB and the
roster, and 1 criticality disagreement inside the CMDB itself, between
its two records for `example-corp-audit-logs`.

### B8, ownerless-resource cross-check

Cost data is the third input and is not one of these four fixtures, so
the three-way overlap cannot be closed here. The two-way is:

- CMDB records with no owner or a dissolved-team owner: **6**.
- Of those, on a resource carrying an open issue: **1**, CI-0004
  `example-corp-audit-logs`, which carries `issue_01`, a critical open
  issue, and is attributed to the dissolved team-atlas by a duplicate CI
  record while the primary record attributes it to team-payments.

That single row is the whole runbook in miniature: unattributed risk and
a CMDB data-quality failure are the same record.

### B12, ticket reconciliation

- **Open issues with no ticket: 1 by `issueKey` (`issue_03`), 0 if the
  `resourceName` fallback is allowed.** SECOPS-1211 names
  `example-corp-build-agent`, which is `issue_03`'s entity. Whichever
  number is reported, the key pair has to be named with it.
- **Closed tickets whose issue is not resolved: 3.** SECOPS-1202,
  SECOPS-1203, SECOPS-1210. It is 2 if `IN_PROGRESS` is not counted as
  open, and 1 if only `resolution: fixed` counts as a false closure
  rather than a dedup.
- **False-closure rate: 3 of 4 closed tickets that carry an
  `issueKey` (75%), or 3 of 6 closed tickets overall (50%).** The other
  two closed tickets have no key and cannot be judged.
- **Duplicate tickets against one root cause: 4** on the base image, 3
  each on CVE-2026-10001 and CVE-2026-10002, 2 on CVE-2025-10005.
- **Ticket age against issue age**, for the 7 tickets that resolve to a
  graph issue: lag is 0 to 3 days on six of them and 50 days on
  SECOPS-1216, which is a ticket opened against an issue resolved seven
  weeks earlier.
- One dangling reference (SECOPS-1206 to `issue_99`) and one ticket with
  no key at all (SECOPS-1208) sit outside every count above.

### B13, decommission verification

- **Retired or decommissioned CMDB records whose resource still exists:
  1.** CI-0003 `example-corp-legacy-share`, retired 2026-01-31.
- **Correctly retired: 3.** CI-0008, CI-0009, CI-0017. An answer that
  flags these has not checked the cloud side.
- **The reverse direction, a live resource with no CMDB record: 1.**
  `example-corp-build-agent`, the same record as B7 bucket one.
- **Closed accounts with a live connector: 1 unambiguous, 2 by a naive
  join.** `example-corp-sandbox` is closed in the roster and still has a
  scanner account. `example-corp-prod-legacy-entry` is also a closed row
  whose key still has a scanner account, but that key has an active row
  too, so a join that does not resolve the duplicate reports a false
  positive. This is the duplicate-key defect from the roster reaching a
  downstream answer, which is the point of seeding it.

### C15, fix-at-source mapping

- 4 tickets cite the base image as root cause.
- 3 are corroborated by the IaC inventory: SECOPS-1211, SECOPS-1213,
  SECOPS-1214, whose resources map to components carrying
  `example-corp/base-python:3.11-slim`.
- 1 is not: SECOPS-1212, on `example-corp-api-01`, which the inventory
  says runs `example-corp/api:2026.07.3`. The root cause on that ticket
  is wrong.
- All 3 corroborated tickets trace to **one source component**: iac-0015,
  `example-corp/base-images`, `python/Dockerfile`, commit
  `d7e8f90123456789012345678901abcdef012345`.

So the collapse is 4 tickets to 3 real instances to 1 fix, with 1 ticket
that needs re-diagnosing rather than fixing. The finding-to-artifact leg
of the runbook stays at zero: `vulnerability_finding` as selected
carries no resource reference and no artifact reference, so there is
nothing on the graph side to start the walk from.

### C16, account enrollment lifecycle

**0 of 14 accounts have a computable enrollment window.** The roster
supplies `vendedAt` for all 14. The scanner-side timestamp does not
exist: `firstScannedAt` is unselected, and the committed
`cloud_account.jsonl` predates the F1 correction that added
`lastScannedAt`, so not even the weaker substitute is present.

The runbook's step 4, accounts currently inside the window, is answerable
in a degraded form as "vended and not yet connected", which is the 8
accounts from B11, of which `example-corp-ml-lab` at 4 days is plausibly
still enrolling and the other 7 are not.

### C20, asset claiming and contest

For team-atlas, the dissolved team: **5 CMDB records, 1 IaC component, 2
roster accounts, 1 ticket.** Every one of those attributions rests on a
team that no longer exists, and one of them (CI-0004) is a duplicate
record contesting a live attribution to team-payments.

Attribution basis cannot be reported from the graph. The graph carries
no owner field in any current selection, so all four attribution paths
the runbook asks to distinguish (tag, account, inherited, none) come
from the external fixtures or from nowhere.

## What these fixtures do not cover

Three class B and C runbooks need external inputs that are not among the
four: **B9** (GRC control register), **B10** (change management
records), and **C19** (GRC exception register). B8 additionally needs a
cost extract, which is why its answer above is a two-way overlap rather
than the three-way the runbook asks for. Nothing here should be read as
making those attemptable.

Ten of the eleven fixture-class runbooks also remain blocked on stave's
own read surface after these fixtures exist, per
`docs/runbooks/attemptability.md`. B11 is the exception, and the reason
`vended_account.jsonl` got the most care.

## Maintenance

**Synthetic by policy.** Same rule as the graph-side fixtures
(`examples/README.md`, `.claude/rules/tenant-data-hygiene.md`). Never
regenerate any of these from a real CMDB, roster, ticket queue, or IaC
repo. Every value is invented: `example-corp-*` names, `12345678901x`
account ids, GUIDs from a zeroed range, `example-corp/*` repo paths,
handle-shaped assignees with no email. `scripts/check-tenant-leaks.sh`
passes on all four files plus this README.

**The as-of date is load-bearing.** Every age in this file is measured
from `2026-08-06T00:00:00Z`. If a fixture's timestamps change, every
figure in the answers section changes with it.

**If the graph-side fixtures change, the answers change.** Most of the
counts above are joins against `cloud_account.jsonl`,
`cloud_resource.jsonl`, and `issue.jsonl` as committed. Growing those
files is the right way to make the ratios realistic, and it invalidates
this section. Re-measure rather than adjust.

**If a curated selection grows a field, say so here.** The 0%
correspondence rows in the matrix are the fixture's record of what stave
does not select today. When `nativeId`, an artifact reference, or an
owner tag becomes reachable, the corresponding row stops being a ceiling
and becomes a real join, and the B7 and C15 answers above are no longer
correct.
