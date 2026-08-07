# v0.2 Composite Verbs, Derived From the Vendor Surface

> **STOP if you are writing the paper pipelines (bd `aae-orc-e4jo.14`).**
> This file is the control arm. Reading it before writing the pipelines
> contaminates the treatment arm and destroys the comparison the whole
> exercise is built on. You are here because this file sits next to
> `field-surface-audit.md`, which IS yours to read. That one, not this
> one.
>
> Read this only at the gate (`.15`), in the mining (`.7`), or in the
> proposal (`.8`).

**Sealed control arm for bd ticket `aae-orc-e4jo.12`.**

This document was written from the vendor surface alone: the vendored
GraphQL schema in `spec/`, stave's own curated operations and CLI, and
Wiz's public product documentation. It had no access to operator
elicitation of any kind. The author did not read `docs/runbooks/` or any
prior verb list. See the isolation attestation at the end.

**Pre-registered interpretation, quoted as fixed:**

> If the runbook-derived verb set overlaps this baseline by 80 percent or
> more (measured over verb names plus argument shapes), the elicitation
> bought nothing and charter F4 as written was correct.

**Stated limitation.** This baseline was written against the FULL
vendored schema (10,038 type definitions, 721 fields on `Query`). The
treatment arm writes against the much narrower curated field selections
in `crates/stave-api/ops/`. That asymmetry is known and is being recorded
separately as a correction to the comparison arithmetic. It is not
corrected for here.

---

## Method

I worked from four questions.

1. What does the schema expose, and which relationships are traversable
   in one query versus requiring a second root field?
2. Where does the curated registry force a multi-call sequence that the
   schema could serve in one shot, and where is the reverse true?
3. What does Wiz's public documentation say the product is for? The
   workflows it advertises are evidence about what operators do.
4. Where does a JSONL stream tool with `list` / `get` / `search` /
   `filter` / `enrich` / `emit` force a user into shell glue?

The fourth question turned out to be the sharpest, and the second
produced the finding that governs everything below.

## The governing finding: most apparent composites are field selections

The Wiz schema is deeply nested. `Issue` carries `assignee`, `projects`,
`cloudAccounts`, `serviceTickets`, `notes`, `sourceRules`, `entity`,
`entitySnapshot`, `evidenceRecords`, `remediationOptions`, `dueAt`,
`resolutionReason`, `resolvedBy`, and `url` as direct fields.
`VulnerabilityFinding` carries `hasExploit`, `hasCisaKevExploit`,
`hasFix`, `fixedVersion`, `recommendedVersion`, `epssProbability`,
`cisaKevDueDate`, `isMaliciousPackage`, `reachability`,
`validatedInRuntime`, and `vulnerableAsset`.

stave's curated documents select between five and ten fields each.

So the single largest v0.2 improvement available is widening the
documents in `crates/stave-api/ops/`, and it produces no verbs at all.
"Show me an issue with its owner and its remediation state" is not a
composite verb; it is `list_issues.graphql` with fifteen more lines. I
name this first because a verb set proposed without it will attribute
value to verbs that a document edit would have delivered.

What genuinely forces multiple calls falls into four classes:

- **Aggregation.** `*GroupedByValues` root fields (roughly forty of
  them) are separate fields from their list counterparts, and stave has
  no verb that reads them or the connection-level counters.
- **Cross-root joins the schema does not traverse.** `issueHistoryEvents`
  is a root field with its own filters; `Issue` has no `history` edge.
  Same for `issueSuggestedAssignees`. Same for the exposure family.
- **Non-connection return shapes.** The trend fields return plain lists,
  not connections, so `extract_items` in `stave-sdk/src/kinds.rs` cannot
  see them at all. The primitive layer structurally cannot carry them.
- **Name-to-id resolution.** Several analytics surfaces take an id in a
  selection input, and the only way to get that id is a prior query.

The verbs below are drawn from those four classes.

## Two primitive prerequisites, stated as prerequisites and not as verbs

Neither of these is a composite verb. Both are load-bearing for the
verbs that follow, so they are named here rather than smuggled in.

**P1. `stave get <kind> <id>`.** Currently unsupported (`kinds.rs` says
so explicitly, and charter F2 leaves it open). The schema resolves it:
`issue(id: ID!)`, `cloudResource(id: ID!)`, `vulnerabilityFinding(id:
ID!)`, `cloudAccount(id: ID!)`, `control(id: ID!)`, `securityFramework(id:
ID!)`, `report(id: ID!)`, and `project(id: ID, slug: String)` all exist as
singular root fields. Verb 5 below cannot be built without this.

**P2. Server-side filter pushdown on `list`.** Every curated document
takes only `$first` and `$after`. `IssueFilters` has 62 input fields;
`VulnerabilityFindingFilters` has around 140. `stave filter --where` runs
client-side after a full connection walk, which finding-002 already
identified as a full walk by construction. Without pushdown, several
verbs below are affordable only on small tenants.

A concrete shape consistent with `cli-philosophy.md`: typed per-kind
flags for the common filters (`--severity`, `--status`, `--project`,
`--since`, `--subscription`) plus a `--filter-by <json>` escape that is
passed to the operation's `filterBy` variable verbatim and validated
against the schema by `check-ops`.

---

## The verb set

Seven verbs, ranked by how much I believe in them. Each carries an
**identity test**: the sentence that decides whether a differently-named
proposal is the same verb.

### 1. `stave count`: server-side aggregation

```
stave count <kind>
    [--group-by <field>]        # per-kind grouping key
    [--severity <sev>...]       # server-side, folded into filterBy
    [--status <status>...]
    [--project <id>...]
    [--subscription <id>...]
    [--since <duration>]
    [--top <n>]                 # groups to return, default 25
```

**What it collapses.** Today, "how many open critical issues per
subscription" means walking the entire issue connection and counting in
`jq`. On a tenant with 100,000 issues at the 500-record page cap that is
200 requests and every record crossing the wire. The same answer is one
request.

**What it builds on.** Two mechanisms, both already in the schema.
Connection-level counters are free on a filtered query with `first: 0`:
`IssueConnection` carries `totalCount`, `uniqueEntityCount`,
`criticalSeverityCount`, `highSeverityCount`, `mediumSeverityCount`,
`lowSeverityCount`, and `informationalSeverityCount`;
`VulnerabilityFindingConnection` carries the same severity block plus
`maxCountReached`; `ControlConnection` carries `enabledCount`. Grouping
uses the `*GroupedByValues` family: `issuesGroupedByValue(groupBy:
IssuesGroupedByValueField!)` accepts `PROJECT`, `RESOURCE`,
`SUBSCRIPTION`, `SOURCE_RULE`, `CONTAINER_SERVICE`, `KUBERNETES_CLUSTER`,
`KUBERNETES_NAMESPACE`; `vulnerabilityFindingsGroupedByValues`,
`cloudResourcesGroupedByValues`, and `configurationFindingsGroupedByValues`
have their own group-by enums. Note that `IssuesGroupedByValue` exposes
only `id` and a nested `issues` connection, so the per-group count comes
from that connection's `totalCount`, selected with `first: 0`.

**Output.** One record per group:
`{_kind: "count", of: "issue", group_by: "SUBSCRIPTION", group: "<id>",
total: N, critical: N, high: N, ...}`. Ungrouped emits a single record
with `group: null`.

**Why it earns its place.** It is the largest capability gap in the tool.
stave can currently answer no question of the form "how many" without
transferring every record, and the schema offers the answer directly on
about forty surfaces. It is also the verb that most reduces tenant load,
which matters for a tool whose only development environment is
production.

**Identity test.** Any verb that returns counts or aggregates computed by
the server rather than by walking records is this verb, whatever it is
called (`agg`, `stats`, `summarize`, `group`, `tally`).

### 2. `stave graph`: run a Security Graph query

```
stave graph run --saved <name-or-id>  [--project <id>] [--limit <n>]
stave graph run --query <file|->      [--project <id>] [--limit <n>]
stave graph run --for-issue <id>      [--limit <n>]
stave graph queries [--builtin]       # list saved and built-in queries
```

**What it collapses.** `--saved <name>` resolves a human name through
`savedGraphQueries(filterBy:)` to an id, fetches the stored
`GraphEntityQueryValue`, and runs it: two calls, and today neither is
reachable from any stave verb. The `--for-issue` form passes `issueId` to
`graphSearch`, which returns the evidence set behind that issue.

**What it builds on.** `graphSearch(query: GraphEntityQueryInput,
projectId: String, issueId: ID, controlId: ID, quick: Boolean)`,
`savedGraphQueries`, `savedGraphQuery(id)`, `builtinSavedGraphQuery(id)`.
`GraphEntityQueryInput` is a recursive structure (`type`, `where`,
`relationships`, `select`, `aggregate`), which is why `--query` takes a
file rather than flags.

**Why it earns its place.** Wiz's public material puts the Security Graph
at the center of the product: it advertises attack-path computation from
internet entry points to sensitive assets, and toxic combinations as
correlated risk chains surfaced from the graph. stave cannot reach any of
it. The only current route is hand-writing a document with a nested
`GraphEntityQueryInput` literal and passing it to `stave api --query`,
which the curated posture refuses; running it requires switching the tool
to exploratory posture. That means a routine read currently costs a
posture downgrade, which is a defect in its own right.

**Safety note.** `GraphSearchResultConnection` exposes
`exportUrl(format:, limit:, type:)`. That is a field on a Query that
mints an egress artifact. The curated document must not select it, and
`check-ops` should refuse any document that does. See the standing hazard
section below.

**Identity test.** Any verb that submits a Security Graph entity query,
or runs a saved or built-in graph query by name, is this verb.

### 3. `stave coverage`: is the estate actually being scanned

```
stave coverage
    [--by account|deployment]   # default account
    [--stale <duration>]        # only rows not scanned within the window
    [--unhealthy]               # only rows with critical or high health issues
```

**What it collapses.** Three root fields, plus per-row edges: the account
inventory, the deployments feeding each account, and the system health
issues explaining why a deployment is degraded.

**What it builds on.** `cloudAccounts` gives `lastScannedAt`,
`firstScannedAt`, `resourceCount`, `containerCount`,
`virtualMachineCount`, `linkedProjects`, `sourceDeployments`, and the
five `*SystemHealthIssueCount` fields. `deployments` gives `status`,
`lastSeenAt`, `modules`, and the same health counters.
`systemHealthIssues(filterBy:)` gives `severity`, `status`, `impact`,
`remediation`, `firstSeenAt`, `lastSeenAt`, and the owning `deployment`.

**Why it earns its place.** The schema itself argues for it. Wiz
deprecated `CloudAccount.status` with the reason "status is being
deprecated, please refer to Deployments and System Health Issues
instead", and deprecated `sourceConnectors` in favor of
`sourceDeployments`. That is the vendor stating that a question which used
to be one field is now spread across several roots. This session already
took the first half of that correction into `list_cloud_accounts.graphql`
by dropping `status` for `lastScannedAt` and `resourceCount`; the verb is
the rest of it. The public documentation independently treats onboarding
coverage and visibility gaps as a named operator concern.

It is also the one verb whose value does not depend on tenant size. A
coverage gap is a small number of rows and matters immediately.

**Identity test.** Any verb that answers "which parts of my estate is Wiz
actually seeing, and how fresh is it" by combining account inventory with
scan recency or deployment health is this verb.

### 4. `stave context <kind> <id>`: one finding, with everything around it

```
stave context issue <id>
    [--with history,evidence,assignees,related]   # default history,evidence
    [--history-since <duration>]
stave context vulnerability_finding <id>
    [--with related-issues,revisions]
```

**What it collapses.** For an issue: `issue(id)` for the record,
`issueHistoryEvents(filterBy: {issue: [id]})` for the lifecycle,
`issue.evidenceRecords` for the evidence, `issueSuggestedAssignees(issueId:)`
for ownership candidates. Three root fields plus one edge. The history and
assignee roots are not reachable from `Issue` at all, so no amount of
field widening substitutes.

**What it builds on.** `issue(id: ID!)`, `issueHistoryEvents(filterBy:
IssueHistoryEventFilters)` where `IssueHistoryEventFilters.issue` is
`[String!]`, `Issue.evidenceRecords(filterBy:, orderBy:)`,
`issueSuggestedAssignees(issueId: ID!)`. `IssueHistoryEvent` carries
`type`, `timestamp`, `triggeredBy`, `message`, and `issueSnapshot`, which
together answer "who changed what, when".

**Why the name is `context` and not `triage`.** Deliberate. This
repository runs a safety-coach gate on every invocation precisely because
an agent reading imperative prose will act on it. A verb named `triage`
reads as an instruction to triage. `context` describes what the command
returns and implies nothing about acting on it. Likewise
`issueSuggestedAssignees` returns a recommendation and never performs an
assignment, and the verb's output must be shaped so that distinction
survives into the stream.

**Prerequisite.** P1 (`stave get <kind> <id>`).

**Identity test.** Any verb that takes a single finding id and returns
that finding together with its lifecycle history, evidence, or ownership
candidates drawn from other root fields is this verb.

### 5. `stave exposure <entity-id>`: reachability and blast radius

```
stave exposure <entity-id>
    [--with network,lateral,access]   # default network,lateral
    [--internet-only]                 # public-internet exposures only
    [--min-severity <sev>]
```

**What it collapses.** Three root fields, each independently scopable to
the same entity, which is what makes the join worth automating:
`networkExposures(filterBy: {exposedEntity | entityInPath})`,
`lateralMovementPaths(filterBy: {source | target})`, and
`entityEffectiveAccessEntries(filterBy: {resource | grantedEntity})`.
Answering "what can reach this thing, and what can it reach" by hand is
three documents and three cursor walks.

**What it builds on.** `NetworkExposure` carries `exposedEntity`,
`accessibleFrom`, `path`, `portRange`, `networkProtocols`, `type`,
`sourceIpRange`, `destinationIpRange`. `LateralMovementPath` carries
`severity`, `pathEntities`, `sourceParent`, `targetParent`,
`isFromPublicAccess`, `isCrossCloud`, `isCrossSubscription`.
`EntityEffectiveAccessFilters` exposes privilege and excessive-access
predicates directly.

**Why it earns its place, and why it is not the same as `graph`.** These
are precomputed products, not graph queries. `graphSearch` will not
return a `LateralMovementPath`; the path analysis is its own surface with
its own severity model. Wiz's public material puts attack-path ranking by
exploitability and blast radius at the front of the product, so this is a
first-class question rather than a niche one.

**Weakest point.** Of the seven, this is the verb whose field selections
I am least confident about, because none of these types have been
exercised against the tenant and F1's scope qualifier applies with full
force. The three-root shape is solid; the selections are guesses.

**Identity test.** Any verb that takes one resource or identity and
returns its network exposure, lateral movement paths, or effective access
from the dedicated exposure roots is this verb.

### 6. `stave posture`: compliance scoring for a framework

```
stave posture
    [--framework <name-or-id>...]     # default: all enabled frameworks
    [--by framework|project|account]  # default framework
    [--categories]                    # per-category breakdown
    [--failing-only]
```

**What it collapses.** Name-to-id resolution plus a fan across roots.
`--framework "CIS AWS"` resolves through `securityFrameworks(filterBy:)`
to an id, then that id is required as a string inside
`ProjectComplianceAnalyticsSelection { framework: String! }` or
`CloudAccountComplianceAnalyticsSelection { framework: String! }` to get
the per-project or per-account view. Those are different root fields
(`projectsWithComplianceAnalytics`, `cloudAccountsWithComplianceAnalytics`)
from the framework list.

**What it builds on.** `securityFrameworks`,
`SecurityFramework.complianceAnalytics(selection:)` returning
`weightedScore`, `averageCompliancePosture`, `passCount`, `failCount`,
`passSubCategoryCount`, `failSubCategoryCount`, `categoryAnalytics`,
`emptyPostureReason`, `scoreUpdatedAt`; `SecurityFramework.controls`;
`projectsWithComplianceAnalytics`; `cloudAccountsWithComplianceAnalytics`;
`policyComplianceAnalytics(policyId: String!)`.

**Why it earns its place.** Wiz advertises continuous assessment against
more than a hundred frameworks and auditor reporting as headline
capabilities. stave's `list security_framework` currently returns id,
name, description, and enabled: the framework roster with no posture in
it. Every compliance question therefore requires ad-hoc GraphQL today.

**Honest weight.** This is thinner than verbs 1 through 5. Its collapse is
one resolution step plus one alternate root, not a deep sequence. It is
here because the capability is entirely absent and the schema serves it
cleanly, not because the call collapse is large.

**Identity test.** Any verb that returns a compliance score, pass/fail
counts, or category breakdown for a named framework is this verb.

### 7. `stave trend <kind>`: time series

```
stave trend issue
    [--from <date>] [--to <date>]       # default: last 30 days
    [--interval day|week|month]         # default day
    [--project <id>...] [--severity <sev>...]
stave trend compliance --framework <name-or-id> [--by project|account]
```

**What it collapses.** Nothing. It earns its place on shape, not on call
count, and I would rather say so than dress it up.

**What it builds on.** `issuesTrendV2(startDate: DateTime!, endDate:
DateTime!, interval: TimeInterval, intervalType:, filterBy:, type:)`,
`auditLogEntriesTrend`, `issueHistoryEventTrend`,
`Project.issuesTrend`, `SecurityFramework.complianceTrend`,
`CloudAccount.complianceTrend`.

**Why it needs a verb rather than a document.** `issuesTrendV2` returns
`[IssuesTrendDataSeriesV2!]!`, a plain list, not a connection. The stream
primitives are built on connections: `extract_items` looks for a `nodes`
array under the root field, `every_document_paginates` in `stave-api`
asserts that every registered operation carries `$first`, `$after`,
`pageInfo`, `endCursor`, and `hasNextPage`, and `stave list` drives a
cursor loop. A trend document fails that test by construction. It also
requires two mandatory date arguments, which no existing verb supplies.
So the primitive layer cannot absorb this; it needs its own path.

**Why it is last.** It answers "are we improving", which every operator
report needs and which stave cannot express at all today. But it is one
call, and if the ranking has to be cut anywhere, it is here.

**Identity test.** Any verb that returns a time series over a date range
rather than a current-state record set is this verb.

---

## Considered and rejected

Recorded because a differently-named proposal may match something here
rather than something above, and a rejection with a reason is more useful
to the comparison than silence.

**`stave exceptions` (suppression audit).** "What findings are being
suppressed, by which rule, expiring when, approved by whom" is a real
operator question, and `IgnoreRule` answers nearly all of it in one root
with a wide selection: `enabled`, `expiredAt`, `createdBy`, `createdAt`,
`findingTypes`, `findingIgnoreReason`, `targets`, `analytics`, `project`,
plus per-finding-type rule lists. Rejected as a verb because it is a
curated document plus a kind-table entry, which is exactly the Category A
mistake this document opens with. It should ship as
`list_ignore_rules.graphql` and a `stave list ignore_rule`.

**`stave report fetch`.** `reportRuns` and `ReportRun` expose `status`,
`progress`, `runAt`, `results`, and `url`. Rejected on hazard grounds:
`ReportRun.url` is a pre-signed download link, so a verb that surfaces it
turns a read into an egress channel and puts a credential-bearing URL in
a JSONL stream that the tenant-data-hygiene rule forbids publishing.
Triggering a run is a mutation and fenced regardless. If this is wanted
later, it needs its own design pass on the redaction path, not a verb
slot here.

**`stave inventory`.** A resource-inventory verb was considered and
collapsed into verbs 1 and 3. Grouped resource counts are `stave count
cloud_resource --group-by ...`; "what is Wiz seeing" is `stave coverage`.
A third verb would overlap both.

**`stave scopes` / `stave permissions --live`.** `permissionScopes(filterBy:
PermissionScopeFilters)` exists as a root field and is a plausible route
to the scope-qualification problem left open in finding-001, where the
token did not expose readable granted scopes. Rejected as a v0.2
composite because it belongs to the existing `auth` family
(`auth scopes`, `auth can-i`, `auth plan`), not to a new verb. Worth
filing separately.

---

## A standing hazard the verb designs must respect

Four Query fields mint egress artifacts as a side effect of being
selected:

- `IssueConnection.exportUrl(format:, includeThreatDetails:, limit:)`
- `ControlConnection.exportUrl(format:, issueAnalyticsSelection:, limit:)`
- `GraphSearchResultConnection.exportUrl(entityOptions:, format:, limit:, type:)`
- `ReportRun.url`

Selecting a field on a Query is normally inert. These are not. The write
guard classifies by operation type and will pass every one of them,
because they are queries. Any curated document backing the verbs above
must exclude them, and `check-ops` is the right place to enforce that:
a deny-list of field paths that no curated document may select, failing
the build rather than warning. This is a schema-derived observation, not
an operator report, and it applies to whatever verb set is finally
chosen.

---

## Isolation attestation

**Inputs consulted, exhaustively.**

Files in this repository:

- `spec/wiz-schema.graphql` and `spec/README.md`. I read the full `Query`
  root type (721 lines), the `Mutation` root only as a line count, and
  targeted extracts for: `Issue`, `VulnerabilityFinding`, `CloudAccount`,
  `SecurityFramework`, `Project`, `Deployment`, `SystemHealthIssue`,
  `IgnoreRule`, `IssueHistoryEvent`, `ReportRun`, `SavedGraphQuery`,
  `NetworkExposure`, `LateralMovementPath`, the `IssueConnection` /
  `CloudResourceConnection` / `VulnerabilityFindingConnection` /
  `ControlConnection` / `IssuesGroupedByValue*` /
  `GraphSearchResultConnection` connection shapes, the input types
  `IssueFilters`, `VulnerabilityFindingFilters`, `CloudResourceFilters`,
  `ControlFilters`, `IssueHistoryEventFilters`, `NetworkExposureFilters`,
  `LateralMovementPathFilters`, `EntityEffectiveAccessFilters`,
  `IssueDateFilter`, `IssueOrder`, `GraphEntityQueryInput`,
  `CloudResourceGroupBy`, the compliance selection inputs, and the enums
  `Severity`, `IssueStatus`, `IssueType`, `IssueOrderField`,
  `VulnerabilitySeverity`, `FindingCommonStatus`,
  `IssuesGroupedByValueField`.
- `crates/stave-api/src/lib.rs` and all twelve documents in
  `crates/stave-api/ops/`.
- `crates/stave-sdk/src/kinds.rs`, `stream.rs`, `enrich.rs`, and the
  header of `cel.rs`.
- `crates/stave-cli/src/main.rs`: the header, the full clap command and
  argument structure, the paging constants, and the emit formats.
- `charter.md` and `CLAUDE.md`.
- `.claude/rules/cli-philosophy.md`, `tenant-data-hygiene.md`, and
  `safety-coach-gate.md`, which reached me through the project
  instructions rather than by my opening them.

Public web sources:

- https://www.wiz.io/platform/wiz-cloud (fetched)
- Search result summaries for Wiz platform overview, Security Graph and
  attack paths, GraphQL API usage, ownership and SLA handling, and
  deployment coverage. The pages surfaced included wiz.io product and
  blog pages, cloud.google.com partner architecture documentation,
  docs.datadoghq.com, docs.qualys.com, and several third-party
  integration and commentary sites.

**Confirmation.** I did not read `docs/runbooks/` or anything inside it,
by any tool. I listed `docs/` once at the top level, which showed the
directory name `runbooks` and nothing about its contents, and I did not
descend into it. I did not read
`docs/design/verb-candidate-registration.md`; I confirmed by directory
listing that no such file exists. I did not run the `stave` binary. I did
not search the repository for the prohibited terms.

Two incidental filename exposures, disclosed for completeness because
neither is content. After writing this document I ran `git status`, which
showed a modified `docs/runbooks/catalogue.md` and an untracked
`docs/runbooks/catalogue-provenance.md`. I learned that those two files
exist and nothing else about them; I did not open either. The same output
showed an untracked `docs/design/field-surface-audit.md`, a sibling in
the directory I wrote to, which I also did not open. All three appear to
be another agent's work in progress in this checkout.

**Compromises to isolation.** None that I am aware of. Two things worth
recording so the reader can judge rather than take my word for it.

First, the project instructions loaded into my context automatically
include this repository's `charter.md`, `CLAUDE.md`, and the rules under
`.claude/rules/`. `charter.md` was on the permitted list. The rules files
were not explicitly listed, but they arrived before I began and I used
`cli-philosophy.md` and `safety-coach-gate.md` in the reasoning above,
visibly. The charter's session log contains one sentence mentioning that
a runbook catalogue exists and is tied to the F4 verb-mining exercise. It
names no runbook and no verb. That sentence told me the treatment arm
exists, which I already knew from the brief, and nothing about its
content.

Second, `charter.md` F2 and F4 name the open questions in the vendor's
own terms (get-by-id, server-side filters, `graphSearch` as a first-class
verb, and `issue-triage` / `vuln-exposure` / `posture-report` as
placeholder composite names). That is prior art from this repository, and
it plainly anchored parts of this proposal: verbs 2, 4, 5, and 6 sit
close to those placeholders. `charter.md` was an explicitly permitted
input, so this is disclosed rather than treated as contamination, but a
scorer should know that the baseline is not independent of the charter
and should discount agreement with F4's three placeholder names
accordingly.
