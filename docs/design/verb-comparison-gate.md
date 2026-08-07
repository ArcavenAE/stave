# Comparison gate: does the elicitation beat the vendor-surface baseline

bd `aae-orc-e4jo.15` (S4a). Written 2026-08-07 by a party that authored
neither arm.

**Inputs read.** `docs/design/verb-baseline-vendor-surface.md` (the
sealed control arm, including its isolation attestation in full),
`docs/runbooks/paper-pipelines.md` (the treatment arm),
`docs/design/verb-candidate-registration.md`,
`docs/design/verb-scoring-amendments.md`,
`docs/design/field-surface-audit.md`, `docs/runbooks/catalogue.md`,
`docs/runbooks/catalogue-provenance.md`, and bd `aae-orc-e4jo` and
`aae-orc-e4jo.15`. The `stave` binary was not run. Nothing in this
document adjusts N, M, the glue kinds, the decoy set, the blind-scoring
procedure, or any outcome trigger.

---

## The ruling

**Outcome 3. Continue.**

| Figure | Value | Threshold | Fires? |
|---|---|---|---|
| Corrected overlap (the threshold figure) | **0 percent** (0 of 5) | 80 percent | no |
| Raw overlap (reported beside it) | **20 percent** (1 of 5) | not the threshold figure | no |
| Priors qualifying | 1 of 5 (`reconcile`) | outcome 4 needs 0 of 12 | no |
| Decoys qualifying | 0 of 7 | outcome 2 needs comparable | no |

Outcome 3 as narrowed by amendment A1 requires all three of: some verb
qualifies, overlap is below threshold, decoys do not score comparably.
All three hold. Outcomes 1, 2, and 4 do not fire.

The two overlap figures fall on the same side of the threshold, so the
"opposite sides is itself the finding" clause in the audit's asymmetry
ruling does not apply.

**The single most important reason.** The corrected overlap is zero
because the control arm, working from the full vendored schema and
Wiz's own product documentation, proposed no verb that crosses out of
Wiz, and could not have. Every one of the thirteen `join` appearances
in the treatment arm is a bridge from the security graph to a system
the schema does not describe. The elicitation's contribution is not a
verb name. It is a class of work that reading the vendor surface cannot
produce by construction, because the vendor surface does not contain
the other half of the join.

That finding is certified by this gate. Nothing else in the treatment
arm is. Section 7 sets out what does not survive.

---

## 1. Baseline verbs tagged

Per the audit's asymmetry ruling. `reachable` means justified by fields
and roots the curated documents already select; `surface-advantaged`
means justified only by `CloudResourceV2`, the `filterBy` and `orderBy`
arguments, the unbound aggregation, history, or diff roots, or an
unselected field.

| # | Baseline verb | Justification rests on | Tag |
|---|---|---|---|
| 1 | `count` | `*GroupedByValues` roots (unbound), connection-level counters (unselected), `filterBy` for the server-side narrowing flags (undeclared) | **surface-advantaged** |
| 2 | `graph` | `graphSearch`, `savedGraphQueries`, `savedGraphQuery`, `builtinSavedGraphQuery`, all unbound | **surface-advantaged** |
| 3 | `coverage` | `cloudAccounts` bound; `lastScannedAt` selected. `--unhealthy` additionally needs `sourceDeployments` (unselected) and the `deployments` and `systemHealthIssues` roots (unbound) | **reachable** |
| 4 | `context <kind> <id>` | singular `issue(id)` root (unbound; `kinds.rs` states get-by-id is unsupported), `issueHistoryEvents` (unbound), `issueSuggestedAssignees` (unbound), `evidenceRecords` (unselected) | **surface-advantaged** |
| 5 | `exposure <entity-id>` | `networkExposures`, `lateralMovementPaths`, `entityEffectiveAccessEntries`, none bound | **surface-advantaged** |
| 6 | `posture` | `SecurityFramework.complianceAnalytics` and `.controls` (unselected), `projectsWithComplianceAnalytics` and `cloudAccountsWithComplianceAnalytics` (unbound), `policyComplianceAnalytics` (unbound) | **surface-advantaged** |
| 7 | `trend <kind>` | `issuesTrendV2`, `issuesTrend`, `auditLogEntriesTrend`, `Project.issuesTrend`, `SecurityFramework.complianceTrend`, all unbound | **surface-advantaged** |

**Reachable 1, surface-advantaged 6.** Verb 3 is tagged reachable on
its core mode. Two of its three modes are not, and that is recorded
rather than folded into the tag, because the definition turns on
"justified only by" unreachable surface and verb 3's core question
("has this account been scanned, and when") is answerable from
`cloudAccounts` plus `lastScannedAt` today.

The baseline's two stated prerequisites, P1 `get <kind> <id>` and P2
server-side filter pushdown, are excluded from the verb set because the
baseline excludes them itself. Both are surface-advantaged, and both
correspond to already-filed capability work (`aae-orc-j1xi` and the
get-by-id half of charter F2).

**Six of seven baseline verbs are unbuildable against stave as it
stands.** Amendment A6 anticipated this for outcome 1; it is worth
stating here even though outcome 1 did not fire, because the same fact
bears on the continuation ordering in section 8.

---

## 2. The runbook-derived set was never defined. Closing that first.

The `.12` pre-registration says "if the runbook-derived verb set
overlaps this baseline by 80 percent or more". Amendment A3 and the
audit both fix the direction as `|intersection| / |runbook-derived
set|`. Neither defines the set that sits in the denominator. Three
readings are defensible:

| Reading | Set | Size |
|---|---|---|
| (a) the registered priors | P1 `join`, P2 `roll-up`, P3 `coverage`, P4 `diff`, P5 `reconcile` | 5 |
| (b) the entries that qualified | P5 `reconcile` | 1 |
| (c) the verbs the pipelines actually demanded | see below | 5 or 9 |

**Ruling: reading (a).** Three reasons.

1. It is the only one of the three fixed before evidence existed, which
   is what the pre-registration is for. Readings (b) and (c) are both
   functions of the treatment arm's output.
2. Reading (b) makes the headline statistic a function of the
   predicate. The registration's own section 5 treats the predicate as
   something that may turn out to be wrong, so a metric that inherits
   its defects cannot adjudicate it. Section 7 shows the predicate does
   have a defect, which would have propagated straight into the
   threshold figure.
3. The umbrella ticket names exactly this set under "VERB CANDIDATES,
   unchanged and now registered rather than assumed".

**Reading (c) collapses onto (a) under its narrow form.** All five
priors appear in the pipelines (N of 13, 16, 12, 5, 5), and the
treatment arm proposed no named verb outside the twelve registered
entries. What it did surface beyond them is four unnamed capability
demands, recorded as notes rather than proposals: an arity-3 join (B8
stage 7), a correspondence rule that is object identity plus a time
window rather than a key (B10 stage 4), interval reconstruction from an
event sequence (B10 stage 6), and a snapshot store to give a temporal
comparison its second time point (C17 stage 4). Reading (c) wide is
those four plus the five priors, giving 9.

Decoys are excluded from every reading. They have no runbook provenance
by construction.

**This was a defect in the pre-registration.** Recorded in section 9.

---

## 3. Overlap arithmetic

Overlap is measured over verb name **plus** argument shape. Two
collisions are ruled out as matches in advance, per amendment A4 and
the registration's section 2: baseline verb 3 `coverage` against prior
P3 `coverage` (same name, different verbs), and decoy D7 `diff --since`
against prior P4 `diff` (decoys are outside the runbook-derived set
anyway).

### Match determination, prior by prior

| Prior | Argument shape | Nearest baseline verb | Ruling |
|---|---|---|---|
| P1 `join` | two streams, a key, mode `inner` / `left-anti` / `right-anti` | none. Verbs 4 and 5 fan several server roots for one id; neither takes two streams and neither reaches outside Wiz | **no match** |
| P2 `roll-up` | a stream, a grouping key, an aggregate | verb 1 `count` | **match**, generous ruling (see below) |
| P3 `coverage` | two streams and a key; fraction that correspond | verb 3 `coverage` (name only) | **no match**, ruled at registration |
| P4 `diff` | two streams and a key; per-field disagreement | none. Verb 7 `trend` is a time series over one kind, not field disagreement across two streams | **no match** |
| P5 `reconcile` | two streams and a key; correspondence, absence both directions, disagreement | none | **no match** |

**The P2 ruling, stated openly because it is the one judgement call.**
P2 and baseline verb 1 answer the same operator question ("how many, by
group") and differ in locus (client-side walk versus server-side
aggregation) and in input type (a JSONL stream versus a kind plus
filter flags). Verb 1's own identity test excludes P2: "any verb that
returns counts or aggregates computed by the server rather than by
walking records is this verb", and P2 walks records. Section 4 of the
registration separately records `*GroupedByValue` as P2's server
analogue, which only makes sense if the two are distinct objects.

I ruled it a match anyway. The gate's null hypothesis is that the
elicitation bought nothing, so near-matches resolve toward overlap; a
gate that resolves ambiguity toward its own continuation is not a gate.
The ruling is also harmless, because verb 1 is surface-advantaged and
therefore excluded from the corrected figure. Ruling it a non-match
would move the raw figure from 20 percent to 0 percent and leave the
threshold figure unchanged.

### The two figures

The audit's asymmetry ruling says surface-advantaged verbs are
"excluded from the denominator". That phrasing predates the direction
being fixed. With the denominator fixed as the runbook-derived set by
A3, the only coherent application is to exclude surface-advantaged
baseline verbs from the **matchable pool**, which can only shrink the
numerator. Applied that way:

```
raw overlap       = |intersection over all 7 baseline verbs| / |runbook set|
                  = |{P2}| / |{P1,P2,P3,P4,P5}|
                  = 1 / 5
                  = 20.0 percent

corrected overlap = |intersection over reachable baseline verbs only| / |runbook set|
                  = |{}| / |{P1,P2,P3,P4,P5}|
                  = 0 / 5
                  = 0.0 percent
```

The corrected intersection is empty because the only reachable baseline
verb is verb 3 `coverage`, and matching it to P3 is ruled out at
registration. P2's single match was to verb 1, which is
surface-advantaged.

### The other two readings, reported so the choice is visible

| Reading | Denominator | Raw | Corrected |
|---|---|---|---|
| (a) registered priors, **used** | 5 | 1/5 = 20.0 percent | 0/5 = **0.0 percent** |
| (b) entries that qualified | 1 | 0/1 = 0.0 percent | 0/1 = 0.0 percent |
| (c) pipeline-demanded, narrow | 5 | 1/5 = 20.0 percent | 0/5 = 0.0 percent |
| (c) pipeline-demanded, wide | 9 | 1/9 = 11.1 percent | 0/9 = 0.0 percent |

Every reading returns a corrected overlap of zero and a raw overlap
between 0 and 20 percent. **The choice of reading does not affect the
outcome.** That is worth stating plainly: the definitional gap was a
real defect in the pre-registration and it happened not to matter here.

---

## 4. The disclosed anchoring, and what it does

The baseline's attestation states that `charter.md`, a permitted input,
names F4's placeholder composite verbs (`issue-triage`,
`vuln-exposure`, `posture-report`), and that this anchored its verbs 2,
4, 5, and 6. The control arm is therefore not independent of the
charter.

**How it was accounted for.** The four anchored verbs are 2 `graph`, 4
`context`, 5 `exposure`, and 6 `posture`. All four are
surface-advantaged, so all four are already outside the corrected
figure's matchable pool. The corrected pool is verb 3 alone, and verb 3
is not among the anchored four. No separate discount arithmetic is
needed: the surface-advantage correction and the anchoring discount
remove the same four verbs, plus two more.

**What it does to the raw figure.** Nothing. The single raw match is
P2 to verb 1, and verb 1 is not among the anchored four.

**Direction of the residual bias.** Amendment A5 records that the
placeholders are not the registered priors, so the anchoring pulls the
baseline toward F4 and away from the priors. If the anchoring has
biased the measured overlap at all, it has biased it **downward**, which
means an unanchored control arm would if anything have scored higher
than 20 percent raw. The gap between 20 percent and 80 percent is large
enough that this does not threaten the ruling, but the ruling does not
survive the anchoring for free, and the direction is recorded rather
than assumed benign.

---

## 5. Outcome 4, which did not fire and half of whose diagnosis holds

Outcome 4's trigger is "the predicate qualifies nothing in either set."
One entry qualified, so it does not fire. I am not rewriting the
trigger.

Its diagnosis is a different question, and it is worth answering because
the trigger was drawn around a case narrower than the failure it was
built to catch. Outcome 4 names two causes and asks which holds.

**Cause 2, pipelines too thin: ruled out affirmatively.** The
diagnostic the amendment specifies is "whether pipelines recorded
per-stage purposes at all". They did, for all twenty runbooks: 106 raw
stages, each with a named purpose, a survives-the-fix tag, and gap
marks checked against all twelve registered entries in a second pass
after drafting. The arm also produced a second tally isolating 16 stages
as document debt, and listed all sixteen with the ticket that deletes
each. This is not an unscoreable pipeline set.

**Cause 1, predicate too strict: holds, in a specific form.** Not
"too strict". Mis-specified. Conjunct 2 is named compression and
measures composition. Section 7 sets out the argument.

So the situation is the one outcome 4 was designed to catch, minus the
literal trigger condition. **That is a defect in the trigger, recorded
in section 9, and it does not change the ruling.** Outcome 3's own
conditions are met independently of it, and outcome 3's certified
finding (section 1) rests on the overlap arithmetic rather than on the
predicate.

---

## 6. Conjunct 3: does per-field disagreement deserve its own kind

The arm scored per-field disagreement across matched records as G5
derivation, reading G5 literally as computing a field from other fields
across a stream, where the stream is the joined one. No kind was added.
The arm reported the strain and asked the gate to rule.

**Ruling: the enumeration has a hole, and it is not being patched
now.**

The hole is real. G4 is "the same logical record compared at two
times", which is pairwise field comparison with time as the
correspondence rule. The enumeration names that special case and omits
the general one, where the correspondence rule is a key. Reading it into
G5 requires treating a matched pair as one record, which is legitimate
after a join but hides that the operation is undefined before one.

It is not being patched because the registration permits a kind to be
added **during** scoring with a note, and forbids tuning once a
measurement exists. A measurement exists. Adding a kind now, after
seeing which entries it favours, is the move the registration exists to
prevent.

**Entries affected, and the effect: none.** P4 and P5 are the two
entries whose scoring leans on the reading, and D7 would be affected if
G4 were folded into a general kind. Conjunct 3 is a disjunction over
kinds. P4 carries G1 independently and P5 carries G1 and G3
independently, so both satisfy conjunct 3 whichever way the comparison
is classified. D7 fails on conjunct 1 and would continue to. No score
moves. The correction belongs in any future registration, not in this
one.

---

## 7. The three items the treatment arm reported rather than resolved

The arm was right not to resolve them. Each is ruled here.

### 7.1 Conjunct 2 measures composition, not compression

**The claim.** Eleven of twelve entries fail conjunct 2 identically,
because a verb that does one thing replaces one stage per runbook
whatever its breadth.

**It is right in substance, with one correction.** It is not a hard
ceiling: a unary verb scores 2 in a runbook that uses its purpose
twice, and four entries do (P3 in B6, P2 in C17, D4 in A3 and C14, D7
in C17). But a **median** of 3 requires the purpose to recur three or
more times in over half the runbooks where the verb appears, which for
a single-purpose stream verb does not happen anywhere in this
catalogue. The arm's conclusion stands: under this predicate only a
composite can clear M = 3.

**What that invalidates, and what it does not.**

It does **not** invalidate the comparison as executed. The same
predicate was applied blind to both sets, and the priors do separate
from the decoys on the conjunct that discriminated. Median N across
priors is 12 (5, 5, 12, 13, 16); median N across decoys is 4 (0, 2, 3,
4, 4, 4, 8). Breadth is where the two sets differ, and conjunct 1 is
what measured it.

It **does** invalidate the specific reading "reconcile is the verb
this exercise found". P5 was registered as three operations bundled
into one name and cleared a bundling test. That is circular, and
section 7.2 adds a second reason the same entry cannot carry a
conclusion.

It also **substantially weakens the outcome-2 test**, which is the
finding here that nobody has stated yet. The decoy set is seven generic
stream verbs, every one of them single-purpose. Conjunct 2 therefore
made it structurally impossible for any decoy to qualify, no matter how
good a decoy was. The observed 0 of 7 is close to a foregone
conclusion, so its non-firing carries much less evidential weight than
the figure suggests. Outcome 2 did not fire, and it had little power to.
Recorded in section 9.

**Verdict: the scoring is valid as executed and cannot support the
conclusion its single qualifier invites.**

### 7.2 The single qualifier is confounded by the catalogue's vocabulary

**The confound is worse than the arm stated, and better evidenced.**

P5 `reconcile` appears in five runbooks: B7, B9, B12, B13, C19. Three
of those five (B7 "CMDB three-bucket reconciliation", B9 "Control
assertion reconciliation", B12 "Ticket reconciliation") carry the word
in the title. Two do not (B13, C19). One runbook that does carry it
(B10 "Change drift reconciliation") drew no P5 mark at all. So the
correlation is strong and not total, which is the honest version of the
arm's "cannot be separated from the result by any amount of care".

The confound is a disclosed one, not a discovered one. The provenance
file records the editorial ruling that kept the word ("standard ITSM
and GRC terms and the runbooks are named after the activity"), and
instructs this gate to read convergence on the leading candidate as
weaker evidence than a fully blind arm would give. That instruction is
followed.

**The deeper problem, which the arm did not have the inputs to see.**
The priors were "named by the same session that designed the exercise",
and that is the same party session that produced the catalogue. The
treatment arm's verb candidates and its evidence corpus therefore have a
common author. Blind scoring protects against scorer bias; it does not
protect against a hypothesis and its ground truth sharing a source. The
decoys are the only genuinely exogenous input to the predicate, which
raises the cost of 7.1's finding that they could not have qualified.

**Standing of the result: P5's qualification carries close to zero
independent evidential weight.** It is not evidence that elicitation
found `reconcile`. It is evidence that one party used one word
consistently. What survives is the overlap arithmetic, which compares
two arms with different authors, different permitted inputs, and one of
them sealed before the other existed.

### 7.3 `join` was one decomposition choice from qualifying

**Declining was right, and it is the strongest evidence in the file
that the pre-registration worked.**

The umbrella names `join --left-anti` as the leading candidate. The
registration records that expectation specifically "so it is visible as
a prior belief rather than discovered as a conclusion". Adopting, after
scoring, a convention whose effect is to promote that exact candidate
would have converted a registered prior belief into a manufactured
finding. The arm declined and said why. That is the mechanism working.

**The convention is also not obviously right on its merits.** A
`join --on <key-expr>` verb taking an expression is idiomatic (SQL joins
on expressions, and stave already carries CEL for `filter --where`).
But the normalisation stages it would absorb are not uniformly key
expressions: B7 stage 4 normalises two sides under different rules, and
C15 stage 5 folds image tag, digest, and module source into one artifact
key across three candidate fields, which is a derivation rather than a
key expression. The convention would over-credit `join` in at least some
of the ten runbooks it touches.

**Does the margin change the ruling? No, and the arithmetic is
checkable.** Under the alternative convention P1's median moves from 2
to 3 and it qualifies. Then 2 of 5 priors qualify and still 0 of 7
decoys, so outcome 2 does not fire; something qualifies, so outcome 4
does not fire; and overlap under reading (a) does not depend on
qualification at all, so both figures are unchanged at 20 percent raw
and 0 percent corrected. Under reading (b) the qualified set becomes
{P1, P5} and the intersection is still empty, so 0/2 = 0 percent.
**Outcome 3 under every combination.**

What the margin does change is which finding the exercise is entitled
to. Had P1 cleared, the exercise's headline would be the
graph-to-external boundary rather than `reconcile`, and that is the
better-founded of the two. The arm reached that reading anyway, without
the promotion: its post-fix analysis establishes that all thirteen of
`join`'s appearances are bridges to data outside Wiz, and that class B
loses only 2 of 44 glue stages to the four read-surface tickets against
class A's 10 of 27. **That analysis, not the qualifying entry, is the
exercise's most valuable output so far**, and it is what this gate
certifies in section 1.

---

## 8. Consequence: what unblocks, in what order

Outcome 3. Nothing closes as not needed. `.4`, `.5`, `.6`, `.7`, `.9`,
`.10`, and `.8` all remain live. The gate's finding changes their
ordering and the scope of two of them.

**1. `.9` run-log harness (scrub and gate by construction). First,
before anything touches the tenant.** It is the structural control:
`safety-coach-gate.md` requires the gate at the point invocations are
generated rather than at the point the harness was written, and
`tenant-data-hygiene.md` trigger 1 has no backstop at all. Both are
cheap to build and neither is recoverable after the fact.

**2. `.4` synthetic external fixtures for the join class.** No tenant
contact, no API budget, and it is the enabling artifact for the one
finding this gate certified. Class B is where the elicitation's
contribution lives.

**3. `.6` class B runs against fixtures.** The load-bearing evidence.
This is where the external-boundary claim either holds up or does not.
B11 is the runbook to start with: the arm identifies it as the only one
whose graph half is served by today's read surface.

**4. `.10` persona judges.** Point them at the class B outputs first.
The arm's sharpest honest limit is that it measures an LLM rather than
an operator ("an operator with the same data in a warehouse would push
every join and every group into SQL and would propose none of these
verbs"). The judges are the only instrument in the plan that tests the
operator side of that, and running them on the class where the finding
lives is worth more than running them evenly.

**5. `.5` class A runs, re-scoped or deferred.** The audit labels
nearly every class A step OURS/selection or OURS/binding, and the
post-fix tally deletes 10 of 27 class A glue stages, 37 percent. A live
class A run today measures the scaffold's caution, not the tool.
Sequence it behind `aae-orc-qijl` and `aae-orc-rsh6`, or scope it to the
steps the audit marks attemptable.

**6. `.7` full mining, then 7. `.8` proposal.**

**What `.8` must carry when it is written.** The corrected overlap of 0
percent and the raw of 20 percent with the P2 ruling that produced the
difference; the conjunct-2 defect and the fact that the decoy test had
little power; the `reconcile` confound and the common-author problem
behind it; the `join` post-fix reading as the certified finding; and
amendment A6's dependency, that six of seven baseline verbs cannot be
built until `aae-orc-rsh6`, `aae-orc-j1xi`, and `aae-orc-gs23` land.

**What this gate does not authorise.**

- It does not authorise `reconcile` as a v0.2 verb. Section 7.2.
- It does not authorise adopting the alternative `join` decomposition
  convention to promote P1. Section 7.3.
- It does not authorise a re-score under a repaired predicate. The
  measurement stands as measured. A repaired predicate is a fresh
  registration for a future question, not a second run at this one.
- It does not authorise adding a glue kind. Section 6.

---

## 9. Findings recorded against the pre-registration

Recorded as findings, not applied as edits.

**F1. The runbook-derived set was never defined.** The `.12`
pre-registration names a denominator it does not specify, and neither
amendment A3 nor the audit's parallel treatment closed the gap; both
fixed the direction and left the set open. Three defensible readings
existed at the gate. They happened to agree here. Any future
pre-registration of a ratio must name both sides of it.

**F2. Outcome 4's trigger is narrower than its diagnosis.** It fires on
"nothing qualifies in either set", but the failure it describes
(predicate too strict, or pipelines too thin) can hold with one entry
qualifying, which is exactly what happened. A trigger phrased on the
diagnostic rather than the count, such as "the pattern of failure is
identical across entries", would have fired.

**F3. Conjunct 2 is named compression and measures composition.**
Section 7.1. Any repair must separate "how much work does this verb save
in one runbook" from "how many purposes does this verb bundle", and
should probably combine breadth and per-runbook saving into a total
rather than gating on a per-runbook median.

**F4. The decoy test had little power against the predicate it was
paired with.** All seven decoys are single-purpose, and conjunct 2
admits only composites. The 0-of-7 result was close to structurally
determined. A decoy set intended to test a predicate must contain at
least one entry that the predicate could in principle admit.

**F5. The priors and the evidence corpus share an author.** The five
priors and the twenty runbooks came from the same party session. Blind
scoring does not address this. The confound the provenance file
documents for the word `reconcile` is one visible instance of a general
condition.

**F6. The audit's asymmetry ruling and amendment A3 are inconsistent on
where the surface-advantage correction applies.** The audit says
surface-advantaged baseline verbs are excluded "from the denominator";
A3 fixes the denominator as the runbook-derived set, which contains no
baseline verbs. Resolved at the gate by applying the correction to the
matchable pool, which is the only reading under which both hold.
Consequential only in that a scorer following the audit's wording alone
would have computed a different statistic.

---

## 10. What the exercise has produced, stated plainly

The gate opens, and it opens on a narrower finding than the exercise set
out to test.

The question was whether eliciting operator runbooks from role-cast
personas produces a better set of composite CLI verbs than reading the
vendor's API. On verbs, the answer so far is not established: the one
verb that cleared the predicate cleared a conjunct that only composites
can clear, and shares its name with three runbook titles written by the
same party that named it.

On something adjacent and larger, the answer is yes, and it is
measurable. The control arm read 10,038 type definitions and 721 root
fields and proposed seven verbs, every one of which operates inside
Wiz. The treatment arm found that thirteen of the twenty runbooks fail
at a boundary the schema does not describe, and that closing stave's own
read-surface debt removes 37 percent of the apparent work in the
graph-only class and 5 percent in the external-join class. No amount of
reading the vendor surface produces that, because the vendor surface
does not contain the CMDB, the GRC register, the ticketing export, the
account roster, or the cost data.

That is worth the fixtures and the class B runs. It is not yet worth a
verb.
