# Field surface audit: what the runbooks need against what stave asks for

bd `aae-orc-e4jo.16` (S0e). Written 2026-08-07, before the paper
pipelines (`.14`) and before the comparison gate (`.15`).

**Method.** Offline diff between the vendored schema `spec/wiz-schema.graphql`
(sha256-pinned, 98,064 lines) and the twelve curated documents in
`crates/stave-api/ops/`, mapped onto the twenty runbooks in
`docs/runbooks/catalogue.md`. No tenant contact, no API budget, no stave
invocation.

**Why it runs before either arm.** Mining "what was hard" measures the
scaffold's caution unless the scaffold's caution is separately accounted
for. A verb proposed because a field was unselected is a proposal to
build the wrong thing.

---

## Headline

The audit was commissioned on the premise that stave's field selections
are conservative. They are, but that is the smallest of three distinct
gaps, and the two larger ones are invisible from inside the selection
frame:

| # | Class | What it is | Fixable by widening a selection? |
|---|---|---|---|
| 1 | **Selection width** | The bound root returns a type carrying the field; the document does not ask for it. | Yes |
| 2 | **Root binding** | The bound root returns a thin type when a richer sibling root exists. `cloudResources` returns `CloudResource` (8 fields); `cloudResourcesV2` returns `CloudResourceV2` (55). | No. Requires binding a different root field. |
| 3 | **Unbound capability** | Whole families of root fields stave does not reach at all: server-side filtering, aggregation, history, diff. | No. Requires new operations. |

Class 2 alone caps eight runbooks. Class 3 bears directly on the verb
exercise and is treated separately below, because a verb proposed as
client-side glue when the server already exposes a root field for it is
the exact failure mode this audit exists to prevent.

**Almost nothing in the catalogue is blocked by Wiz.** Of the runbook
steps that cannot be attempted today, the internal-facing ones are
blocked by stave in every case examined. The genuinely unavailable
material is the external input in class B, which is unavailable by
category and not by vendor limitation.

---

## Per-kind surface

Selected counts are fields named in the curated document, excluding
`pageInfo`. "Notable unselected" lists only fields a catalogue runbook
actually asks for.

| Kind | Root bound | Type | Fields | Selected | Notable unselected |
|---|---|---|---|---|---|
| issue | `issuesV2` | `Issue` | 56 | 9 | `assignee`, `projects`, `project`, `serviceTickets`, `serviceTicket`, `control`, `sourceRule(s)`, `resolutionReason`, `resolvedBy`, `statusChangedAt`, `reopenedAt`, `openReason`, `rejectionExpiredAt`, `notes`, `cloudAccounts`, `entity`, `url` |
| vulnerability_finding | `vulnerabilityFindings` | `VulnerabilityFinding` | 125 | 6 | `severity`, `vulnerableAsset`, `projects`, `hasFix`, `fixedVersion`, `hasExploit`, `hasCisaKevExploit`, `cisaKevDueDate`, `epssProbability`, `resolutionReason`, `layerMetadata`, `rootComponent`, `detailedName`, `version`, `locationPath`, `ignoreRules`, `vcsCodeOwners`, `sourceMappedCodeFindings`, `revisions` |
| cloud_resource | `cloudResources` | `CloudResource` | **8** | 6 | `graphEntity`, `subscriptionId`. **The type is nearly exhausted. See class 2.** |
| cloud_account | `cloudAccounts` | `CloudAccount` | 24 | 6 | `firstScannedAt`, `status`, `connector`, `sourceConnectors`, `sourceDeployments`, `linkedProjects`, `connectorIssues` |
| project | `projects` | `Project` | 60 | 5 | `projectOwners`, `securityChampions`, `businessUnit`, `tags`, `identifiers`, `riskProfile`, `cloudAccountLinks`, `resourceTagLinks` |
| control | `controls` | `Control` | 46 | 4 | `lastRunAt`, `lastSuccessfulRunAt`, `lastRunError`, `serviceTickets`, `securitySubCategories`, `scopeQuery`, `query`, `resolutionRecommendation`, `impactSeverity` |
| service_account | `serviceAccounts` | `ServiceAccount` | 22 | 4 | `lastLoginAt`, `lastRotatedAt`, `expiresAt`, `enabled`, `scopes`, `assignedProjects`, `createdBy` |
| user | `users` | `User` | 41 | 3 | `lastLoginAt`, `enabled`, `isSuspended`, `role`, `effectiveAssignedProjects`, `identityProviderType` |
| audit_log_entry | `auditLogEntries` | `AuditLogEntry` | 17 | 4 | `performer`, `user`, `serviceAccount`, `actionType`, `actionParameters`, `sourceIP`, `requestId`, `userAgent` |
| security_framework | `securityFrameworks` | `SecurityFramework` | 29 | 4 | `controls`, `cloudConfigurationRules`, `categories`, `certification`, `version`, `nextAuditDate`, `owner` |
| cloud_configuration_rule | `cloudConfigurationRules` | `CloudConfigurationRule` | 45 | 5 | `control`, `securitySubCategories`, `scopeAccounts`, `remediationInstructions`, `findings`, `shortId` |
| report | `reports` | `Report` | 21 | 2 | `lastRun`, `lastSuccessfulRun`, `nextRunAt`, `type`, `format`, `project` |

Two selections are worth calling out on their own:

- **`vulnerabilityFindings` selects `vendorSeverity` but not `severity`.**
  `severity: VulnerabilitySeverity!` is the non-null canonical field;
  `vendorSeverity` is nullable and vendor-reported. Any severity-ranked
  runbook currently ranks on the weaker field, and can rank on nothing at
  all where the vendor did not supply one.
- **`ServiceAccount.clientSecret` is a selectable `String!` on a type
  stave already queries.** Not a gap to close. Recorded here so the next
  person widening this selection sees the hazard in the same table as the
  fields they want. The safety coach already refuses documents selecting
  it.

---

## Class 2: the `cloudResources` binding

`CloudResource` has eight fields. stave selects six of them. Widening
this selection can add `graphEntity` and `subscriptionId` and then it is
finished. Nothing else exists to select.

`cloudResourcesV2(after, filterBy, first, orderBy)` returns
`CloudResourceV2`, which carries 55 fields, including precisely the ones
the catalogue turns on:

| Field on `CloudResourceV2` | Runbook that needs it |
|---|---|
| `isAccessibleFromInternet`, `isOpenToAllInternet` | A2 step 3, A3 step 2 |
| `hasSensitiveData`, `hasAccessToSensitiveData` | A3 step 3 |
| `owners`, `projects` | A1 step 4, A2 step 4, B8, C20 |
| `tags` | B7 step 3, B8 step 1, C20 steps 2 and 3 |
| `externalId`, `providerUniqueId`, `region` | B6, B7 |
| `iacDetails`, `iacDeployment`, `iacModuleSource`, `codeRepository` | C15 |
| `deletedAt`, `lastSeen`, `status` | B13, C18 step 2 |
| `issueAnalytics`, `vulnerabilityAnalytics` | A3 step 4 ranking |

The catalogue's first collision claims the IaC address is the best
available join key. That claim is computable, but only through
`cloudResourcesV2`. Under the current binding it is not merely hard, it
is unreachable, and an executor working from the curated surface would
conclude the tool cannot express it. That conclusion would be about
stave, not about Wiz, and without this audit the two would be
indistinguishable afterwards.

This is the single highest-value correction available to stave's read
surface, and it is one bound root field.

---

## Class 3: unbound capability, and a correction to charter F2

### Server-side filtering exists

Charter F2 records as an open question "whether list operations grow
`filterBy` variables (server-side filtering vs the current client-side
`stave filter`)", and the 2026-08-06 evidence line states that `search`
and `--since` "are full-connection walks by construction" and that "only
a server-side filter" removes the walk.

The walk is real. The premise that Wiz offers no server-side filter is
not. Verified in the vendored schema:

```
issuesV2(after: String, filterBy: IssueFilters, filterScope: IssueFiltersScope,
         first: Int, orderBy: IssueOrder): IssueConnection!
cloudResourcesV2(after: String, filterBy: CloudResourceV2Filters,
                 first: Int, orderBy: CloudResourceOrder): CloudResourceV2Connection!
```

`IssueFilters` has 60 input fields, among them `status`, `severity`,
`createdAt`, `dueAt`, `resolvedAt`, `statusChangedAt`, `assignee`,
`project`, `resolutionReason`, `openReason`, `rejectionExpiresAt`,
`search`, and `hasServiceTicket`.

The curated documents declare `$first` and `$after` and nothing else. So
the full-connection walk is a property of stave's documents, not of the
Wiz API. Three consequences:

1. **F2 needs correcting.** The server-side filter is not a thing to
   wish for. It is available and unused.
2. **The safety coach's check 4 keeps its verdict and loses its
   reason.** The unconditional HALT on `search` and on `list --since`
   remains correct, because today's documents genuinely do walk the
   connection. The stated cause ("charter F2: there are no server-side
   filter variables yet") is wrong and should be restated as "stave's
   curated documents do not declare the filter variables the schema
   offers." The behavior does not change until the documents do.
3. **`hasServiceTicket` is B12 step 1 as a server-side boolean**, and
   `assignee` is A1 step 4. Two of the catalogue's named walls are
   filter arguments stave does not pass.

### Aggregation, history, and diff

Root fields stave binds nothing to:

| Root field | Shape | Bears on |
|---|---|---|
| `issuesGroupedByValue(groupBy:, filterBy:, orderBy:)` | server-side group and count | A1 step 2, C14 step 3 |
| `cloudResourcesGroupedByValues`, `cloudAccountsGroupedByValue`, `vulnerabilityFindingsGroupedByValues` | same shape, other kinds | A2 step 2, C14 |
| `vulnerabilityFindingsGroupedByLayer(resourceId:, resourceType:)` | findings grouped by image layer | C14 step 1, directly |
| `issuesTrend`, `issuesTrendV2`, `auditLogEntriesTrend` | time series | C17 |
| `issueHistoryEvents(filterBy:, orderBy:)`, `cloudResourceRevisions`, `VulnerabilityFinding.revisions` | change over time | B10 step 4, C17, C18 |
| `securityFrameworksDiff(baseFrameworkId:, targetFrameworkIds:)` | server-side diff | B9 |
| `issueSuggestedAssignees(issueId:)` | ownership routing | A1 step 4, A2 step 4 |
| `graphSearch(query:, controlId:, issueId:, projectId:)` | the graph query surface | A2, A3, B6 |
| `cloudAccountsWithComplianceAnalytics`, `projectsWithComplianceAnalytics` | joined compliance rollup | A5, B9 |

**This is the finding that most affects the verb exercise.** Three of
the five registered priors have a server-side analogue here. "Roll-up"
is `*GroupedByValue`. "Diff" is `securityFrameworksDiff`, at least for
frameworks. Trend and history answer C17, which the catalogue calls a
test of whether the tool can express change at all.

That does not retire those verb candidates. A stave verb that fans out
across kinds and emits one stream is not the same object as a single
server root field, and the client-side verb may still earn its place.
But a proposal that presents client-side grouping as the only way to
group has misread the surface, and `.14` and `.15` need this table in
front of them to avoid it.

---

## Runbook step classification

Acceptance criterion for this ticket. Each step currently believed
unattemptable, labelled.

**Deviation from the ticket, stated rather than buried.** The ticket
asks for two labels, "Wiz API cannot" and "our selection does not". Two
is not enough and the missing one matters: the external inputs in class
B are neither. They are outside any cloud security tool by category, not
absent from this vendor's API. Forcing them into "Wiz API cannot" would
read as a vendor defect and would inflate the count of things the API
fails to do. Three labels, with the third named honestly:

- **OURS/selection**: the field exists on the bound type, unselected.
- **OURS/binding**: needs `cloudResourcesV2` or another unbound root.
- **EXTERNAL**: requires an input no security graph holds. Not a defect.
- **WIZ**: the API genuinely cannot.

| Step | Blocker | Label |
|---|---|---|
| A1.2 group by severity and status | client-side today; `issuesGroupedByValue` unbound | OURS/binding |
| A1.4 attribute to an owner | `Issue.assignee` unselected; `issueSuggestedAssignees` unbound | OURS/selection |
| A2.1 does the CVE exist | `vulnerabilityExternalId` unselected | OURS/selection |
| A2.2 enumerate where | `vulnerableAsset`, `projects` unselected | OURS/selection |
| A2.3 internet-reachable | `isAccessibleFromInternet` is V2-only | OURS/binding |
| A2.4 who to wake | as A1.4, plus `owners` V2-only | OURS/selection + binding |
| A3.2 internet-exposed | as A2.3 | OURS/binding |
| A3.3 holding sensitive data | `hasSensitiveData` is V2-only | OURS/binding |
| A3.4 rank survivors | `severity` unselected on findings; analytics V2-only | OURS/selection + binding |
| A4.1 accounts with creation dates | `createdAt` selected | attemptable |
| A4.2 bucket by age | attemptable | attemptable |
| A4.3 cross-reference audit log | `AuditLogEntry.serviceAccount`/`user`/`performer` unselected | OURS/selection |
| A4.4 flag no activity | `ServiceAccount.lastLoginAt`, `lastRotatedAt`, `enabled` unselected | OURS/selection |
| A5.2 controls for a framework | `SecurityFramework.controls` unselected | OURS/selection |
| A5.3 coverage by severity | attemptable once A5.2 lands | OURS/selection |
| B6.1 candidate keys per system | graph side needs `externalId`, `tags`, `region`, all V2-only | OURS/binding |
| B6.2 key population fraction | as B6.1 | OURS/binding |
| B7.1 to B7.3 all three buckets | graph side V2-only; CMDB extract external | OURS/binding + EXTERNAL |
| B8.1 no owner tag | `tags`, `owners` V2-only | OURS/binding |
| B8.3 carrying cost | cost data | EXTERNAL |
| B9.1 asserted controls | GRC register | EXTERNAL |
| B9.2 actual enablement | `enabled` selected | attemptable |
| B9.3 diff both directions | `securityFrameworksDiff` unbound; controls need `lastSuccessfulRunAt` | OURS/binding + selection |
| B10.1 changes from audit log | `actionType`, `actionParameters`, `performer` unselected | OURS/selection |
| B10.2 join to approved changes | change management records | EXTERNAL |
| B10.4 controls disabled for an interval | `issueHistoryEvents` and trend roots unbound | OURS/binding |
| B11.1 authoritative roster | vended-account roster | EXTERNAL |
| B11.2 accounts the scanner connected | selected and sufficient | attemptable |
| B11.4 age of each unscanned account | `firstScannedAt`, `status` unselected | OURS/selection |
| B12.1 issues with no ticket | `serviceTickets` unselected; `hasServiceTicket` filter unused | OURS/selection |
| B12.2 closed tickets, open issue | ticketing export | EXTERNAL |
| B13.1 retired records | CMDB extract | EXTERNAL |
| B13.2 does the cloud still report it | `deletedAt`, `lastSeen`, `status` V2-only | OURS/binding |
| B13.4 accounts closed without telling the scanner | `CloudAccount.status`, `connector` unselected | OURS/selection |
| C14.1 group by root cause | `layerMetadata`, `rootComponent` unselected; `vulnerabilityFindingsGroupedByLayer` unbound | OURS/selection + binding |
| C14.3 rank causes | `severity` unselected | OURS/selection |
| C15.1 map to producing artifact | `iacDetails`, `iacModuleSource` V2-only | OURS/binding |
| C15.2 walk back to repo and commit | `codeRepository` V2-only; `vcsCodeOwners` unselected | OURS/binding + selection |
| C16.1 vend timestamp | vending pipeline | EXTERNAL |
| C16.2 scanner connection timestamp | `firstScannedAt` unselected | OURS/selection |
| C17.1 stable signature | derivable client-side from fields not yet selected | OURS/selection |
| C17.2 closed and reappeared | `revisions`, `issueHistoryEvents` unbound; `reopenedAt` unselected | OURS/binding + selection |
| C18.2 does the entity still exist | `deletedAt`, `lastSeen` V2-only | OURS/binding |
| C18.3 remediated versus evaporated | `resolutionReason`, `resolvedBy` unselected | OURS/selection |
| C19.1 issues with a filed exception | GRC register external; `openReason`, `rejectionExpiredAt` unselected | OURS/selection + EXTERNAL |
| C19.3 exceptions past expiry | `rejectionExpiredAt` unselected; `ignoreRules` unselected on findings | OURS/selection |
| C19.4 reopened despite acceptance | `reopenedAt`, `statusChangedAt` unselected | OURS/selection |
| C20.1 what the graph attributes to a team | `owners`, `projects`, `tags` V2-only | OURS/binding |
| C20.2 attribution basis | as C20.1 | OURS/binding |

**Count: zero steps labelled WIZ.** Every internal-facing block traces
to a stave selection or a stave binding. Fifteen step-halves are
EXTERNAL, which is the class B thesis holding up exactly as the
catalogue predicted.

That is the headline for `.14` and `.15`: when the paper pipelines
report that a runbook cannot be served, the default explanation is
stave's read surface, and the burden is on the claim that it is Wiz.

---

## Ruling on the control and treatment surface asymmetry

The problem, restated. The sealed baseline (`.12`) is written from the
full vendored schema. The paper pipelines (`.14`) are written against
the curated registry. The baseline therefore sees capabilities the
runbook arm cannot reach, which inflates apparent baseline coverage and
biases the `.15` comparison toward outcome 1 for a reason that has
nothing to do with elicitation.

The ticket offers two remedies: widen the selections so both arms see
the same surface, or state the asymmetry as a correction in the gate's
arithmetic.

**Ruling: correction, not widening.** Not a preference. Widening any
selection requires live validation against the tenant (charter B2, F1),
phase 0 has no tenant contact by design, and a widened-but-unvalidated
document would put a third state into the comparison worse than either
arm. The widening branch cannot land in this phase. Widening tickets are
filed instead and the surface stays as it is for both arms.

**The correction `.15` applies.** At the gate, every baseline verb is
tagged with the surface its justification rests on:

- **reachable**: justified by fields and roots the curated documents
  already select.
- **surface-advantaged**: justified only by fields or roots the
  treatment arm cannot reach today, meaning `CloudResourceV2`, the
  `filterBy` and `orderBy` arguments, the aggregation, history, and diff
  roots, or any unselected field in the table above.

Overlap is then computed twice, and both numbers are reported:

- **raw overlap**, over the whole baseline, which is the biased figure.
- **corrected overlap**, with surface-advantaged baseline verbs
  excluded from the denominator.

The pre-registered 80 percent threshold is evaluated against the
**corrected** figure. The raw figure is reported beside it so the size of
the correction is visible rather than asserted. If the two land on
opposite sides of the threshold, that fact is the finding and neither
number is quietly preferred.

`.12` has been instructed to note that it wrote against the full schema
and not to correct for the asymmetry itself, so the tagging is done at
the gate by someone who can see both arms.

### A defect in the pre-registration, surfaced before the numbers

The `.12` pre-registration reads "if the runbook-derived verb set
overlaps this baseline by 80 percent or more". Overlap of what over
what is unstated, and the two readings differ:

- `|intersection| / |baseline|`: "the runbook arm rediscovered the
  baseline". A runbook arm proposing many novel verbs still scores high
  if it happens to cover the baseline.
- `|intersection| / |runbook set|`: "the runbook arm found nothing the
  baseline did not". This is the reading that matches the stated
  conclusion, that the elicitation bought nothing.

The second is the one the pre-registration means, since the claim under
test is that elicitation adds something. Recording it here, before
either arm's output exists, so it is fixed rather than chosen once the
numbers are visible. If the ticket author intended the first reading,
that is a correction to make now and not at the gate.

---

## Rulings per gap

Each gap gets one of: widen now, leave and label, or file as capability
work.

| Gap | Ruling |
|---|---|
| Unselected fields on already-bound types | **Leave and label.** Widening needs live validation. Filed as a ticket. The affected runbook steps are labelled OURS/selection above and `.14` treats them as reachable-in-principle, not as tool limits. |
| `cloudResources` versus `cloudResourcesV2` | **File as capability work, high priority.** Largest single correction available. Cannot be done offline. |
| `filterBy` and `orderBy` unused | **File as capability work.** Also correct charter F2 and the safety coach's check-4 rationale, both of which state the filters do not exist. |
| Aggregation, history, diff roots | **File as capability work, and hand this table to `.14` and `.15`.** These bear on verb candidates directly. |
| `vendorSeverity` selected instead of `severity` | **File as a defect.** Distinct from the widening work; the currently selected field is the weaker of the two. |

None of these block `.14`. All of them change how `.14`'s output is read.

---

## Follow-ups filed

See the bd tickets linked from `aae-orc-e4jo.16`.
