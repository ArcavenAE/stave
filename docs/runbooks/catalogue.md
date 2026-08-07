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

> **Status.** Written before the pre-run review completed. Expect
> amendment from that review, particularly to the class A steps, which a
> capability audit may show are not achievable as written.

---

## How to read this

**Three classes**, and the split is the catalogue's main finding rather
than an organising convenience:

| Class | Definition | Count |
|---|---|---|
| **A. Queries** | Answerable from the security graph alone | 5 |
| **B. Joins** | Require a mandatory external input | 8 |
| **C. Spoke-team** | Run by the teams receiving the tickets, not the team sending them | 7 |

Class A runbooks are queries against a tool that has the data. Class B
runbooks fail at a join, every time, and the join is against a system the
security tool cannot see. That is a difference in kind, and a tool built
only to answer queries makes the joins possible and painful, which is why
they currently live in spreadsheets.

**Originating persona** is recorded per runbook because that persona
judges whether an attempt satisfied it (bd `aae-orc-e4jo.10`). They wrote
the success criterion; they are the right party to say whether the output
would serve them.

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

### Two collisions worth keeping

**The IaC address beats every other join key.** Minted at resource
creation, carries lineage, answers "what is this" and "who made it" in
one field. The CMDB `sys_id` is minted by an import process; the native
cloud id carries no provenance. This reframes shadow IT from "in the
cloud but not in the CMDB" to "in the cloud but not in git", which is a
cleaner definition and a computable one.

**Three runbooks answer with an absence** (B11, B13, C16). An absence
cannot be produced by querying the tenant, because the tool cannot report
what it was never pointed at. Each needs an external roster carried in
and diffed. This is the strongest structural claim the catalogue makes
about stave's shape.

---

## Class A: Queries

Answerable from the security graph alone. These work today in principle;
the exercise tests whether they work in practice.

### A1. Remediation SLA sweep
**Persona:** Priya Raghunathan, Cloud Vulnerability Manager
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
**Success criterion:** Priya can walk into the monthly with it and answer
"who is chasing what" without opening another system.
**Known risk:** step 4 is the one that fails. Owner attribution is the
recurring wall across this whole catalogue.

### A2. Emergency blast radius
**Persona:** Priya Raghunathan
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
**Success criterion:** answerable inside twenty minutes. Steps 1 to 3
currently take minutes; step 4 takes two days, and that gap is the point
of running this one.

### A3. Toxic combination triage
**Persona:** Priya Raghunathan
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
**Success criterion:** the list is short enough that Priya works all of
it, and she believes the ranking.

### A4. Standing credential review
**Persona:** Marcus Bell, SecOps and detection engineering
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
**Success criterion:** Marcus can take the candidate list to the owning
teams and ask "do you still need this".

### A5. Framework evidence pull
**Persona:** Greta Lindqvist, IT Risk and GRC
**Objective:** An auditor asks what is implemented. A list is not
evidence; a dated, reproducible derivation is.
**Inputs:** graph only (controls, security frameworks, cloud config rules)
**Steps:**
1. Enumerate frameworks and their enablement.
2. For the framework in scope, enumerate its controls and enablement.
3. Produce coverage by severity.
4. Stamp the artifact with a date and the exact derivation.
**Output:** a dated artifact stating as-of time, method, and result.
**Success criterion:** it survives the three questions Greta always gets:
as of when, how derived, and show me last quarter's.

---

## Class B: Joins

Each requires an external input the security graph does not contain. This
is the class the exercise expects stave to fail, informatively.

### B6. Join key coverage
**Persona:** Dr. Ines Bauer, data reconciliation
**Objective:** Precondition for every other runbook in this class. Every
cross-check dies on a join key, so measure the keys before attempting any
join.
**Inputs:** graph, plus every external system in scope
**Steps:**
1. For each system, identify the candidate keys it carries.
2. Measure the fraction of records populating each key.
3. Identify which key pairs actually join, and at what coverage.
4. Report the ceiling each downstream reconciliation is subject to.
**Output:** a key coverage matrix with a joinability percentage per pair.
**Success criterion:** every later reconciliation in this class states its
coverage ceiling up front instead of implying completeness.

### B7. CMDB three-bucket reconciliation
**Persona:** Dale Okonkwo, CMDB and ITSM architect
**Objective:** Know which resources the CMDB is missing, which of its
records are stale, and where the two systems disagree.
**Inputs:** graph (cloud resources) + CMDB extract
**Steps:**
1. Bucket one: in the cloud, absent from the CMDB. Shadow IT.
2. Bucket two: in the CMDB, absent from the cloud. Stale records.
3. Bucket three: present in both, attributes disagree. Owner,
   environment, criticality.
**Output:** three lists with counts, and a disagreement breakdown by
attribute.
**Success criterion:** Dale can hand each bucket to a different remedial
process. Bucket three is the one that matters; it is where bad decisions
originate.

### B8. Ownerless-resource cross-check
**Persona:** Renata Ochoa, FinOps, with Tobi Fenwick, cloud platform
**Objective:** Security's unattributed risk, FinOps's unattributed spend,
and the CMDB's unowned records are one dataset, and three teams currently
pay separately for the same data quality failure.
**Inputs:** graph (cloud resources, issues) + cost data + CMDB extract
**Steps:**
1. Resources with no owner tag or a tag pointing at a dissolved team.
2. Intersect with resources carrying open issues.
3. Intersect with resources carrying cost.
4. Report the three-way overlap.
**Output:** the intersection with counts, plus the risk and spend each
segment carries.
**Success criterion:** one list serves all three teams, and the overlap
size justifies a single remediation programme instead of three.

### B9. Control assertion reconciliation
**Persona:** Greta Lindqvist
**Objective:** The GRC platform holds a claim that a control is
implemented. The security tool holds a fact. Nobody has compared them.
**Inputs:** graph (controls) + GRC control register
**Steps:**
1. Extract asserted controls from the GRC register.
2. Extract actual enablement from the graph.
3. Diff, in both directions.
4. Flag assertions that cannot be substantiated.
**Output:** a reconciliation with an unsubstantiated-assertion list.
**Success criterion:** Greta stops signing attestations she cannot
evidence.

### B10. Change drift reconciliation
**Persona:** Marcus Bell
**Objective:** Point-in-time compliance is not continuous compliance. A
control disabled on the 3rd and re-enabled on the 27th reads green on the
30th.
**Inputs:** graph (audit log, controls) + change management records
**Steps:**
1. Extract configuration and control changes from the audit log.
2. Join to approved change records.
3. Flag changes with no corresponding approval.
4. Identify controls that were disabled for any interval in the period.
**Output:** an unapproved-change list and a continuity gap report per
control.
**Success criterion:** Greta's attestation can say "continuously
enabled" and mean it, or say where it cannot.

### B11. Scan coverage gap
**Persona:** Marcus Bell, resolved by Kwame Adeyemi, landing zone
**Objective:** The single most important runbook in the catalogue, and the
one that cannot be answered from inside the tool. A scanner reports on
what it can see. An account it was never pointed at is not a finding; it
is an absence, and it is invisible precisely in the system you would use
to look.
**Inputs:** graph (cloud accounts) + **external vended-account roster**
**Steps:**
1. Obtain the authoritative account roster from the account vending
   pipeline, the billing hierarchy, or the org structure.
2. Obtain the accounts the scanner has connected.
3. Anti-join: in the roster, absent from the scanner.
4. Report the gap with the age of each unscanned account.
**Output:** the unscanned account list, with a duration per entry.
**Success criterion:** the number is zero, or it is known. Today it is
neither.
**Note:** this runbook is the reason the anti-join is the leading verb
candidate.

### B12. Ticket reconciliation
**Persona:** Priya Raghunathan, with Deepak Varma
**Objective:** Remediation metrics assume issues and tickets correspond.
They do not.
**Inputs:** graph (issues) + ticketing export
**Steps:**
1. Open issues with no corresponding ticket. Unworked.
2. Closed tickets whose issue remains open. False closure.
3. Compare ticket age to issue age where both exist.
4. Count duplicate tickets against a single root cause.
**Output:** four counts with exemplars, and a false-closure rate.
**Success criterion:** the MTTR number on Priya's Thursday slide can be
defended, or is known to be wrong and by how much.

### B13. Decommission verification
**Persona:** Dale Okonkwo, with Kwame Adeyemi
**Objective:** Retired means retired. Four hundred records say retired and
nothing evidences it.
**Inputs:** graph (cloud resources, cloud accounts) + CMDB retired records
**Steps:**
1. Extract records in a retired or decommissioned state.
2. Check whether the cloud still reports the corresponding resources.
3. Report contradictions in both directions.
4. Include accounts closed without the scanner being told.
**Output:** a contradiction list, split by direction.
**Success criterion:** Dale can retire records with evidence, and Kwame
learns which closed accounts still have a live connector.

---

## Class C: Spoke-team runbooks

Run by the teams receiving the tickets. Central IT's runbooks assume a
cooperative spoke; these are what the spoke actually experiences.

### C14. Root-cause collapse
**Persona:** Deepak Varma, application DevOps
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
**Persona:** Sanne de Vries, GitOps and platform DevOps
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
**Persona:** Kwame Adeyemi
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
**Persona:** Dr. Ines Bauer, with Deepak Varma
**Objective:** A fixed finding that returns was never fixed.
**Inputs:** graph (findings, issues) over time
**Steps:**
1. Compute a stable signature per finding, independent of resource id.
2. Identify signatures that closed and reappeared within a window.
3. Distinguish reappearance on the same resource from a new resource.
4. Report a recurrence rate per cause.
**Output:** a recurrence list with intervals and a rate.
**Success criterion:** remediation metrics separate fixed from returned.
**Note:** requires history. A single point in time cannot answer it, which
makes it a test of whether the tool can express change at all.

### C18. Resolved versus evaporated disambiguation
**Persona:** Deepak Varma
**Objective:** Half of Deepak's tickets age out because the resource
stopped existing. The finding closes. Nothing was fixed.
**Inputs:** graph (issues, findings, resources)
**Steps:**
1. Take resolved issues in the period.
2. Determine whether the affected entity still exists.
3. Split into remediated and evaporated.
4. Compute what the resolution metrics look like with evaporated removed.
**Output:** the split, with a corrected metric beside the reported one.
**Success criterion:** the gap between the two numbers is quantified. If
it is large, the remediation programme is measuring infrastructure churn.

### C19. Exception and risk acceptance round-trip
**Persona:** Deepak Varma, contested by Greta Lindqvist
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
**Success criterion:** Deepak files exceptions again, because they stick.
**Note:** unresolved between the two personas. Greta has a process; the
scanner does not know about it; Deepak stopped using it. All three are
true.

### C20. Asset claiming and contest
**Persona:** Kwame Adeyemi, with Renata Ochoa
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

| ID | Runbook | Class | External input | Judge |
|---|---|---|---|---|
| A1 | Remediation SLA sweep | query | none | Priya |
| A2 | Emergency blast radius | query | none | Priya |
| A3 | Toxic combination triage | query | none | Priya |
| A4 | Standing credential review | query | none | Marcus |
| A5 | Framework evidence pull | query | none | Greta |
| B6 | Join key coverage | join | all systems | Ines |
| B7 | CMDB three-bucket reconciliation | join | CMDB extract | Dale |
| B8 | Ownerless-resource cross-check | join | cost + CMDB | Renata |
| B9 | Control assertion reconciliation | join | GRC register | Greta |
| B10 | Change drift reconciliation | join | change records | Marcus |
| B11 | Scan coverage gap | join | account roster | Marcus |
| B12 | Ticket reconciliation | join | ticketing export | Priya |
| B13 | Decommission verification | join | CMDB retired | Dale |
| C14 | Root-cause collapse | spoke | none | Deepak |
| C15 | Fix-at-source mapping | spoke | IaC + registry | Sanne |
| C16 | Account enrollment lifecycle | spoke | vending records | Kwame |
| C17 | Regression and recurrence | spoke | none (needs history) | Ines |
| C18 | Resolved versus evaporated | spoke | none | Deepak |
| C19 | Exception round-trip | spoke | GRC exceptions | Deepak |
| C20 | Asset claiming and contest | spoke | ownership registry | Kwame |

## The cast

| Persona | Role |
|---|---|
| Priya Raghunathan | Cloud Vulnerability Manager. Owns the remediation SLA. |
| Dale Okonkwo | CMDB and ITSM architect. The configuration item is the atom of IT. |
| Greta Lindqvist | IT Risk and GRC. Thinks in control objectives and evidence dates. |
| Tobi Fenwick | Cloud platform engineer. Believes everything is a tagging problem. |
| Renata Ochoa | FinOps. Hunts orphaned spend. |
| Marcus Bell | SecOps and detection. Lives in the audit log. |
| Dr. Ines Bauer | Data reconciliation. Knows where the join keys are buried. |
| Kwame Adeyemi | Cloud platform, landing zone. Vends the accounts. |
| Sanne de Vries | GitOps and platform DevOps. Reconciles from git or it is not real. |
| Deepak Varma | Application DevOps. Receives the tickets. |

Session dynamics and running threads:
`_bmad-output/party-mode/memories/installed/.memlog.md`.
