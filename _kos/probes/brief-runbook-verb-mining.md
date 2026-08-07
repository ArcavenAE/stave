# Exploration brief: runbook-derived verb mining for v0.2

bd `aae-orc-e4jo.2` (S0b). Written 2026-08-07.

> **Written after phases 0 through 3 ran, not before them.** The brief
> was scoped before the exercise was reframed, and the reframe reordered
> everything around it. Presenting it as though it preceded the work
> would misrepresent the record, so it is written as what it is: the
> brief for a probe already partly executed, recording the hypothesis as
> stated at the outset and what has already happened to it.

Question node: `question-runbook-derived-verb-bootstrap` at the
orchestrator. Charter frontier: F4.

---

## Hypothesis

As stated at the outset, so it can be judged against what followed:

> The verb set derived from operator runbooks differs materially from the
> verb set implied by the vendor's API surface, and the difference is
> concentrated in joins against external inputs rather than in richer
> queries.

Two clauses, and they have fared differently.

**Clause 2 has counter-evidence already.** The field-surface audit
(`docs/design/field-surface-audit.md`) found the dominant gap is richer
queries: zero runbook steps classify as blocked by Wiz, and the internal
blocks are unselected fields and unbound roots throughout. The sealed
baseline reached the same conclusion independently and put it first, that
the largest v0.2 improvement is widening the curated documents and it
produces no verbs at all. The paper pipelines then measured it: 15.1
percent of all glue stages are deleted by four field-selection tickets,
rising to 37 percent in the graph-only class.

**Clause 2 also has support, in the half the audit did not touch.** The
same tally deletes only 5 percent of the external-join class, and all
thirteen post-fix appearances of `join` are bridges to data outside Wiz.
So the difference is concentrated in external joins, and richer queries
are a large independent gap that the hypothesis did not anticipate. Both
are true and the hypothesis named only one.

**Clause 1 is what the gate decides**, and it is not yet answered.

## Method, as reframed

The original method was to attempt the runbooks and mine the audit trail.
A pre-run review found two defects: the plan could confirm its verb
candidates but had no way to fail, and its primary instrument could not
see the thing it was measuring, because the audit trail emits one line
per API call and nothing for the client-side work between calls, which is
exactly where a verb would live.

Reframed sequence, with the phase-0 audit inserted ahead of both arms:

0. **Audit the curated field surface** (`.16`). Separates what Wiz cannot
   do from what stave does not ask for, and rules on the surface
   asymmetry between the two arms. Done.
1. **Seal a vendor-surface baseline** with no runbook input (`.12`), the
   control arm. Committed before any treatment output existed. Done.
2. **Register priors, decoys, and the scoring predicate** (`.13`) before
   any evidence. Done.
3. **Write paper pipelines** for all 20 runbooks without executing them
   (`.14`), tallied raw and post-fix. Done.
4. **Execute the 5 class A runbooks** purely to commission the runlog and
   audit-trail join (`.5`). Not the evidence source.
5. **Compare 3 against 1 at the gate** (`.15`), and stop if it closes.

Everything expensive sits behind step 5.

## Timebox

Phase 0 through 3 cost four documents and a tally, no tenant contact and
no API budget. Behind the gate: roughly 8 to 14 hours, with 3 to 6 of
that in safety-gate round trips.

## Success signal

The pre-registered threshold from `.12`, and deliberately not novelty:

> If the runbook-derived verb set overlaps the sealed baseline by 80
> percent or more, measured over verb names plus argument shapes, the
> elicitation bought nothing and charter F4 as written was correct.

Amended at `docs/design/verb-scoring-amendments.md` to evaluate against
the corrected overlap, since the two arms read surfaces of different
widths.

An earlier draft of this brief used "at least one verb candidate is
killed and at least one unanticipated verb emerges" as the signal. That
is removed on purpose. It made novelty the success condition, which
pressures the run toward manufacturing a killed candidate. A
pre-registered numeric threshold cannot be satisfied by adjusting the
story afterwards.

## What would falsify the verb candidates

Registered before any evidence, in `docs/design/verb-candidate-registration.md`:

- **The decoy set.** Seven generic stream verbs with no runbook
  provenance, scored under the identical predicate. If they score
  comparably to the priors, the predicate measures plausibility rather
  than elicitation.
- **The scoring predicate**, with N and M fixed at registration.
- **The sealed baseline**, which fixes what "reading the API would have
  told you anyway" means before anyone can argue about it.

## What has already happened to them

Recorded here because a brief that omits results it already has is
decoration.

One of twelve entries qualified: the prior `reconcile`. Zero decoys
qualified. That reads as support for the priors until two things beside
it are read.

Eleven of twelve failed the compression conjunct identically, because a
verb that does one thing replaces one stage per runbook whatever its
breadth. Only a composite can clear the threshold, and `reconcile`
qualifies because it is registered as three operations in one verb. The
predicate is measuring composition, not compression.

And `reconcile` appears in three runbook titles. The single qualifier
shares its name with the word the catalogue uses for the runbooks it
qualified on.

Neither has been adjusted, because a measurement now exists and the
registration forbids tuning past that point. Both go to the gate.

## The method's own weakness, restated

Already named in `question-runbook-derived-verb-bootstrap` and not
solved by any of this: the same model family cast the personas, wrote the
runbooks, wrote the pipelines, and will score them. The baseline and the
decoys are two controls placed against it. Neither escapes it.

Two specific residues are now measured rather than feared. The baseline
disclosed that `charter.md`, a permitted input, anchored four of its
seven proposals on F4's placeholder verb names. The pipeline arm
disclosed that its substrate assumption is a shell pipeline, and that an
operator with the same data in a warehouse would push every join and
group into SQL and propose none of these verbs.

The second is the more serious of the two and it is not addressed
anywhere in the current design.
