# finding-003: six pre-registration defects, found by running the pre-registration

Date: 2026-08-07
Probe: `_kos/probes/brief-runbook-verb-mining.md`, bd `aae-orc-e4jo`
Artifacts: `docs/design/verb-candidate-registration.md`,
`docs/design/verb-scoring-amendments.md`,
`docs/design/verb-comparison-gate.md`

## What happened

An experiment was pre-registered to test whether eliciting operator
runbooks from role-cast personas produces better CLI verbs than reading
the vendor's API. It carried three controls: a sealed control arm
committed before any treatment output existed, a decoy set of seven
generic verbs with no runbook provenance, and a scoring predicate with
its thresholds fixed before any evidence.

The gate ruled outcome 3, continue, on a narrower finding than the
experiment set out to test. That result is in
`docs/design/verb-comparison-gate.md` and is not repeated here.

**This finding is about the six defects the pre-registration turned out
to have.** All six were invisible on paper and became obvious the moment
numbers existed. They are recorded because the defects transfer to any
future pre-registration in this repository or elsewhere; the verb result
does not.

## The six

**1. A ratio was registered without defining one of its sides.** The
threshold read "if the runbook-derived verb set overlaps this baseline by
80 percent or more". The direction was later fixed by amendment
(`|intersection| / |runbook-derived set|`). Nothing ever defined what the
runbook-derived set WAS. Three defensible readings existed at the gate:
the five registered priors, the entries that qualified under the
predicate, and the verbs the pipelines actually demanded. They gave
different denominators and happened to agree on the ruling here, which is
luck rather than design.

*Rule: name both sides of every registered ratio, and its denominator
explicitly, not just its direction.*

**2. A stop-condition was phrased on a count when its diagnosis was a
pattern.** A fourth gate outcome was added to catch "the predicate is
broken", triggered on "the predicate qualifies nothing in either set".
Exactly one entry qualified, so it did not fire, while the situation it
was written for was substantially present. A trigger phrased on the
diagnostic, such as "the pattern of failure is identical across
entries", would have fired.

*Rule: phrase a stop-condition on the thing you are worried about, not
on a count that usually accompanies it.*

**3. A conjunct was named for one property and measured another.** The
predicate's second conjunct was called compression and defined as the
median number of stages a verb replaces per runbook. A verb that does one
thing replaces one stage per runbook however broad it is, so a median of
three can only be cleared by a verb that bundles several operations. It
measured composition. The single entry that qualified had been registered
as three operations under one name.

*Rule: before fixing a threshold, ask what shape of candidate can clear
it at all. If the answer is a subset of the candidates, the threshold is
selecting on that property and should be named for it.*

**4. The control had no power against the predicate it was paired
with.** All seven decoys were single-purpose stream verbs. Given defect
3, no decoy could have qualified regardless of merit. The observed zero
of seven was close to structurally determined, so its non-firing carried
much less evidential weight than the number suggests. The control looked
like it was working and was not testing anything.

*Rule: a control set must contain at least one member the test could in
principle admit. Check that before registering, by asking what it would
take for a control to pass.*

**5. The hypothesis and its ground truth shared an author.** The five
verb candidates and the twenty runbooks they were scored against came
from the same party session. Blind scoring was implemented and protects
against scorer bias; it does nothing about this. One visible instance:
the single qualifying verb, `reconcile`, appears in three of the five
runbook titles it scored on. The decoys were the only genuinely exogenous
input, which is what made defect 4 expensive.

*Rule: record who authored the candidates and who authored the corpus.
If it is the same party, blind scoring is not the control you need.*

**6. Two documents stated the same correction incompatibly.** The field
surface audit said surface-advantaged verbs are excluded "from the
denominator"; the amendment fixed the denominator as a set containing no
such verbs. Both were written the same day by the same author. The gate
resolved it by applying the correction to the matchable pool, the only
reading under which both hold.

*Rule: when a correction is stated in two places, state it once and
reference it.*

## Two process observations worth keeping

**The pre-registration worked, and the clearest evidence is a decision
not to score.** The treatment arm found that an alternative stage
decomposition would move the leading registered candidate over the
threshold, declined to adopt it, and said why: adopting a convention
after seeing that it promotes the expected answer is the tuning the
registration exists to prevent. The gate agreed and showed the ruling was
unchanged either way. A registration that is only ever satisfied is
decoration; this one was load-bearing at the moment it cost something.

**Defects 1, 2, and 6 were introduced by the same person who wrote the
controls, on the same day, while fixing other defects.** Amendments made
in good faith to strengthen a pre-registration introduced three new holes
in it. This is not an argument against amending. It is an argument for
having the gate be a party who did not write the registration, which is
the only reason all six were found.

## Actions

- The measurement stands as measured. The gate explicitly refused a
  re-score under a repaired predicate: a repaired predicate is a fresh
  registration for a future question, not a second run at this one.
- A repaired predicate, should the question be asked again, must separate
  per-runbook saving from purpose-bundling and should probably combine
  breadth and saving into a total rather than gating on a per-runbook
  median.
- Filed as bd tickets from this finding: the repaired-predicate
  registration, and the harvest of the gate's answer to sub-question A
  of `question-runbook-derived-verb-bootstrap` at the orchestrator.

## Cross-references

- `docs/design/verb-comparison-gate.md` sections 7 and 9, where five of
  the six were first stated
- `docs/runbooks/paper-pipelines.md`, honest-limits section, which
  reported defects 3 and 5 without the inputs to see their full extent
- `docs/runbooks/catalogue-provenance.md`, which recorded the editorial
  ruling behind defect 5's visible instance and instructed the gate to
  discount convergence accordingly
- orc `question-runbook-derived-verb-bootstrap`, which named the
  closed-loop weakness before any of this ran
