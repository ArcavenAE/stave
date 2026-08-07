# Verb scoring amendments (gate-scoped)

> **STOP if you are writing the paper pipelines (bd `aae-orc-e4jo.14`).**
> This file discusses the sealed control arm. Your reading is
> `verb-candidate-registration.md` sections 1 through 3, which are
> complete and self-sufficient for writing pipelines, plus
> `field-surface-audit.md`. Nothing here changes N, M, the glue kinds,
> the decoy set, or the blind scoring procedure.
>
> Read this at the gate (`.15`), in the mining (`.7`), or in the
> proposal (`.8`).

Amendments to `verb-candidate-registration.md`. Split out of that file on
2026-08-07, on the same day and before any measurement existed, because
the registration is required reading for `.14` and these amendments
necessarily describe the control arm.


Recorded here rather than in a ticket so the change is dated, visible in
git, and attached to the thing it amends.

### 2026-08-07, before any measurement exists

Made after the phase-0 artifacts landed (`.16` audit, `.12` baseline,
this registration) and **before** any paper pipeline, run, or score. No
number this predicate governs has been computed yet, which is the only
window in which amending it is legitimate.

**A1. The readings in section 5 did not map onto the gate's outcomes,
and one mapping was actively wrong.**

Section 5 names three readings of the predicate. bd `aae-orc-e4jo.15`
independently names three gate outcomes. They are on different axes:

| `.15` outcome | Meaning | Action |
|---|---|---|
| 1 | overlap with the sealed baseline is at or above threshold | stop |
| 2 | decoys score comparably to priors | stop |
| 3 | neither 1 nor 2 | continue |

Section 5's third reading, "neither set qualifies", falls through to
`.15` outcome 3 and would therefore route the exercise into fixtures,
judge invocations, and full mining. That is exactly backwards: if no verb
in either set clears the predicate, there is nothing to spend the
expensive half of the umbrella on.

**A fourth outcome is added:**

> **Outcome 4. The predicate qualifies nothing in either set.** Stop.
> Diagnose which of two causes holds before any rewrite: the predicate is
> too strict, or the paper pipelines are not detailed enough to score
> against. The diagnosis is made by inspecting whether pipelines recorded
> per-stage purposes at all, and is reported with the arithmetic. Do not
> loosen N or M as a first move; an unscoreable pipeline set is a `.14`
> defect and loosening the predicate would hide it.

Outcome 3 is correspondingly narrowed to "some verb qualifies, overlap is
below threshold, and the decoys do not score comparably."

**A2. Outcome 1 is evaluated against the corrected overlap, not the raw
overlap.**

Per the asymmetry ruling in `docs/design/field-surface-audit.md`, the
sealed baseline was written against the full vendored schema while the
paper pipelines are written against the much narrower curated surface.
Baseline verbs are therefore tagged `reachable` or `surface-advantaged`
at the gate, overlap is computed both ways, and **the threshold is
evaluated against the corrected figure** with the raw figure reported
beside it. If the two fall on opposite sides of the threshold, that fact
is the finding and neither number is quietly preferred.

**A3. The overlap metric's reading is fixed.**

The `.12` pre-registration says "overlaps this baseline by 80 percent or
more" without stating overlap of what over what. Fixed as
`|intersection| / |runbook-derived set|`, meaning "the runbook arm found
nothing the baseline did not", because that is the reading matching the
stated conclusion that the elicitation bought nothing. The alternative
reading, over the baseline, would let a runbook arm proposing many novel
verbs still score high by happening to cover the baseline.

**A4. Two name collisions must not be scored as matches.**

Both are recorded in section 2 and here, because a scorer working from
names alone would log false overlap:

- **Across arms.** The sealed baseline's verb 3 is named `coverage`. So
  is registered prior P3. They are different verbs: P3 is two streams and
  a key reporting the fraction that correspond; the baseline's is "is the
  estate actually being scanned", combining account inventory with scan
  recency and deployment health. Score on argument shape.
- **Within the decoy set.** D7 `diff --since` versus P4 `diff`, already
  ruled in section 2.

**A5. The scorer is told about the baseline's self-disclosed anchoring.**

The baseline's isolation attestation states plainly that `charter.md`, a
permitted input, names F4's placeholder composite verbs (`issue-triage`,
`vuln-exposure`, `posture-report`) and that this anchored four of its
seven proposals. The control arm is therefore not independent of the
charter. The scorer receives this and discounts agreement with those
three names accordingly. Note the placeholders are not the registered
priors, so the anchoring pulls the baseline toward F4 and not toward the
priors.

**A6. Outcome 1's action carries a dependency it did not have.**

`.15` outcome 1 says to adopt the sealed baseline as the v0.2 proposal.
Several baseline verbs are surface-advantaged, meaning they cannot be
built until `aae-orc-rsh6` (bind `cloudResourcesV2`), `aae-orc-j1xi`
(declare `filterBy` and `orderBy`), and `aae-orc-gs23` (bind the
aggregation, history, and diff roots) land. Adopting the baseline whole
on outcome 1 would produce a proposal that is not implementable as
written. The adoption must carry those dependencies explicitly rather
than reading as a ready verb set.

**What is NOT amended.** N stays 3. M stays 3, still the median. The six
glue kinds are unchanged. The blind-scoring procedure is unchanged. The
decoy set is unchanged. None of those was touched, and none may be
touched once a number exists.
