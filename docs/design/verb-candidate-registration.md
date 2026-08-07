# Verb candidate registration and scoring predicate

bd `aae-orc-e4jo.13` (S0d). Registered 2026-08-07, **before** any paper
pipeline (`.14`), any run, and any scoring.

This document exists so the scoring predicate cannot be tuned to promote
the verbs the exercise already favours. Everything below is fixed at
registration. If the predicate turns out to be the wrong one, that is
outcome 2 of the gate (`.15`) and the correct response is to stop and
rewrite it in the open, not to adjust it mid-scoring.

---

## 1. The priors

Named by the same session that designed the exercise that will evaluate
them. Stated here unchanged from bd `aae-orc-e4jo`.

| # | Verb | Argument shape as registered |
|---|---|---|
| P1 | `join` | two streams, a key, and a mode: `inner`, `left-anti`, `right-anti` |
| P2 | `roll-up` | a stream, a grouping key, an aggregate |
| P3 | `coverage` | two streams and a key; reports the fraction that correspond |
| P4 | `diff` | two streams and a key; reports per-field disagreement |
| P5 | `reconcile` | two streams and a key; reports correspondence, absence in each direction, and disagreement together |

`join --left-anti` is recorded in the umbrella as the leading candidate.
That expectation is registered here so it is visible as a prior belief
rather than discovered as a conclusion.

## 2. The decoys

Drawn from generic stream-tool vocabulary with no runbook provenance.
Stated unchanged from the ticket.

| # | Verb | Argument shape as registered |
|---|---|---|
| D1 | `sort --by` | a stream, one or more field keys, direction |
| D2 | `dedupe` | a stream, a key or key set |
| D3 | `pivot` | a stream, a row key, a column key, a cell aggregate |
| D4 | `topn` | a stream, a rank field, a count |
| D5 | `explain` | a record or stream; reports how a derived value was arrived at |
| D6 | `watch` | a stream expression, an interval |
| D7 | `diff --since` | one stream at two times, a key |

**The decoys are the measurement, not filler.** If they score comparably
to the priors under the identical predicate, the predicate is measuring
"is this a plausible stream verb" rather than "did role-cast elicitation
find something", and the elicitation added nothing a generic vocabulary
would not have supplied.

**Some decoys may legitimately win.** `sort --by` and `topn` are real
gaps in a JSONL tool and may well earn their place. A decoy scoring well
is a result about the tool, not a defect in the decoy set. It is recorded
as a result.

### Name collision in the registered sets, ruled at registration

`D7 diff --since` shares a name with `P4 diff`. Since overlap and
scoring are defined over verb name **plus** argument shape, the two are
distinct entries and are scored separately. But the collision is a real
measurement hazard: any judgement that leans on name similarity will
credit D7's score to P4, or the reverse.

**Ruling.** D7 and P4 are scored as distinct entries. Any conclusion that
rests on D7 is reported separately and explicitly, and is never pooled
into a "decoys scored X" summary without that note. If D7 scores well and
P4 does not, that is evidence about temporal comparison as a capability
and is not evidence for the `diff` prior.

## 3. The scoring predicate

Applied identically to all twelve entries. Every input is computed from
`.14`'s paper pipelines, so the scoring is mechanical rather than a
matter of opinion.

A verb **qualifies** when all three conjuncts hold.

### Conjunct 1: breadth. Appears in **N = 3** or more runbooks.

Three of twenty is fifteen percent. A verb appearing in one or two
runbooks is bespoke to those runbooks and does not earn a place in the
verb set; a threshold much above three would reject verbs that are
genuinely load-bearing for a single class of the catalogue.

"Appears in" is decided from the paper pipeline for that runbook, not
from opinion: the verb appears if `.14`'s pipeline for that runbook uses
it, or is written with a marked gap where it would be used if it existed.
`.14` is instructed to mark those gaps regardless of whether the verb is
a prior or a decoy.

### Conjunct 2: compression. Collapses **M = 3** or more steps.

Counted per runbook as the number of stave invocations plus named glue
stages in that runbook's paper pipeline that the verb would replace. A
verb scores M for a runbook, and the qualifying M is its **median across
the runbooks where it appears**, not its maximum. The median is chosen
because a maximum rewards one spectacular case and a mean is dragged by
it.

Three is set rather than two because two is the arithmetic minimum for
the word "collapse" to mean anything, and a bar at the arithmetic minimum
is not a bar.

### Conjunct 3: absorption. Absorbs client-side glue of a named kind.

The kinds are enumerated here so the conjunct is falsifiable. A verb
satisfies conjunct 3 when the glue it absorbs is of at least one kind
below, named explicitly in the scoring:

| Kind | Definition |
|---|---|
| G1 correspondence | matching records across two streams on a key |
| G2 aggregation | grouping records and computing a per-group value |
| G3 set difference | presence or absence of a key between two streams |
| G4 temporal comparison | the same logical record compared at two times |
| G5 derivation | computing a field from other fields across a stream |
| G6 ordering and selection | ranking a stream and taking a subset |

A verb that absorbs no glue of any named kind fails conjunct 3 even if it
is convenient. If a verb absorbs glue that fits none of G1 to G6, the
kind is added to this table **with a note that it was added during
scoring**, and the addition is reported at the gate. Adding a kind
silently is the failure mode this enumeration exists to prevent.

### Scoring is blind to set membership

The scorer receives all twelve entries as one shuffled list with the
prior and decoy labels stripped, scores them, and the labels are
reattached afterwards. The mapping is held in this document and not given
to the scorer.

This costs nothing and removes the most obvious route by which a prior
gets the benefit of the doubt.

## 4. Recorded alongside the score, not part of it

For each of the twelve, record whether the Wiz API already exposes a
server-side root field that does the same job. Established by the field
surface audit (`docs/design/field-surface-audit.md`, bd
`aae-orc-e4jo.16`): `issuesGroupedByValue` and its siblings, the `*Trend`
and `*HistoryEvents` roots, `securityFrameworksDiff`, and the
`filterBy` and `orderBy` arguments stave does not currently pass.

This is **not** a fourth conjunct and does not disqualify a verb. A stave
verb that fans out across kinds and emits one stream is not the same
object as a single server root field. But a verb whose entire value is
client-side re-implementation of an available server field is a different
proposition from one with no server analogue, and the gate needs to see
which is which.

It applies identically to both sets. At least P2 `roll-up` and P4 `diff`
have server analogues, and so do D3 `pivot` and D7 `diff --since`.

## 5. What a result looks like

Reported at the gate as one table of twelve rows: verb, argument shape,
runbook count, median compression, glue kinds absorbed, qualifies yes or
no, server analogue yes or no. Set membership is a column added after
scoring.

Three readings are pre-named:

- **Priors qualify, decoys largely do not.** The elicitation surfaced
  something a generic stream vocabulary would not have. The exercise did
  what it claimed.
- **Decoys qualify comparably.** The predicate is measuring plausibility,
  not elicitation. This is outcome 2 of the gate and the response is to
  stop and rewrite the predicate in the open.
- **Neither set qualifies.** The predicate is too strict, or the paper
  pipelines are not detailed enough to score against. Also a stop, and
  the diagnosis is which of the two before any rewrite.

None of the three is a failure of the exercise. The exercise fails only
if the predicate is adjusted after the numbers are visible.

---

## Amendments

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
