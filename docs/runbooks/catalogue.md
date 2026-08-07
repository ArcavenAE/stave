# Operator Runbook Catalogue

Twenty runbooks used by enterprise cloud vulnerability management and IT
operations teams working with cloud security graph inventory data.
Elicited 2026-08-06 from a cast of role-appropriate personas, deliberately
widened past the vendor's own product framing to include the
reconciliation and inventory cross-checks that a security tool is not
designed to serve but is constantly used for.

This catalogue is an input artifact, not a specification of stave. It
describes what operators are trying to accomplish. Whether stave can
accomplish it is the question the exercise answers.

Tracked as bd `aae-orc-e4jo`. Framing and known weaknesses of the method:
`aae-orc/_kos/nodes/frontier/question-runbook-derived-verb-bootstrap.yaml`.

> **This file is the executor's copy.** The party's reasoning about how
> these runbooks should be answered, which analytical patterns recur, and
> which persona contributed each one, is held separately in
> `catalogue-provenance.md` and is deliberately not repeated here. That
> file is read by the judges and by the analysis, never by the executor
> or the pipeline author. See bd `aae-orc-e4jo.1`.

---

## How to read this

**Three classes:**

| Class | Definition | Count |
|---|---|---|
| **A. Queries** | Answerable from the security graph alone | 5 |
| **B. Joins** | Require a mandatory external input | 8 |
| **C. Spoke-team** | Run by the teams receiving the tickets, not the team sending them | 7 |

### The simulation rule

Several runbooks contain steps phrased as actions: assign, clear,
resolve, comment, classify, accept. **Every one is simulated and never
performed.** The tenant is a live production environment for an
information security team. "Assign the issue" means produce what the
assignment would be. It never means calling something that assigns it.

Steps carrying this risk are marked **[SIMULATE]**.

Every stave invocation goes through the `stave-safety-coach` subagent
before it runs (`.claude/rules/safety-coach-gate.md`). Every read that
reaches a durable artifact goes through `scripts/scrub.sh`
(`.claude/rules/tenant-data-hygiene.md`).

### Known constraint on reading these

A field-surface audit (`docs/design/field-surface-audit.md`, bd
`aae-orc-e4jo.16`) established that where a runbook step cannot be
served today, the cause is stave's own read surface in every internal
case examined, and not the Wiz API. When recording that a step could not
be attempted, the default explanation is stave's field selections or its
bound root fields. The burden is on the claim that the vendor cannot.

---

## Class A: Queries

Answerable from the security graph alone.

### A1. Remediation SLA sweep
**Objective:** It is Tuesday, the monthly review is Thursday. What is
open, what is past SLA, and who am I chasing.
**Inputs:** graph only (issues)
**Steps:**
1. Pull open issues.
2. Group by severity and status.
3. Compute age from creation; flag those past the severity's SLA window.
4. Attribute each to an owner.

**Output:** a dated table, severity by status by age band, with an owner
column and a past-SLA count.
**Success criterion:** the operator can walk into the monthly with it and
answer "who is chasing what" without opening another system.

### A2. Emergency blast radius
**Objective:** Something drops, the CISO asks "are we exposed", and the
answer is needed in twenty minutes.
**Inputs:** graph only (vulnerability findings, cloud resources, issues)
**Steps:**
1. Given a CVE or a vulnerable package, determine whether it exists in
   the estate.
2. Enumerate where, by account and resource type.
3. Determine which of those are internet-reachable.
4. Identify who to wake up.

**Output:** a count, a breakdown by exposure, and a contact list.
**Success criterion:** answerable inside twenty minutes.

### A3. Toxic combination triage
**Objective:** Not every critical is urgent. Find the ones that are
genuinely dangerous: exposed, critical, and sitting on something that
matters.
**Inputs:** graph only (issues, cloud resources)
**Steps:**
1. Pull issues of critical and high severity.
2. Narrow to internet-exposed entities.
3. Narrow again to entities holding sensitive data.
4. Rank the survivors.

**Output:** a short ranked list, small enough to act on this week.
**Success criterion:** the list is short enough that the operator works
all of it, and believes the ranking.

### A4. Standing credential review
**Objective:** Every service account is a key under a doormat. Which ones
exist, how old are they, and does anyone remember why.
**Inputs:** graph only (service accounts, users, audit log)
**Steps:**
1. Enumerate service accounts with creation dates.
2. Bucket by age.
3. Cross-reference the audit log for recent activity per account.
4. Flag accounts with no activity in the window. **[SIMULATE]** any
   suggestion to disable one.

**Output:** an inventory with age and last-activity, and a candidate
list for review.
**Success criterion:** the operator can take the candidate list to the
owning teams and ask "do you still need this".

### A5. Framework evidence pull
**Objective:** An auditor asks what is implemented. A list is not
evidence; a dated, reproducible derivation is.
**Inputs:** graph only (controls, security frameworks, cloud config rules)
**Steps:**
1. Enumerate frameworks and their enablement.
2. For the framework in scope, enumerate its controls and enablement.
3. Produce coverage by severity.
4. Stamp the artifact with a date and the exact derivation.

**Output:** a dated artifact stating as-of time, method, and result.
**Success criterion:** it survives three questions: as of when, how
derived, and show me last quarter's.

---

## Class B: Joins

Each requires an external input the security graph does not contain.

### B6. Join key coverage
**Objective:** A precondition for the rest of this class. Every
cross-check depends on a key that both systems carry, so measure the keys
before attempting anything that depends on them.
**Inputs:** graph, plus every external system in scope
**Steps:**
1. For each system, identify the candidate keys it carries.
2. Measure the fraction of records populating each key.
3. Identify which key pairs correspond, and at what coverage.
4. Report the ceiling each downstream cross-check is subject to.

**Output:** a key coverage matrix with a correspondence percentage per
pair.
**Success criterion:** every later cross-check in this class states its
coverage ceiling up front instead of implying completeness.

### B7. CMDB three-bucket reconciliation
**Objective:** Know which resources the CMDB is missing, which of its
records are stale, and where the two systems disagree.
**Inputs:** graph (cloud resources) + CMDB extract
**Steps:**
1. Bucket one: in the cloud, not in the CMDB.
2. Bucket two: in the CMDB, not in the cloud.
3. Bucket three: present in both, attributes disagree. Owner,
   environment, criticality.

**Output:** three lists with counts, and a disagreement breakdown by
attribute.
**Success criterion:** each bucket can be handed to a different remedial
process.

### B8. Ownerless-resource cross-check
**Objective:** Security's unattributed risk, FinOps's unattributed spend,
and the CMDB's unowned records are one dataset, and three teams currently
pay separately for the same data quality failure.
**Inputs:** graph (cloud resources, issues) + cost data + CMDB extract
**Steps:**
1. Resources with no owner tag or a tag pointing at a dissolved team.
2. Narrow to resources carrying open issues.
3. Narrow to resources carrying cost.
4. Report the three-way overlap.

**Output:** the overlap with counts, plus the risk and spend each segment
carries.
**Success criterion:** one list serves all three teams, and the overlap
size justifies a single remediation programme instead of three.

### B9. Control assertion reconciliation
**Objective:** The GRC platform holds a claim that a control is
implemented. The security tool holds a fact. Nobody has compared them.
**Inputs:** graph (controls) + GRC control register
**Steps:**
1. Extract asserted controls from the GRC register.
2. Extract actual enablement from the graph.
3. Compare in both directions.
4. Flag assertions that cannot be substantiated.

**Output:** a comparison with an unsubstantiated-assertion list.
**Success criterion:** the operator stops signing attestations that
cannot be evidenced.

### B10. Change drift reconciliation
**Objective:** Point-in-time compliance is not continuous compliance. A
control disabled on the 3rd and re-enabled on the 27th reads green on the
30th.
**Inputs:** graph (audit log, controls) + change management records
**Steps:**
1. Extract configuration and control changes from the audit log.
2. Relate them to approved change records.
3. Flag changes with no corresponding approval.
4. Identify controls that were disabled for any interval in the period.

**Output:** an unapproved-change list and a continuity gap report per
control.
**Success criterion:** an attestation can say "continuously enabled" and
mean it, or say where it cannot.

### B11. Scan coverage gap
**Objective:** A scanner reports on what it can see. An account it was
never pointed at produces no finding, so the question cannot be settled
from the scanner's own output alone.
**Inputs:** graph (cloud accounts) + **external vended-account roster**
**Steps:**
1. Obtain the authoritative account roster from the account vending
   pipeline, the billing hierarchy, or the org structure.
2. Obtain the accounts the scanner has connected.
3. Identify roster accounts the scanner does not have.
4. Report the gap with the age of each unscanned account.

**Output:** the unscanned account list, with a duration per entry.
**Success criterion:** the number is zero, or it is known. Today it is
neither.

### B12. Ticket reconciliation
**Objective:** Remediation metrics assume issues and tickets correspond.
They do not.
**Inputs:** graph (issues) + ticketing export
**Steps:**
1. Open issues with no corresponding ticket.
2. Closed tickets whose issue remains open.
3. Compare ticket age to issue age where both exist.
4. Count duplicate tickets against a single root cause.

**Output:** four counts with exemplars, and a false-closure rate.
**Success criterion:** the reported MTTR number can be defended, or is
known to be wrong and by how much.

### B13. Decommission verification
**Objective:** Retired means retired. Four hundred records say retired and
nothing evidences it.
**Inputs:** graph (cloud resources, cloud accounts) + CMDB retired records
**Steps:**
1. Extract records in a retired or decommissioned state.
2. Check whether the cloud still reports the corresponding resources.
3. Report contradictions in both directions.
4. Include accounts closed without the scanner being told.

**Output:** a contradiction list, split by direction.
**Success criterion:** records can be retired with evidence, and closed
accounts that still have a live connector are identified.

---

## Class C: Spoke-team runbooks

Run by the teams receiving the tickets.

### C14. Root-cause collapse
**Objective:** Three hundred and forty tickets for one CVE in one base
image. That is one finding rendered badly.
**Inputs:** graph (vulnerability findings, issues)
**Steps:**
1. Group findings by root cause: base image, module version, shared AMI.
2. Collapse to one item per cause, with instance count as a field.
3. Rank causes by instance count and severity.
4. **[SIMULATE]** any ticket creation or merge.

**Output:** a cause-ranked list with instance counts.
**Success criterion:** the item count an owning team receives matches the
number of distinct fixes they must make.

### C15. Fix-at-source mapping
**Objective:** Remediating a running resource is remediating a symptom. It
returns on the next deploy.
**Inputs:** graph (findings, resources) + IaC inventory and image registry
**Steps:**
1. Map each finding to the artifact that produced it: image tag, module
   version, manifest.
2. Walk back to the repository and commit.
3. Identify the change that would prevent recurrence.
4. **[SIMULATE]** the pull request.

**Output:** finding-to-source mapping with a proposed change per artifact.
**Success criterion:** success is not "ticket closed" but "no new
resources created from the vulnerable artifact after date X", which is
checkable.

### C16. Account enrollment lifecycle
**Objective:** Accounts are vended by one team and connected to the
scanner by another, with no automated link. The interval between is a
blind window nobody measures.
**Inputs:** graph (cloud accounts) + **vending pipeline records**
**Steps:**
1. For each account, obtain the vend timestamp.
2. Obtain the scanner connection timestamp.
3. Compute the window per account; report the distribution and the worst
   case.
4. Identify accounts currently inside the window.

**Output:** a window distribution and a live-exposure list.
**Success criterion:** the window has a measured maximum, so a standard
can be written against it. Today it is unbounded.

### C17. Regression and recurrence tracking
**Objective:** A fixed finding that returns was never fixed.
**Inputs:** graph (findings, issues) over time
**Steps:**
1. Compute a stable signature per finding, independent of resource id.
2. Identify signatures that closed and reappeared within a window.
3. Distinguish reappearance on the same resource from a new resource.
4. Report a recurrence rate per cause.

**Output:** a recurrence list with intervals and a rate.
**Success criterion:** remediation metrics separate fixed from returned.

### C18. Resolved versus evaporated disambiguation
**Objective:** Tickets age out because the resource stopped existing. The
finding closes. Nothing was fixed.
**Inputs:** graph (issues, findings, resources)
**Steps:**
1. Take resolved issues in the period.
2. Determine whether the affected entity still exists.
3. Split into remediated and evaporated.
4. Compute what the resolution metrics look like with evaporated removed.

**Output:** the split, with a corrected metric beside the reported one.
**Success criterion:** the gap between the two numbers is quantified.

### C19. Exception and risk acceptance round-trip
**Objective:** Much of what a spoke team receives is not exploitable in
its context. Today the options are patch it anyway or let the ticket go
red, and the third path does not survive.
**Inputs:** graph (issues) + GRC exception register
**Steps:**
1. Identify issues with a filed exception.
2. Determine whether the scanner suppresses them.
3. Identify exceptions past expiry.
4. Identify issues repeatedly reopened despite an accepted risk.
   **[SIMULATE]** any suppression, dismissal, or acceptance.

**Output:** an exception reconciliation, plus a reopened-despite-accepted
count.
**Success criterion:** exceptions get filed again, because they stick.

### C20. Asset claiming and contest
**Objective:** Ownership run bottom-up. A team should be able to ask what
the tool thinks is theirs, and dispute it.
**Inputs:** graph (resources, issues) + team and ownership registry
**Steps:**
1. Given a team, enumerate what the graph attributes to it.
2. Present the attribution basis: tag, account, inherited.
3. Identify attributions resting on a dissolved team or a stale tag.
4. **[SIMULATE]** any reassignment.

**Output:** a per-team inventory with attribution basis and a disputable
list.
**Success criterion:** the nearest living human in a tag stops inheriting
a dissolved team's tickets.

---

## Index

| ID | Runbook | Class | External input |
|---|---|---|---|
| A1 | Remediation SLA sweep | query | none |
| A2 | Emergency blast radius | query | none |
| A3 | Toxic combination triage | query | none |
| A4 | Standing credential review | query | none |
| A5 | Framework evidence pull | query | none |
| B6 | Join key coverage | join | all systems |
| B7 | CMDB three-bucket reconciliation | join | CMDB extract |
| B8 | Ownerless-resource cross-check | join | cost + CMDB |
| B9 | Control assertion reconciliation | join | GRC register |
| B10 | Change drift reconciliation | join | change records |
| B11 | Scan coverage gap | join | account roster |
| B12 | Ticket reconciliation | join | ticketing export |
| B13 | Decommission verification | join | CMDB retired |
| C14 | Root-cause collapse | spoke | none |
| C15 | Fix-at-source mapping | spoke | IaC + registry |
| C16 | Account enrollment lifecycle | spoke | vending records |
| C17 | Regression and recurrence | spoke | none (needs history) |
| C18 | Resolved versus evaporated | spoke | none |
| C19 | Exception round-trip | spoke | GRC exceptions |
| C20 | Asset claiming and contest | spoke | ownership registry |
