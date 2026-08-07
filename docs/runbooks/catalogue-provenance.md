# Runbook catalogue: provenance and reasoning

**SEALED.** Read by the judges (bd `aae-orc-e4jo.10`) and by the analysis
(`.7` and `.15`). **Not read by the executor or the paper-pipeline
author** (`.14`).

Companion to `catalogue.md`, which holds the runbooks themselves. This
file holds everything the party concluded *about* them: the analytical
patterns it noticed, which persona contributed each runbook, and the
editorial judgements that shaped the catalogue's prose.

Split out 2026-08-07 per bd `aae-orc-e4jo.1`. The reason for the split is
below and is itself part of the method.

---

## Why this file exists

The catalogue as first written contained the answer key, and the executor
reads the catalogue.

The exercise measures whether role-cast elicitation surfaces composite
verbs that reading the vendor's API would not. If the executor is told
which analytical pattern each runbook wants before attempting it, and
then reaches for that pattern, the pattern has been transmitted rather
than measured. The result would look like a finding and be an echo.

The same contamination reaches the judges through the persona mapping,
which is why the mapping is here too. A judge who knows they are judging
their own runbook is not scoring the output, they are recognising
themselves in it.

---

## Two collisions from the party

These are the party's strongest analytical claims. Both point at the same
structural conclusion, which is precisely why they are withheld from the
executor.

**1. The IaC address beats every other join key.** Minted at resource
creation, carries lineage, answers "what is this" and "who made it" in
one field. The CMDB `sys_id` is minted by an import process; the native
cloud id carries no provenance. This reframes shadow IT from "in the
cloud but not in the CMDB" to "in the cloud but not in git", which is a
cleaner definition and a computable one.

*Audit note, added 2026-08-07:* the field-surface audit
(`docs/design/field-surface-audit.md`) found this claim is computable but
only through `cloudResourcesV2`, which stave does not bind. Under the
current binding the IaC address is not merely awkward to reach, it is
unreachable, so an executor working the curated surface would conclude
the tool cannot express it. That conclusion would be about stave and not
about Wiz.

**2. Three runbooks answer with an absence** (B11, B13, C16). An absence
cannot be produced by querying the tenant, because the tool cannot report
what it was never pointed at. Each needs an external roster carried in
and compared. This is the strongest structural claim the catalogue makes
about stave's shape.

Both collisions point at the same verb, which is the leading registered
candidate. An executor who reads them and then proposes that verb has
been told, not measured.

---

## The class split as a finding

The three-class split is the catalogue's main finding rather than an
organising convenience.

Class A runbooks are queries against a tool that has the data. Class B
runbooks fail at a join, every time, and the join is against a system the
security tool cannot see. That is a difference in kind, and a tool built
only to answer queries makes the joins possible and painful, which is why
they currently live in spreadsheets.

The class labels themselves stay in the executor's copy, because a
runbook's class is part of what it is and the external-input column
cannot be hidden without making the runbooks unattemptable. What is
withheld is this reading of what the split means.

---

## Editorial judgements the split required

Recorded so the redaction is auditable rather than silent. Anyone
comparing the two files should be able to see exactly what moved and on
what rule.

**The rule applied:** domain nouns in runbook titles and objectives stay
in the executor's copy; imperative tool-verbs inside *steps* were
rephrased into outcome language.

The distinction is that "CMDB reconciliation" is what operators call the
activity, and removing it would distort the runbook past recognition,
while "Anti-join: in the roster, absent from the scanner" as a step is an
instruction to the executor naming the operation to perform. The first
describes the job. The second prescribes the tool.

Specific changes:

| Location | Was | Now | Reason |
|---|---|---|---|
| B11 step 3 | "Anti-join: in the roster, absent from the scanner" | "Identify roster accounts the scanner does not have" | named the candidate verb outright |
| B11 note | "this runbook is the reason the anti-join is the leading verb candidate" | removed to this file | states the expected answer |
| B11 objective | "The single most important runbook in the catalogue, and the one that cannot be answered from inside the tool ... it is an absence, and it is invisible precisely in the system you would use to look" | neutral restatement | collision 2, plus an importance ranking the executor should not inherit |
| B9 step 3 | "Diff, in both directions" | "Compare in both directions" | `diff` is a registered verb candidate |
| B8 steps 2 and 3 | "Intersect with ..." | "Narrow to ..." | set-operation vocabulary |
| B6 step 3 | "which key pairs actually join" | "which key pairs correspond" | same |
| A1 | "Known risk: step 4 is the one that fails. Owner attribution is the recurring wall across this whole catalogue." | removed to this file | tells the executor where to expect failure and generalises it across the set |
| A2 | "Steps 1 to 3 currently take minutes; step 4 takes two days, and that gap is the point of running this one." | removed to this file | states the expected finding |
| C17 | "requires history. A single point in time cannot answer it, which makes it a test of whether the tool can express change at all." | removed to this file | states the expected finding |
| C19 | note on the unresolved disagreement between two personas | removed to this file | persona reasoning |
| all runbooks | `**Persona:**` line | removed to this file | judge contamination |
| index | Judge column | removed to this file | same |
| end matter | the cast table and the memlog pointer | removed to this file | same |
| header | "Two collisions worth keeping" section | removed to this file | collisions 1 and 2 |

Retained deliberately, with reasons:

- **"reconciliation" in the titles of B7, B9, B10, B12.** A registered
  verb candidate is spelled `reconcile`, but these are standard ITSM and
  GRC terms and the runbooks are named after the activity. Removing the
  word would distort the elicited artifact more than it would protect the
  measurement.
- **"coverage" in B6 and B11 titles.** Same reasoning.
- **"Collapse" in C14.** The runbook's own name for the job, and not a
  registered candidate.
- **B6's objective referring to join keys.** B6 is a runbook *about*
  measuring join keys. Its subject matter is not a leaked hint.

A residual risk worth stating rather than hiding: this redaction reduces
transmission, it does not eliminate it. The class names remain, the
external-input column remains, and a competent executor may infer the
shape from the structure alone. If the paper pipelines converge on the
leading candidate, that convergence is weaker evidence than it would be
from a fully blind arm, and `.15` should read it that way.

---

## Originating personas

Each persona judges whether an attempt satisfied the runbook they
contributed (bd `aae-orc-e4jo.10`). They wrote the success criterion, so
they are the right party to say whether the output would serve them.

| ID | Runbook | Judge |
|---|---|---|
| A1 | Remediation SLA sweep | Priya Raghunathan |
| A2 | Emergency blast radius | Priya Raghunathan |
| A3 | Toxic combination triage | Priya Raghunathan |
| A4 | Standing credential review | Marcus Bell |
| A5 | Framework evidence pull | Greta Lindqvist |
| B6 | Join key coverage | Dr. Ines Bauer |
| B7 | CMDB three-bucket reconciliation | Dale Okonkwo |
| B8 | Ownerless-resource cross-check | Renata Ochoa, with Tobi Fenwick |
| B9 | Control assertion reconciliation | Greta Lindqvist |
| B10 | Change drift reconciliation | Marcus Bell |
| B11 | Scan coverage gap | Marcus Bell, resolved by Kwame Adeyemi |
| B12 | Ticket reconciliation | Priya Raghunathan, with Deepak Varma |
| B13 | Decommission verification | Dale Okonkwo, with Kwame Adeyemi |
| C14 | Root-cause collapse | Deepak Varma |
| C15 | Fix-at-source mapping | Sanne de Vries |
| C16 | Account enrollment lifecycle | Kwame Adeyemi |
| C17 | Regression and recurrence | Dr. Ines Bauer, with Deepak Varma |
| C18 | Resolved versus evaporated | Deepak Varma |
| C19 | Exception round-trip | Deepak Varma, contested by Greta Lindqvist |
| C20 | Asset claiming and contest | Kwame Adeyemi, with Renata Ochoa |

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

## Notes carried out of the executor's copy

- **A1:** step 4 is the one that fails. Owner attribution is the recurring
  wall across this whole catalogue.
- **A2:** steps 1 to 3 currently take minutes; step 4 takes two days, and
  that gap is the point of running this one.
- **B7:** bucket three is the one that matters; it is where bad decisions
  originate.
- **B11:** the single most important runbook in the catalogue, and the one
  that cannot be answered from inside the tool.
- **C17:** requires history. A single point in time cannot answer it,
  which makes it a test of whether the tool can express change at all.
- **C19:** unresolved between the two personas. Greta has a process; the
  scanner does not know about it; Deepak stopped using it. All three are
  true.
